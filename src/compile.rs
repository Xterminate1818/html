use crate::trace::*;
use std::{collections::HashMap, io::Write, path::Path, rc::Rc};

use crate::parse::*;

type Lexeme = HtmlElement;

pub const RECURSION_LIMIT: usize = 256;

#[derive(Debug, Clone)]
pub struct Element {
  attributes: Attributes,
  child_span: Vec<Lexeme>,
}
pub type Templates = HashMap<String, Element>;

fn parse_element<'a>(tail: &'a [Lexeme]) -> Option<(Element, &'a [Lexeme])> {
  // Parse first element
  let (_, attributes) = match tail.first() {
    Some(lm) => match lm {
      Lexeme::OpenTag {
        attributes,
        is_empty: true,
        ..
      } => {
        return Some((
          Element {
            attributes: attributes.clone(),
            child_span: vec![],
          },
          &tail[1..],
        ))
      },
      Lexeme::OpenTag {
        name,
        attributes,
        is_empty: false,
      } => (name, attributes),
      _ => return None,
    },
    None => return None,
  };

  let mut depth = 0;
  let mut end = 1;

  for lm in tail {
    match lm {
      Lexeme::OpenTag { is_empty, .. } => {
        if !is_empty {
          depth += 1;
        }
      },
      Lexeme::CloseTag { .. } => {
        depth -= 1;
        if depth == 0 {
          break;
        }
      },
      _ => {
        if depth == 0 {
          return None;
        }
      },
    }
    end += 1;
  }
  Some((
    Element {
      attributes: attributes.clone(),
      child_span: tail[1..end - 1].to_vec(),
    },
    &tail[end..],
  ))
}

pub fn parse_templates(source: impl AsRef<str>) -> Result<Templates> {
  let mut tail: &[Lexeme] = &parse_html(source.as_ref())?;
  let mut new_templates: Templates = Default::default();
  while let Some(lm) = tail.first() {
    match lm {
      Lexeme::OpenTag {
        name,
        is_empty: false,
        ..
      } => {
        let (head, new_tail) = parse_element(tail)
          .ctx(format!("at template definition {}", name))?;

        new_templates.insert(name.clone(), head);
        tail = new_tail;
      },
      // Ignore comments
      Lexeme::Comment(_) => {
        tail = &tail[1..];
      },
      _ => {
        eprintln!("[WARNING] Unexpected root node: {}", lm.serialize());
        tail = &tail[1..];
      },
    }
  }
  Ok(new_templates)
}

pub fn parse_templates_file(path: impl AsRef<Path>) -> Result<Templates> {
  let file = read_file(path.as_ref()).ctx(format!("opening templates file"))?;
  parse_templates(file).ctx(format!("in file {}", path.as_ref().display()))
}

fn expand_template(
  base: &Element,
  template: &Element,
  output: &mut Vec<Lexeme>,
) {
  for element in &template.child_span {
    match element {
      Lexeme::OpenTag {
        name,
        is_empty,
        attributes,
      } => {
        let mut new_attributes = HashMap::new();

        for (key, value) in attributes {
          if let Some(at_key) = value.strip_prefix('@') {
            if let Some(at_value) = base.attributes.get(at_key) {
              new_attributes.insert(at_key.to_string(), at_value.clone());
            }
          } else {
            new_attributes.insert(key.into(), value.into());
          }
        }

        // for (key, value) in attributes {
        //   // If value is non-null
        //   if let Some(id) = value {
        //     // If value is '@' prefixed
        //     if let Some(variable_name) = id.strip_prefix('@') {
        //       // If key exists in base
        //       if let Some(new_value) =
        // base.attributes.get(variable_name) {
        //         new_attributes.insert(key.clone(),
        // new_value.clone());       }
        //       // Otherwise discard variable
        //     }
        //     // Insert non-null non-variable value
        //     else {
        //       new_attributes.insert(key.clone(), value.clone());
        //     }
        //   } else {
        //     // Insert null non-variable value
        //     new_attributes.insert(key.clone(), value.clone());
        //   }
        // }

        output.push(Lexeme::OpenTag {
          name: name.clone(),
          attributes: new_attributes,
          is_empty: *is_empty,
        });
      },
      Lexeme::Directive { name, .. } => {
        if name == "children" {
          output.append(&mut base.child_span.to_vec());
        } else {
          output.push(element.clone());
        }
      },
      _ => output.push(element.clone()),
    }
  }
}

