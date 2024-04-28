mod compile;
mod directives;
mod parse;
mod trace;

fn main() -> Result<(), trace::Error> {
  let mut c = compile::Compiler::new();
  c.with_template_folder("templates/")?
    .with_src_folder("hyper-src/", "hyper-build/")?;
  Ok(())
}
