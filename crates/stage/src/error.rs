//! Shared errors for stage.

use std::fmt;

#[derive(Debug)]
pub struct ParseError {
    pub msg: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for ParseError {}

impl From<serde_json::Error> for ParseError {
    fn from(e: serde_json::Error) -> Self {
        ParseError { msg: e.to_string() }
    }
}
