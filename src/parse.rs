use std::collections::HashMap;

use crate::directives::expand_directive;

pub type Attributes = HashMap<String, String>;
pub type Offset = usize;
pub type Parse<'a, T> = (T, &'a str, Offset);
pub type MaybeParse<'a, T> = Option<Parse<'a, T>>;

#[inline]
fn parse_until(i: &str, condition: impl Fn(char) -> bool) -> Parse<&str> {
  match i.chars().position(condition) {
    Some(pos) => (&i[..pos], &i[pos..], pos),
    None => (&i, "", i.len()),
  }
}

fn parse_until_str<'a>(
  tail: &'a str,
  to_match: &'static str,
) -> MaybeParse<'a, &'a str> {
  for i in 0..tail.len() {
    let substr = &tail[0..i];
    if substr.ends_with(to_match) {
      let end = i - to_match.len();
      return Some((&tail[0..end], &tail[end..], end));
    }
  }
  None
}

#[inline]
fn parse_str<'a>(i: &'a str, matches: &str) -> MaybeParse<'a, &'a str> {
  let length = matches.len();
  match i.get(0..matches.len()) {
    Some(s) => {
      if s.eq_ignore_ascii_case(matches) {
        Some((s, &i[length..], length))
      } else {
        None
      }
    },
    None => None,
  }
}

fn parse_char(i: &str, matches: char) -> MaybeParse<char> {
  if let Some(c) = i.chars().next() {
    if c == matches {
      return Some((c, &i[1..], 1));
    }
  }
  None
}

fn parse_delimited(i: &str, delim: char) -> MaybeParse<&str> {
  let (_start, i, o1) = parse_char(i, delim)?;
  let (contents, i, o2) = parse_until(i, |c| c == delim);
  let (_end, i, o3) = parse_char(i, delim)?;
  Some((contents, i, o1 + o2 + o3))
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

pub const LEXEME_MEMORY_LIMIT: usize = 65535;

// Tags that are implicitly self closing, ending in /> is
// optional
const VOID_ELEMENTS: [&str; 16] = [
  "area", "base", "br", "col", "command", "embed", "hr", "img", "input",
  "keygen", "link", "meta", "param", "source", "track", "wbr",
];

#[derive(Clone, Debug)]
pub enum ErrorKind {
  /// Encountered illegal sequence
  Illegal,
  /// Closing tag does not match previous opening tag
  UnbalancedTags,
  /// Tried to parse `N > 65535` elements
  MemoryLimit,
}

impl From<Error> for crate::trace::Error {
  fn from(value: Error) -> Self {
    Self {
      kind: crate::trace::ErrorKind::Parsing,
      reason: match value.kind {
        ErrorKind::Illegal => "Illegal character encountered",
        ErrorKind::UnbalancedTags => "Unbalanced open and close tags",
        ErrorKind::MemoryLimit => "Ran out of memory",
      }
      .to_string(),
      backtrace: vec![format!("at line {} column {}", value.row, value.column)],
    }
  }
}

#[derive(Clone, Debug)]
pub struct Error {
  pub kind: ErrorKind,
  pub char_index: usize,
  pub row: usize,
  pub column: usize,
}

#[derive(Clone, Debug)]
pub enum HtmlElement {
  /// The required `<!DOCTYPE HTML>` preamble
  DocType,
  /// Text inside of a `<!-- comment -->`
  Comment(String),
  /// Any opening tag, including <empty/> tags
  OpenTag {
    /// The name of the tag
    name: String,
    /// Attribute names and their values if present
    attributes: Attributes,
    /// Whether the tag closes itself. Tags that are
    /// implicitly empty are:
    /// `area, base, br, col, command, embed, hr, img, input
    /// keygen, link, meta, param, source, track, wbr`
    is_empty: bool,
  },
  /// Any closing tag that is not empty. Closing implicitly
  /// empty tags is an error
  CloseTag { name: String },
  /// Any inner text that is not entirely whitespace
  Text(String),
  /// A `<script>` tag and its contents
  Script {
    /// Attribute names and their values if present
    attributes: Attributes,
    /// The raw text between the open and close `<script>`
    /// tags
    contents: String,
  },
  /// A `<style>` tag and its contents
  Style {
    /// Attribute names and their values if present
    attributes: Attributes,
    /// The raw text between the open and close `<style>`
    /// tags
    contents: String,
  },
  Directive {
    name: String,
    attributes: Attributes,
    contents: String,
  },
}
fn serialize_attributes(attr: &Attributes) -> String {
  attr
    .iter()
    .map(|(k, v)| {
      if v.is_empty() {
        format!(" {k}")
      } else {
        format!(" {k}=\"{v}\"")
      }
    })
    .collect::<Vec<_>>()
    .join("")
}

impl HtmlElement {
  pub fn serialize(&self) -> String {
    match self {
      Self::DocType => "<!DOCTYPE html>".into(),
      Self::Comment(_) => format!("<!---->"),
      Self::OpenTag {
        name,
        attributes,
        is_empty: false,
      } => format!("<{}{}>", name, serialize_attributes(&attributes)),
      Self::OpenTag {
        name,
        attributes,
        is_empty: true,
      } => format!("<{}{}/>", name, serialize_attributes(&attributes)),
      Self::CloseTag { name } => format!("</{}>", name),
      Self::Style {
        attributes,
        contents,
      } => {
        format!(
          "<style{}>\n{}\n</style>",
          serialize_attributes(attributes),
          contents
        )
      },
      Self::Script {
        attributes,
        contents,
      } => {
        format!(
          "<script{}>\n{}\n</script>",
          serialize_attributes(attributes),
          contents
        )
      },
      Self::Text(t) => t.clone(),
      Self::Directive {
        name,
        attributes,
        contents,
      } => expand_directive(&name, &attributes, &contents),
    }
  }
}

/// Used as the `condition` argument for `parse_until` to
/// parse names of things
const NAME_REGEX: fn(char) -> bool =
  |c| !(c.is_ascii_alphanumeric() || [':', '_', '-', '@'].contains(&c));

/// Used as the `condition` argument for `parse_until` to
/// arbitrary whitespace
const WS_REGEX: fn(char) -> bool = |c| !c.is_whitespace();

fn parse_doctype(i: &str) -> MaybeParse<HtmlElement> {
  let (_, i, o1) = parse_str(i, "<!doctype")?;
  let (_, i, o2) = parse_until(i, WS_REGEX);
  // Minimum of 1 whitespace
  if o2 == 0 {
    return None;
  }
  let (_, i, o3) = parse_str(i, "html")?;
  let (_, i, o4) = parse_until(i, WS_REGEX);
  let (_, i, o5) = parse_str(i, ">")?;
  Some((HtmlElement::DocType, i, o1 + o2 + o3 + o4 + o5))
}

fn parse_comment<'a>(tail: &'a str) -> MaybeParse<HtmlElement> {
  let (_, tail, o1) = parse_str(tail, "<!--")?;
  let (comment, tail, o2) = parse_until_str(tail, "-->")?;
  let (_, tail, o3) = parse_str(tail, "-->")?;
  Some((HtmlElement::Comment(comment.into()), tail, o1 + o2 + o3))
}

