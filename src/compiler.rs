use html_parser::{Dom, DomVariant, Element, ElementVariant, Node};
use std::{collections::HashMap, path::Path};

use crate::{
  macros::*,
  trace::{WithContext, *},
};

/// Replaces @variables in template with appropriate
/// values, or removes if not provided
fn init_vars(
  node: Node,
  attributes: &HashMap<String, Option<String>>,
  children: &Vec<Node>,
) -> Node {
  let mut element = match node {
    Node::Text(_) | Node::Comment(_) => return node,
    Node::Element(e) => e,
  };

  let mut new_classes = Vec::with_capacity(element.classes.len());

  for cls in &mut element.classes {
    if !cls.starts_with('@') {
      new_classes.push(cls.clone());
      continue;
    }
    let name = &cls[1..];
    // Attributes in class must not be null
    if let Some(Some(k)) = attributes.get(name) {
      new_classes.push(k.clone());
    }
    // Otherwise discard variable from classes
  }
  element.classes = new_classes;

  // Attributes in ID must not be null
  if let Some(id) = &element.id {
    if let Some(name) = id.strip_prefix('@') {
      element.id = attributes.get(name).unwrap_or(&None).clone();
    } else {
      element.id = None;
    }
  }

  let mut new_attributes = HashMap::new();

  for (key, value) in &element.attributes {
    if let Some(id) = value {
      if let Some(variable_name) = id.strip_prefix('@') {
        if let Some(value) = attributes.get(variable_name) {
          // Insert null and non-null variables if key exists
          new_attributes.insert(key.clone(), value.clone());
        }
        // Otherwise discard variable
      }
      // Insert non-null non-variable value
      else {
        new_attributes.insert(key.clone(), value.clone());
      }
    } else {
      // Insert null non-variable value
      new_attributes.insert(key.clone(), value.clone());
    }
  }

  element.attributes = new_attributes;
  let mut new_children = Vec::with_capacity(element.children.len());
  for child in &element.children {
    let name = child.element().map(|c| c.name.as_str()).unwrap_or("");
    if "tp:children" == name {
      for child in children {
        new_children.push(child.clone());
      }
    } else {
      let child = init_vars(child.clone(), attributes, children);
      new_children.push(child);
    }
  }

  element.children = new_children;

  Node::Element(element)
}

/// Swaps out a template invocation for its definition
fn expand_templates(
  invocation: Element,
  templates: &Element,
) -> Result<Vec<Node>> {
  for child in &invocation.children {
    if child
      .element()
      .map(|c| c.name.as_str().strip_prefix("tp:").is_some())
      .unwrap_or(false)
    {
      return Err(Error::new(
        ErrorKind::Compilation,
        "Illegal use of tp namespace",
      ));
    }
    for subchild in child {
      if subchild
        .element()
        .map(|c| c.name.as_str().strip_prefix("tp:").is_some())
        .unwrap_or(false)
      {
        return Err(Error::new(
          ErrorKind::Compilation,
          "Illegal use of tp namespace",
        ));
      }
    }
  }
  // Collect params
  let mut attributes = invocation.attributes;
  let classes = invocation.classes.join(" ");
  if classes.len() != 0 {
    attributes.insert("class".into(), Some(classes));
  }
  if let Some(id) = invocation.id {
    attributes.insert("id".into(), Some(id));
  }
  // Swap params
  let expanded = init_vars(
    Node::Element(templates.clone()),
    &attributes,
    &invocation.children,
  );
  Ok(expanded.element().unwrap().children.clone())
}

