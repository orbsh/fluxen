//! Shared errors for stage.

use std::fmt;

#[derive(Debug)]
pub struct KdlError {
    pub msg: String,
}

impl fmt::Display for KdlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for KdlError {}

impl From<serde_json::Error> for KdlError {
    fn from(e: serde_json::Error) -> Self {
        KdlError { msg: e.to_string() }
    }
}