fn parse_raw_text(i: &str) -> MaybeParse<(String, Attributes, String)> {
  let (open, mut i, o1) = parse_open_tag(i)?;
  let (open_name, attributes, is_empty) = match open {
    HtmlElement::OpenTag {
      name,
      attributes,
      is_empty,
    } => (name, attributes, is_empty),
    _ => unreachable!(),
  };

  let (close_name, contents, i, o2) = if is_empty {
    (open_name.clone(), "".to_string(), i, 0)
  } else {
    let mut contents = String::new();
    let mut o2 = 0;
    while !i.is_empty() {
      let (text, new_i, new_off) = parse_text(i);
      contents.push_str(&text);
      o2 += new_off;
      i = new_i;
      if i.starts_with("</") {
        if let Some((HtmlElement::CloseTag { name }, _, _)) = parse_close_tag(i)
        {
          if name == open_name {
            break;
          }
        }
      }
      if text.is_empty() {
        i = &i[1..];
        o2 += 1;
        contents.push('<');
      }
    }
    let (close, i, o3) = parse_close_tag(i)?;
    let close_name = match close {
      HtmlElement::CloseTag { name } => name,
      _ => unreachable!(),
    };
    (close_name, contents, i, o2 + o3)
  };
  if open_name != close_name {
    return None;
  }

  Some(((open_name, attributes, contents), i, o1 + o2))
}

fn parse_style(i: &str) -> MaybeParse<HtmlElement> {
  let ((name, attributes, contents), i, o) = parse_raw_text(i)?;
  if name != "style" {
    None
  } else {
    Some((
      HtmlElement::Style {
        attributes,
        contents,
      },
      i,
      o,
    ))
  }
}

fn parse_script(i: &str) -> MaybeParse<HtmlElement> {
  let ((name, attributes, contents), i, o) = parse_raw_text(i)?;
  if name != "script" {
    None
  } else {
    Some((
      HtmlElement::Script {
        attributes,
        contents,
      },
      i,
      o,
    ))
  }
}

fn parse_directive(i: &str) -> MaybeParse<HtmlElement> {
  let ((name, attributes, contents), i, o) = parse_raw_text(i)?;
  if let Some(name) = name.strip_prefix('@') {
    Some((
      HtmlElement::Directive {
        name: name.to_string(),
        attributes,
        contents,
      },
      i,
      o,
    ))
  } else {
    None
  }
}

