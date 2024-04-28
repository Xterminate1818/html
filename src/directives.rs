use crate::parse::Attributes;

pub fn expand_directive(
  name: &str,
  attributes: &Attributes,
  contents: &str,
) -> String {
  match name {
    "style" => style_dir(attributes).unwrap_or("<!-- -->".to_string()),
    "script" => script_dir(attributes).unwrap_or("<!-- -->".to_string()),
    "code" => code(attributes, contents).unwrap_or("<!-- -->".to_string()),
    _ => "<!-- -->".to_string(),
  }
}

fn style_dir(attributes: &Attributes) -> Option<String> {
  let path = attributes.get("href")?;
  let file = std::fs::read_to_string(path).ok()?;
  Some(format!("<style>\n{}\n</style>", file.trim()))
}

fn script_dir(attributes: &Attributes) -> Option<String> {
  let path = attributes.get("href")?;
  let file = std::fs::read_to_string(path).ok()?;
  Some(format!("<script>\n{}\n</script>", file.trim()))
}

fn code(attributes: &Attributes, contents: &str) -> Option<String> {
  use inkjet::*;
  let minimum_indent = contents
    .trim()
    .lines()
    .map(|line| line.len() - line.trim_start().len())
    .min()
    .unwrap_or(0);

  let normalized_string: String = contents
    .lines()
    .map(|line| line.replacen(" ", "", minimum_indent))
    .map(|line| line + "\n")
    .collect();

  let language = parse_language(attributes.get("lang")?)?;

  let mut hl = Highlighter::new();
  let buffer = hl
    .highlight_to_string(language, &MyFormatter(), normalized_string)
    .ok()?;

  let buffer = format!(
    "<div class=\"code-block\">\n<pre><code>{}</code></pre>\n</div>",
    buffer
  );

  Some(buffer)
}

struct MyFormatter();
use inkjet::{
  constants::HIGHLIGHT_CLASS_NAMES, formatter::*,
  tree_sitter_highlight::HighlightEvent,
};

impl Formatter for MyFormatter {
  fn write<W>(
    &self,
    source: &str,
    writer: &mut W,
    event: inkjet::tree_sitter_highlight::HighlightEvent,
  ) -> inkjet::Result<()>
  where
    W: std::fmt::Write,
  {
    match event {
      HighlightEvent::Source { start, end } => {
        let span = source
          .get(start..end)
          .expect("Source bounds should be in bounds!");
        let span = v_htmlescape::escape(span).to_string();
        writer.write_str(&span)?;
      },
      HighlightEvent::HighlightStart(idx) => {
        let name = HIGHLIGHT_CLASS_NAMES[idx.0];
        write!(writer, "<span class=\"{}\">", name)?;
      },
      HighlightEvent::HighlightEnd => {
        writer.write_str("</span>")?;
      },
    }

    Ok(())
  }
}

fn parse_language(lang: &str) -> Option<inkjet::Language> {
  use inkjet::*;
  Some(match lang {
    "rust" => Language::Rust,
    "python" => Language::Python,
    "javascript" => Language::Javascript,
    "html" => Language::Html,
    "css" => Language::Css,
    "toml" => Language::Toml,
    _ => return None,
  })
}