/// Serializes HTML node
/// * `node` - Node to serialize
/// * `templates` - Map of templates by their names
fn node_to_string(
  node: &Node,
  templates: &HashMap<String, Element>,
  macros: &HashMap<String, Macro>,
) -> Result<String> {
  const OPEN: bool = false;
  const CLOSE: bool = true;
  let mut stack: Vec<(Node, bool)> = vec![(node.clone(), OPEN)];
  let mut buf = String::new();
  while let Some((current, closing)) = stack.pop() {
    if closing == OPEN {
      stack.push((current.clone(), CLOSE));
      match current {
        Node::Text(t) => {
          buf.push_str(&t);
        },
        Node::Element(e) => {
          // Expand if macro
          if let Some(mc) = macros.get(&e.name) {
            stack.pop().unwrap();
            let expanded =
              mc(&e.attributes).ctx(format!("Expanding macro: {}", e.name))?;
            buf.push_str(&expanded);
            continue;
          }
          // Expand if template
          if let Some(tp) = templates.get(&e.name) {
            let _ = stack.pop();
            let elements = expand_templates(e.clone(), tp)
              .ctx(format!("Expanding template: {}", e.name))?;
            for element in elements.into_iter().rev() {
              stack.push((element, OPEN));
            }
            continue;
          }
          // <
          buf.push('<');
          // Tag name
          buf.push_str(&e.name);
          // Classes
          if !e.classes.is_empty() {
            buf.push_str(&format!(" class='{}'", e.classes.join(" ")));
          }
          // ID
          if let Some(id) = &e.id {
            buf.push_str(&format!(" id='{}'", id));
          }
          // Attributes
          for (k, v) in &e.attributes {
            match v {
              Some(v) => buf.push_str(&format!(" {}='{}'", k, v)),
              None => buf.push_str(&format!(" {}", k)),
            }
          }
          match e.variant {
            ElementVariant::Normal => {
              buf.push_str(">\n");
              for child in e.children.iter().rev() {
                stack.push((child.clone(), OPEN));
              }
            },
            ElementVariant::Void => {
              buf.push_str("/>\n");
              let _ = stack.pop();
            },
          }
        },
        Node::Comment(_) => {},
      }
    } else {
      match current {
        Node::Text(_) => buf.push('\n'),
        Node::Comment(_) => {},
        Node::Element(e) => {
          buf.push_str(&format!("</{}>\n", e.name));
        },
      }
    }
  }
  Ok(buf)
}

pub struct Compiler {
  templates: HashMap<String, Element>,
  macros: HashMap<String, Macro>,
}

impl Compiler {
  pub fn new() -> Self {
    Self {
      templates: HashMap::new(),
      macros: default_macros(),
    }
  }

  pub fn parse_templates_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
    let file = read_file(&path)?;
    self.parse_templates(&file).ctx(format!(
      "Parsing templates file: {}",
      path.as_ref().to_string_lossy()
    ))?;
    Ok(())
  }

  pub fn parse_templates(&mut self, html: &str) -> Result<()> {
    let tps: HashMap<String, Element> = Dom::parse(html)?
      .children
      .into_iter()
      .map(|i| i.element().cloned())
      .flatten()
      .map(|i| (i.name.clone(), i))
      .collect();
    if tps.is_empty() {
      return Err(Error::new(ErrorKind::Parsing, "No blueprints found"));
    }
    self.templates.extend(tps);
    Ok(())
  }

  pub fn compile_source_file(&self, path: impl AsRef<Path>) -> Result<String> {
    let file = read_file(&path)?;
    self.compile_source(&file).ctx(format!(
      "Compiling source file: {}",
      path.as_ref().to_string_lossy()
    ))
  }

  pub fn compile_source(&self, html: &str) -> Result<String> {
    let dom = Dom::parse(html)?;
    if !dom.errors.is_empty() {}
    if dom.tree_type == DomVariant::Empty {
      return Err(Error::new(ErrorKind::Parsing, "Empty DOM"));
    }
    if dom.tree_type != DomVariant::Document {
      return Err(Error::new(
        ErrorKind::Parsing,
        "DOM must exactly have 1 root node",
      ));
    }
    let tree = &dom.children[0];
    node_to_string(&tree, &self.templates, &self.macros)
  }
}