fn parse_open_tag(i: &str) -> MaybeParse<HtmlElement> {
  let (_, i, o1) = parse_str(i, "<")?;
  let (name, i, o2) = parse_until(i, NAME_REGEX);
  let mut attributes = HashMap::new();
  let mut i = i;
  let mut o3 = 0;
  while let Some(((key, value), new_i, new_o)) = parse_attribute(i) {
    attributes.insert(key, value);
    i = new_i;
    o3 += new_o;
  }
  let (_, i, o4) = parse_until(i, WS_REGEX);
  // Find all attributes
  let (is_empty, i, o5) = match parse_str(i, "/") {
    Some((_, i, o5)) => (true, i, o5),
    None => (false, i, 0),
  };

  let is_empty = is_empty || VOID_ELEMENTS.contains(&name);
  let (_, i, o6) = parse_char(i, '>')?;
  Some((
    HtmlElement::OpenTag {
      name: name.to_string(),
      attributes,
      is_empty,
    },
    i,
    o1 + o2 + o3 + o4 + o5 + o6,
  ))
}

fn parse_attribute(i: &str) -> MaybeParse<(String, String)> {
  let (_, i, o1) = parse_until(i, WS_REGEX);
  if o1 == 0 {
    return None;
  }
  let (key, i, o2) = parse_until(i, NAME_REGEX);
  if o2 == 0 {
    return None;
  }
  let get_value = || -> Option<(&str, &str, Offset)> {
    let (_, i, o1) = parse_until(i, WS_REGEX);
    let (_, i, o2) = parse_str(i, "=")?;
    let (_, i, o3) = parse_until(i, WS_REGEX);
    let (value, i, o4) = parse_delimited(i, '\"')
      .or_else(|| parse_delimited(i, '\''))
      .unwrap_or_else(|| parse_until(i, NAME_REGEX));
    Some((value, i, o1 + o2 + o3 + o4))
  };
  let (value, i, o3) = get_value().unwrap_or(("", i, 0));
  Some(((key.to_string(), value.to_string()), i, o1 + o2 + o3))
}

fn parse_close_tag(i: &str) -> MaybeParse<HtmlElement> {
  let (_, i, o1) = parse_str(i, "</")?;
  let (name, i, o2) = parse_until(i, NAME_REGEX);
  if o2 == 0 {
    return None;
  }
  let (_, i, o3) = parse_until(i, WS_REGEX);
  let (_, i, o4) = parse_str(i, ">")?;
  Some((
    HtmlElement::CloseTag {
      name: name.to_string(),
    },
    i,
    o1 + o2 + o3 + o4,
  ))
}

fn parse_text(i: &str) -> Parse<String> {
  let (text, i, o1) = parse_until(i, |c| c == '<');
  (text.to_string(), i, o1)
}

pub fn parse_html(
  input: &str,
) -> Result<Vec<HtmlElement>, crate::trace::Error> {
  let mut output = vec![];
  let mut validation_stack = vec![];
  let mut i = input;
  let mut offset = 0;
  let throw_err = |kind, offset| {
    let (row, column) = index_to_rc(input, offset);
    Error {
      kind,
      char_index: offset,
      row,
      column,
    }
    .into()
  };
  while i.len() > 0 {
    let (lm, new_i, new_off) = if i.starts_with("<!--") {
      parse_comment(i)
    } else if i.starts_with("<!") {
      parse_doctype(i)
    } else if i.starts_with("<style") {
      parse_style(i)
    } else if i.starts_with("<script") {
      parse_script(i)
    } else if i.starts_with("</") {
      parse_close_tag(i)
    } else if i.starts_with("<@") {
      parse_directive(i)
    } else if i.starts_with("<") {
      parse_open_tag(i)
    } else {
      let (text, new_i, new_off) = parse_text(i);
      // skip if text is all whitespace
      if text.trim().is_empty() {
        i = new_i;
        offset += new_off;
        continue;
      }
      Some((HtmlElement::Text(text), new_i, new_off))
    }
    .ok_or_else(|| throw_err(ErrorKind::Illegal, offset))?;

    if output.len() >= LEXEME_MEMORY_LIMIT {
      return Err(throw_err(ErrorKind::MemoryLimit, offset));
    }

    match &lm {
      HtmlElement::OpenTag {
        name,
        is_empty: false,
        ..
      } => validation_stack.push(name.clone()),
      HtmlElement::CloseTag { name } => {
        let top = validation_stack.pop();
        if top.map_or(true, |top| &top != name) {
          return Err(throw_err(ErrorKind::UnbalancedTags, offset));
        }
      },
      _ => {},
    };

    output.push(lm);
    i = new_i;
    offset += new_off;
  }
  if validation_stack.is_empty() {
    Ok(output)
  } else {
    Err(throw_err(ErrorKind::UnbalancedTags, offset))
  }
}