fn compilation_pass(
  mut source: &[Lexeme],
  templates: &Templates,
) -> Result<(Vec<Lexeme>, usize)> {
  let mut output: Vec<Lexeme> = vec![];
  let mut num_expanded = 0;
  while let Some(lm) = source.first() {
    match lm {
      Lexeme::OpenTag { name, .. } => {
        if let Some(tmp) = templates.get(name) {
          num_expanded += 1;
          let (base, new_tail) =
            parse_element(source).ctx(format!("at template usage {}", name))?;
          expand_template(&base, tmp, &mut output);
          source = new_tail;
          continue;
        } else {
          output.push(lm.clone());
        }
      },
      _ => output.push(lm.clone()),
    }
    source = &source[1..];
  }
  Ok((output, num_expanded))
}

pub fn compile_source(
  source: impl AsRef<str>,
  templates: &Templates,
) -> Result<Vec<Lexeme>> {
  let mut source: Vec<Lexeme> = parse_html(source.as_ref())?;
  for i in 1..=RECURSION_LIMIT {
    let (new_source, num_expanded) = compilation_pass(&source, templates)?;
    source = new_source;
    if num_expanded == 0 {
      break;
    }
    if source.len() >= LEXEME_MEMORY_LIMIT {
      return Err(compile_error("reached memory limit expanding templates"));
    }
    if i == RECURSION_LIMIT {
      return Err(compile_error("reached recursion limit expanding templates"));
    }
  }
  Ok(source.to_vec())
}

pub fn compile_source_file(
  path: impl AsRef<Path>,
  templates: &Templates,
) -> Result<Vec<Lexeme>> {
  let file = read_file(&path)?;
  compile_source(file, templates)
    .ctx(format!("while compiling file {}", path.as_ref().display()))
}

pub fn serialize(output: &Vec<Lexeme>) -> String {
  output
    .iter()
    .map(|lm| lm.serialize())
    .collect::<Vec<_>>()
    .join("")
}

pub fn serialize_mini(output: &Vec<Lexeme>) -> String {
  output
    .iter()
    .map(|lm| lm.serialize())
    .collect::<Vec<_>>()
    .join("")
}

pub struct Compiler {
  templates: HashMap<String, Element>,
}

impl Compiler {
  pub fn new() -> Self {
    Self {
      templates: Default::default(),
    }
  }

  pub fn with_template_file(
    &mut self,
    path: impl AsRef<Path>,
  ) -> Result<&mut Self> {
    let templates = parse_templates_file(path)?;
    self.templates.extend(templates);
    Ok(self)
  }

  pub fn with_src(
    &mut self,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
  ) -> Result<&mut Self> {
    let source = compile_source_file(from, &self.templates)?;
    let serial = serialize(&source);
    let mut new_file = std::fs::File::create(to)?;
    new_file.write_all(serial.as_bytes())?;
    Ok(self)
  }

  pub fn with_template_folder(
    &mut self,
    from: impl AsRef<Path>,
  ) -> Result<&mut Self> {
    for file in std::fs::read_dir(&from)? {
      let file =
        file.ctx(format!("reading directory {}", from.as_ref().display()))?;
      let path = file.path();
      let last = path.components().last().ctx("empty path encountered")?;
      let ft = match file.file_type() {
        Ok(ft) => ft,
        Err(_) => continue,
      };
      if ft.is_dir() {
        self.with_template_folder(&from.as_ref().join(&last))?;
        continue;
      }
      let ext = match path.extension() {
        Some(ext) => ext,
        None => continue,
      };

      if ext != "html" {
        continue;
      }
      self.with_template_file(&path)?;
    }
    Ok(self)
  }

  pub fn with_src_folder(
    &mut self,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
  ) -> Result<&mut Self> {
    let _ = std::fs::remove_dir_all(&to);
    let _ = std::fs::create_dir_all(&to);
    for file in std::fs::read_dir(&from)? {
      let file =
        file.ctx(format!("reading directory {}", from.as_ref().display()))?;
      let path = file.path();
      let last = path.components().last().ctx("empty path encountered")?;
      let destination = to.as_ref().join(&last);
      let ft = match file.file_type() {
        Ok(ft) => ft,
        Err(_) => continue,
      };
      if ft.is_dir() {
        self.with_src_folder(
          &from.as_ref().join(&last),
          &to.as_ref().join(&last),
        )?;
        continue;
      }
      let ext = match path.extension() {
        Some(ext) => ext,
        None => continue,
      };

      // Copy file but do not compile
      if ext != "html" {
        let file = std::fs::read(&path)
          .ctx(format!("opening file to read: {}", path.display()))?;
        let mut new_file = std::fs::File::create(&destination)
          .ctx(format!("opening file to write: {}", destination.display()))?;
        new_file
          .write_all(&file)
          .ctx(format!("writing to file: {}", destination.display()))?;
        continue;
      }
      self.with_src(&path, destination)?;
    }
    Ok(self)
  }
}
