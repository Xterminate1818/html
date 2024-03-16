use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum ParseError {
  InvalidTag,
  MismatchedClosing { expected: String, found: String },
  UnmatchedOpen(String),
  UnmatchedClose(String),
  VoidClosingTag(String),
  Unknown,
}

use crate::trace::{self, WithContext};

impl From<ParseError> for trace::Error {
  fn from(value: ParseError) -> Self {
    let msg = match value {
      ParseError::InvalidTag => "Failed to parse a tag".into(),
      ParseError::MismatchedClosing { expected, found } => {
        format!(
          "Found closing tag '{}' where '{}' was expected",
          found, expected
        )
      },
      ParseError::UnmatchedOpen(s) => {
        format!("The tag '{}' is opened, but never closed", s)
      },
      ParseError::UnmatchedClose(s) => {
        format!("The tag '{}' is closed, but never opened", s)
      },
      ParseError::VoidClosingTag(s) => {
        format!("The tag '{}' should not have a closing tag", s)
      },
      ParseError::Unknown => {
        return trace::Error::new(
          trace::ErrorKind::Unknown,
          "Unknown error while parsing",
        )
      },
    };
    trace::Error::new(trace::ErrorKind::Parsing, msg)
  }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lexeme<'a> {
  OpenTag {
    name: &'a str,
    attributes: HashMap<&'a str, Option<&'a str>>,
    is_void: bool,
  },
  CloseTag {
    name: &'a str,
  },
  Text(&'a str),
  Doctype,
  Comment,
}

fn normalize_whitespace(mut tail: &str) -> String {
  // https://developer.mozilla.org/en-US/docs/Web/API/Document_Object_Model/Whitespace
  let mut _index = 0;
  let mut buffer = String::with_capacity(tail.len());
  while !tail.is_empty() {
    match parse_whitespace_min(tail, 1, &mut _index) {
      Some((_, new_tail)) => {
        buffer.push(' ');
        tail = new_tail;
      },
      None => {},
    }
    let (chars, new_tail) =
      parse_while(tail, |c| !c.is_whitespace(), &mut _index);
    buffer.push_str(chars);
    tail = new_tail
  }
  buffer
}

/// Try parsing single specific character ignoring case
fn parse_char<'a>(
  tail: &'a str,
  c: char,
  index: &mut usize,
) -> Option<(&'a str, &'a str)> {
  if !tail.is_empty() && tail[0..1].eq_ignore_ascii_case(&c.to_string()) {
    *index += 1;
    Some((&tail[0..1], &tail[1..]))
  } else {
    None
  }
}

fn parse_str<'a>(
  tail: &'a str,
  to_match: &'a str,
  index: &mut usize,
) -> Option<(&'a str, &'a str)> {
  if tail.len() < to_match.len() {
    return None;
  }
  if tail[0..to_match.len()].eq_ignore_ascii_case(to_match) {
    *index += to_match.len();
    Some((&tail[0..to_match.len()], &tail[to_match.len()..]))
  } else {
    None
  }
}

fn parse_until_str<'a>(
  tail: &'a str,
  to_match: &'a str,
  index: &mut usize,
) -> Option<(&'a str, &'a str)> {
  for i in 0..tail.len() {
    let substr = &tail[0..i];
    if substr.ends_with(to_match) {
      *index += i;
      return Some((&tail[0..i], &tail[i..]));
    }
  }
  None
}

/// Parse until condition is not true for next character
fn parse_while<'a>(
  tail: &'a str,
  condition: impl Fn(char) -> bool,
  index: &mut usize,
) -> (&'a str, &'a str) {
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
  *index += end;
  (&tail[0..end], &tail[end..])
}

fn parse_whitespace<'a>(i: &'a str, index: &mut usize) -> (&'a str, &'a str) {
  parse_while(i, |c| c.is_whitespace(), index)
}

fn parse_whitespace_min<'a>(
  tail: &'a str,
  min: usize,
  index: &mut usize,
) -> Option<(&'a str, &'a str)> {
  let mut new_index = 0;
  let (ws, tail) = parse_whitespace(tail, &mut new_index);
  if ws.len() < min {
    None
  } else {
    *index += new_index;
    Some((ws, tail))
  }
}

/// Try parsing all characters between two delimiter
/// characters
fn parse_delimited<'a>(
  i: &'a str,
  delimiter: char,
  index: &mut usize,
) -> Option<(&'a str, &'a str)> {
  let mut new_index = 0;
  let (_, tail) = parse_char(i, delimiter, &mut new_index)?;
  let (value, tail) = parse_while(tail, |c| c != delimiter, &mut new_index);
  let (_, tail) = parse_char(tail, delimiter, &mut new_index)?;
  *index += new_index;
  Some((value, tail))
}

