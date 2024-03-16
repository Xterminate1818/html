mod compiler;
mod macros;
mod parser;
mod trace;
use compiler::*;
use trace::*;

fn run_compiler() -> Result<()> {
  // Read config file
  let default_config = include_str!("../default.toml")
    .parse::<toml::Table>()
    .unwrap();
  let user_config = std::fs::read_to_string("lg.toml")
    .ctx("Reading config file")?
    .parse::<toml::Table>()
    .ctx("Parsing config file")?;
  let mut config = default_config.clone();
  config.extend(user_config);

  let mut c = Compiler::new();
  let compile_table = config["compile"]
    .as_table()
    .ctx("Reading compile options")?;

  // Parse all templates
  let templates_path = compile_table["templates"]
    .as_str()
    .ctx("Reading templates path")?;
  let dirs = walkdir::WalkDir::new(templates_path)
    .into_iter()
    .flatten()
    .filter_map(|v| {
      if v.file_name().to_str().unwrap_or("").ends_with(".html")
        && v.file_type().is_file()
      {
        Some(v)
      } else {
        None
      }
    });
  for file in dirs {
    c.parse_templates_file(file.path())?;
  }
  // Compile all source files
  let source_path = compile_table["source"]
    .as_str()
    .ctx("Reading templates path")?;
  let dirs = walkdir::WalkDir::new(source_path)
    .into_iter()
    .flatten()
    .filter_map(|v| {
      if v.file_name().to_str().unwrap_or("").ends_with(".html")
        && v.file_type().is_file()
      {
        Some(v)
      } else {
        None
      }
    });

  for file in dirs {}

  // println!("{}", s);
  Ok(())
}

fn main() {
  let test = include_str!("../simple.html").to_string();
  // let test = " <as>    </as> ".to_string();
  let r = parser::parse_html(&test).unwrap();
  for l in r {
    println!("{l:?}");
  }
  // match run_compiler() {
  //   Ok(_) => {},
  //   Err(e) => {
  //     eprintln!("{}", e);
  //     std::process::exit(1);
  //   },
  // }
}
