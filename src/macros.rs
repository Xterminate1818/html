use crate::trace::*;
use std::collections::HashMap;

pub fn default_macros() -> HashMap<String, Macro> {
  let mut hm: HashMap<String, Macro> = HashMap::new();
  hm.insert("lg:include".into(), INCLUDE_MACRO);
  hm
}

pub type Macro = fn(&HashMap<String, Option<String>>) -> Result<String>;

const INCLUDE_MACRO: Macro = |input| {
  let href = input
    .get("href")
    .ok_or(compile_error("'href' attribute missing"))?
    .as_ref()
    .ok_or(compile_error("'href' attribute is empty"))?;

  let rel = input
    .get("rel")
    .ok_or(compile_error("'rel' attribute missing"))?
    .as_ref()
    .ok_or(compile_error("'rel' attribute is empty"))?;

  let r = match rel.as_str() {
    "css" => format!("<style>\n{}\n</style>\n", read_file(href)?),
    "js" => format!("<script>\n{}\n</script>\n", read_file(href)?),
    "inline" => read_file(href)?,
    _ => return Err(compile_error(format!("Invalid 'rel' value: {}", rel))),
  };
  Ok(r)
};