fn parse_tag_name<'a>(
  i: &'a str,
  index: &mut usize,
) -> Option<(&'a str, &'a str)> {
  let mut new_index = 0;
  let (value, tail) = parse_while(
    i,
    |c| c.is_ascii_alphanumeric() || [':', '_', '-'].contains(&c),
    &mut new_index,
  );
  if value.is_empty() {
    None
  } else {
    *index += new_index;
    Some((value, tail))
  }
}

fn parse_attribute_key<'a>(
  i: &'a str,
  index: &mut usize,
) -> Option<(&'a str, &'a str)> {
  let mut new_index = 0;
  let (value, tail) = parse_while(
    i,
    |c| {
      !(['"', '\'', '>', '/', '='].contains(&c)
        || c.is_control()
        || c.is_whitespace())
    },
    &mut new_index,
  );
  if value.is_empty() {
    None
  } else {
    *index += new_index;
    Some((value, tail))
  }
}

fn parse_attribute_val<'a>(
  i: &'a str,
  index: &mut usize,
) -> Option<(&'a str, &'a str)> {
  const SINGLE_QUOTE: char = '\'';
  const DOUBLE_QUOTE: char = '"';
  let mut new_index = 0;
  let (value, tail) =
    parse_delimited(i, SINGLE_QUOTE, &mut new_index) // Single quote delimit
    .or_else(|| parse_delimited(i, DOUBLE_QUOTE, &mut new_index)) // Double quote delimit
    .or_else(|| { // Unquoted
      Some(parse_while(i, |c| {
        !(c.is_whitespace()
          || [SINGLE_QUOTE, DOUBLE_QUOTE, '=', '<', '>', '`'].contains(&c))
      }, &mut new_index))
    })?;
  *index += new_index;
  Some((value, tail))
}

/// Returns Option<((key, value), tail)>
fn parse_key_val<'a>(
  tail: &'a str,
  index: &mut usize,
) -> Option<((&'a str, Option<&'a str>), &'a str)> {
  let mut new_index = 0;
  // Require whitespace
  let (_, tail) = parse_whitespace_min(tail, 1, &mut new_index)?;
  // Fail when no key found
  let (key, tail) = parse_attribute_key(tail, &mut new_index)?;
  if let Some((_, tail)) = parse_char(
    parse_whitespace(tail, &mut new_index).1,
    '=',
    &mut new_index,
  ) {
    let (_, tail) = parse_whitespace(tail, &mut new_index);
    // Fail when = is not followed by value
    let (val, tail) = parse_attribute_val(tail, &mut new_index)?;
    let val = if val.is_empty() { None } else { Some(val) };
    *index += new_index;
    Some(((key, val), tail))
  } else {
    *index += new_index;
    Some(((key, None), tail))
  }
}

// Tags that are implicitly self closing, ending in /> is optional
const VOID_ELEMENTS: [&str; 16] = [
  "area", "base", "br", "col", "command", "embed", "hr", "img", "input",
  "keygen", "link", "meta", "param", "source", "track", "wbr",
];

fn parse_open_tag<'a>(
  tail: &'a str,
  index: &mut usize,
) -> Result<(Lexeme<'a>, &'a str), ParseError> {
  let mut new_index = 0;
  // <
  let (_, tail) =
    parse_char(tail, '<', &mut new_index).ok_or(ParseError::Unknown)?;
  // tag name
  let (name, mut tail) =
    parse_tag_name(tail, &mut new_index).ok_or(ParseError::InvalidTag)?;
  // attributes
  let mut attributes: HashMap<&str, Option<&str>> = HashMap::new();
  while let Some((kv, new_tail)) = parse_key_val(tail, &mut new_index) {
    attributes.insert(kv.0, kv.1);
    tail = new_tail;
  }
  let (_, tail) = parse_whitespace(tail, &mut new_index);
  let (is_void, tail) =
    parse_char(tail, '/', &mut new_index).unwrap_or(("", tail));
  let is_void = !is_void.is_empty() || VOID_ELEMENTS.contains(&name);
  let (_, tail) = match parse_char(tail, '>', &mut new_index) {
    Some(v) => v,
    None => {
      return Err(ParseError::InvalidTag);
    },
  };
  *index += new_index;
  Ok((
    Lexeme::OpenTag {
      name,
      attributes,
      is_void,
    },
    tail,
  ))
}

fn parse_close_tag<'a>(
  tail: &'a str,
  index: &mut usize,
) -> Result<(Lexeme<'a>, &'a str), ParseError> {
  let mut new_index = 0;
  let (_, tail) =
    parse_char(tail, '<', &mut new_index).ok_or(ParseError::Unknown)?;
  let (_, tail) =
    parse_char(tail, '/', &mut new_index).ok_or(ParseError::Unknown)?;
  let (name, tail) =
    parse_tag_name(tail, &mut new_index).ok_or(ParseError::InvalidTag)?;
  let (_, tail) = parse_whitespace(tail, &mut new_index);
  let (_, tail) =
    parse_char(tail, '>', &mut new_index).ok_or(ParseError::InvalidTag)?;
  *index += new_index;
  Ok((Lexeme::CloseTag { name }, tail))
}

