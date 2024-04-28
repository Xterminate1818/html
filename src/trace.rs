use std::{fmt::Display, path::Path};

pub type Result<T> = std::result::Result<T, Error>;

pub struct Error {
  pub kind: ErrorKind,
  pub reason: String,
  pub backtrace: Vec<String>,
}

impl Error {
  pub fn new(kind: ErrorKind, reason: impl Into<String>) -> Self {
    Self {
      kind,
      reason: reason.into(),
      backtrace: vec![],
    }
  }

  pub fn msg(mut self, message: impl Into<String>) -> Self {
    self.reason = message.into();
    self
  }
}

pub fn compile_error(reason: impl Into<String>) -> Error {
  Error {
    kind: ErrorKind::Compilation,
    reason: reason.into(),
    backtrace: vec![],
  }
}

pub fn read_file(p: impl AsRef<Path>) -> Result<String> {
  std::fs::read_to_string(&p)
    .ctx(format!("Opening file: {}", p.as_ref().to_string_lossy()))
}

#[derive(Debug)]
pub enum ErrorKind {
  IO,
  Parsing,
  Compilation,
  Memory,
  Unknown,
}

impl Display for ErrorKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "{}",
      match self {
        ErrorKind::IO => "IO",
        ErrorKind::Parsing => "PARSING",
        ErrorKind::Compilation => "COMPILATION",
        ErrorKind::Unknown => "UNKNOWN",
        ErrorKind::Memory => "MEMORY",
      }
    )
  }
}

impl Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "\n[{} ERROR] {}\nBacktrace:\n", self.kind, self.reason)?;
    for s in self.backtrace.iter().rev() {
      write!(f, "{}\n", s)?;
    }
    Ok(())
  }
}

impl std::fmt::Debug for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self)
  }
}

impl From<std::io::Error> for Error {
  fn from(value: std::io::Error) -> Self {
    Self {
      kind: ErrorKind::IO,
      reason: format!("{}", value),
      backtrace: vec![],
    }
  }
}

pub trait WithContext<T, S: Into<String>> {
  fn ctx(self, s: S) -> Result<T>;
}

impl<T, E, S> WithContext<T, S> for std::result::Result<T, E>
where
  E: Into<Error>,
  S: Into<String>,
{
  fn ctx(self, s: S) -> Result<T> {
    self.map_err(|e| e.into()).map_err(|mut e| {
      e.backtrace.push(s.into());
      e
    })
  }
}

impl<T, S> WithContext<T, S> for std::option::Option<T>
where
  S: Into<String>,
{
  fn ctx(self, s: S) -> Result<T> {
    self.ok_or_else(|| Error {
      kind: ErrorKind::Unknown,
      reason: "Missing expected value".into(),
      backtrace: vec![s.into()],
    })
  }
}
