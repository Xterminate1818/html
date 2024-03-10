use std::collections::HashMap;

use crate::trace::*;

#[derive(Clone, Debug)]
pub enum Lexeme<'a> {
  OpenTag {
    name: &'a str,
    attributes: HashMap<&'a str, Option<&'a str>>,
    is_void: bool,
  },
  CloseTag {
    name: &'a str,
  },
  Content(&'a str),
}

fn normalize_whitespace(s: &str) {
  // https://developer.mozilla.org/en-US/docs/Web/API/Document_Object_Model/Whitespace
  todo!()
}

fn error(message: impl Into<String>) -> Error {
  Error {
    kind: ErrorKind::Parsing,
    reason: message.into(),
    backtrace: vec![],
  }
}

/// Try parsing single specific character
fn parse_char(i: &str, c: char) -> Option<(&str, &str)> {
  if i.starts_with(c) {
    Some((&i[0..1], &i[1..]))
  } else {
    None
  }
}

// Parse until condition is not true for next character
fn parse_while(tail: &str, condition: impl Fn(char) -> bool) -> (&str, &str) {
  let mut end;
  let mut it = tail.char_indices();
  'outer: loop {
    match it.next() {
      Some((i, c)) => {
        end = i;
        if !condition(c) {
          break 'outer;
        }
      },
      None => {
        // Reached end of input
        return (&tail, "");
      },
    };
  }
  (&tail[0..end], &tail[end..])
}

fn parse_whitespace(i: &str) -> (&str, &str) {
  parse_while(i, |c| c.is_whitespace())
}

fn parse_doctype(tail: &str) -> Option<(&str, &str)> {
  const doctype_str = "<!DOCTYPE>"
}

/// Try parsing all characters between two delimiter
/// characters
fn parse_delimited(i: &str, delimiter: char) -> Option<(&str, &str)> {
  let (_, tail) = parse_char(i, delimiter)?;
  let (value, tail) = parse_while(tail, |c| c != delimiter);
  let (_, tail) = parse_char(tail, delimiter)?;
  Some((value, tail))
}

fn parse_tag_name(i: &str) -> Option<(&str, &str)> {
  let (value, tail) = parse_while(i, |c| c.is_ascii_alphanumeric() || c == ':');
  if value.is_empty() {
    None
  } else {
    Some((value, tail))
  }
}

fn parse_attribute_key(i: &str) -> Option<(&str, &str)> {
  let (value, tail) = parse_while(i, |c| {
    !(['"', '\'', '>', '/', '='].contains(&c) || c.is_control())
  });
  if value.is_empty() {
    None
  } else {
    Some((value, tail))
  }
}

fn parse_attribute_val(i: &str) -> Option<(&str, &str)> {
  const SINGLE_QUOTE: char = '\'';
  const DOUBLE_QUOTE: char = '"';
  let (value, tail) = parse_delimited(i, '\'') // Single quote delimit
    .or_else(|| parse_delimited(i, '"')) // Double quote delimit
    .or_else(|| { // Unquoted
      Some(parse_while(i, |c| {
        !(c.is_whitespace()
          || [SINGLE_QUOTE, DOUBLE_QUOTE, '=', '<', '>', '`'].contains(&c))
      }))
    })?;
  if value.is_empty() {
    None
  } else {
    Some((value, tail))
  }
}

/// Returns Option<((key, value), tail)>
fn parse_key_val(tail: &str) -> Option<((&str, Option<&str>), &str)> {
  // Require whitespace
  let (ws, tail) = parse_whitespace(tail);
  if ws.is_empty() {
    return None;
  }
  // Fail when no key found
  let (key, tail) = parse_attribute_key(tail)?;
  let (_, tail) = parse_whitespace(tail);
  if let Some((_, tail)) = parse_char(tail, '=') {
    let (_, tail) = parse_whitespace(tail);
    // Fail when = is not followed by value
    let (val, tail) = parse_attribute_val(tail)?;
    Some(((key, Some(val)), tail))
  } else {
    Some(((key, None), tail))
  }
}

const VOID_ELEMENTS: [&str; 16] = [
  "area", "base", "br", "col", "command", "embed", "hr", "img", "input",
  "keygen", "link", "meta", "param", "source", "track", "wbr",
];

fn parse_open_tag(tail: &str) -> Option<(Lexeme, &str)> {
  // <
  let (_, tail) = parse_char(tail, '<')?;
  // tag name
  let (name, mut tail) = parse_tag_name(tail)?;
  // attributes
  let mut attributes: HashMap<&str, Option<&str>> = HashMap::new();
  while let Some((kv, new_tail)) = parse_key_val(tail) {
    attributes.insert(kv.0, kv.1);
    tail = new_tail;
  }
  let (_, tail) = parse_whitespace(tail);
  let (is_void, tail) = parse_char(tail, '/').unwrap_or(("", tail));
  let is_void = !is_void.is_empty() || VOID_ELEMENTS.contains(&name);
  let (_, tail) = parse_char(tail, '>')?;
  Some((
    Lexeme::OpenTag {
      name,
      attributes,
      is_void,
    },
    tail,
  ))
}

fn parse_close_tag(tail: &str) -> Option<(Lexeme, &str)> {
  let (_, tail) = parse_char(tail, '<')?;
  let (_, tail) = parse_char(tail, '/')?;
  let (name, tail) = parse_tag_name(tail)?;
  let (_, tail) = parse_whitespace(tail);
  let (_, tail) = parse_char(tail, '>')?;
  Some((Lexeme::CloseTag { name }, tail))
}

fn parse_text(tail: &str) -> Option<(Lexeme, &str)> {
  let (txt, tail) = parse_while(tail, |c| c != '<');
  if txt.is_empty() {
    None
  } else {
    Some((Lexeme::Content(txt), tail))
  }
}

pub fn parse_html(mut tail: &str) -> Vec<Lexeme> {
  let mut stack = vec![];
  while !tail.is_empty() {
    let (_, new_tail) = parse_whitespace(tail);
    if new_tail.is_empty() {
      break;
    }
    let (lm, new_tail) = parse_open_tag(new_tail)
      .or_else(|| parse_close_tag(new_tail))
      .or_else(|| parse_text(new_tail))
      .unwrap();
    stack.push(lm);
    tail = new_tail;
  }
  stack
}