fn parse_doctype<'a>(
  tail: &'a str,
  index: &mut usize,
) -> Result<(Lexeme<'a>, &'a str), ParseError> {
  let mut new_index = 0;
  let mut closure = || -> Option<(&str, &str)> {
    let (_, tail) = parse_str(tail, "<!doctype", &mut new_index)?;
    let (_, tail) = parse_whitespace_min(tail, 1, &mut new_index)?;
    let (_, tail) = parse_str(tail, "html", &mut new_index)?;
    let (_, tail) = parse_whitespace(tail, &mut new_index);
    parse_char(tail, '>', &mut new_index)
  };
  let (_, tail) = closure().ok_or(ParseError::Unknown)?;
  *index += new_index;
  Ok((Lexeme::Doctype, tail))
}

fn parse_comment<'a>(
  tail: &'a str,
  index: &mut usize,
) -> Result<(Lexeme<'a>, &'a str), ParseError> {
  let mut new_index = 0;
  let (_, tail) =
    parse_str(tail, "<!--", &mut new_index).ok_or(ParseError::Unknown)?;
  let (_, tail) =
    parse_until_str(tail, "-->", &mut new_index).ok_or(ParseError::Unknown)?;
  *index += new_index;
  Ok((Lexeme::Comment, tail))
}

fn parse_text<'a>(
  tail: &'a str,
  index: &mut usize,
) -> Result<(Lexeme<'a>, &'a str), ParseError> {
  let mut new_index = 0;
  let (txt, tail) = parse_while(tail, |c| c != '<', &mut new_index);
  if txt.is_empty() {
    Err(ParseError::Unknown)
  } else {
    *index += new_index;
    Ok((Lexeme::Text(txt), tail))
  }
}

fn or_keep_error<'a>(
  r: Result<(Lexeme<'a>, &'a str), ParseError>,
  op: impl FnOnce() -> Result<(Lexeme<'a>, &'a str), ParseError>,
) -> Result<(Lexeme<'a>, &'a str), ParseError> {
  match r {
    Ok(val) => Ok(val),
    Err(e) => op().map_err(|new_e| match e {
      ParseError::Unknown => new_e,
      _ => e,
    }),
  }
}

fn index_to_rc(input: &str, index: usize) -> (usize, usize) {
  let (mut row, mut col) = (1, 1);
  for c in input[0..index].chars() {
    if c == '\n' {
      row += 1;
      col = 1;
    } else {
      col += 1;
    }
  }
  (row, col)
}

pub fn parse_html(input: &str) -> trace::Result<Vec<Lexeme>> {
  let mut tail = input;
  let mut lexeme_stack = vec![];
  let mut validation_stack = vec![];
  let mut index = 0;

  let err = |error: ParseError, index: usize| -> trace::Result<Vec<Lexeme>> {
    let (row, col) = index_to_rc(input, index);
    let e: trace::Error = error.into();
    Err(e).ctx(format!("Starting at line {} character {}", row, col))
  };

  while !tail.is_empty() {
    let (_, new_tail) = parse_whitespace(tail, &mut index);
    if new_tail.is_empty() {
      break;
    }
    let result = or_keep_error(parse_open_tag(new_tail, &mut index), || {
      parse_close_tag(new_tail, &mut index)
    });
    let result = or_keep_error(result, || parse_text(new_tail, &mut index));
    let result = or_keep_error(result, || parse_comment(new_tail, &mut index));
    let (lm, new_tail) =
      match or_keep_error(result, || parse_doctype(new_tail, &mut index)) {
        Ok(v) => v,
        Err(e) => {
          return err(e, index);
        },
      };
    // Validate that open and close tags match
    match lm {
      Lexeme::OpenTag { name, is_void, .. } => {
        if !is_void {
          validation_stack.push(name);
        }
      },
      Lexeme::CloseTag { name } => {
        if VOID_ELEMENTS.contains(&name) {
          return err(ParseError::VoidClosingTag(name.into()).into(), index);
        }
        if let Some(top) = validation_stack.pop() {
          if name != top {
            return err(
              ParseError::MismatchedClosing {
                expected: top.into(),
                found: name.into(),
              },
              index,
            );
          }
        } else {
          return err(ParseError::UnmatchedClose(name.into()), index);
        }
      },
      Lexeme::Comment => {
        tail = new_tail;
        continue;
      },
      _ => {},
    };

    lexeme_stack.push(lm);

    tail = new_tail;
  }
  if let Some(top) = validation_stack.pop() {
    let e: trace::Error = ParseError::UnmatchedOpen(top.into()).into();
    return Err(e).ctx("At end of file");
  }
  Ok(lexeme_stack)
}
