//! # omnilens-frontend-python
//!
//! Python language frontend. Parses Python source files via tree-sitter
//! and converts to USIR.

mod parser;

use std::path::Path;

use anyhow::Result;
use omnilens_core::frontend::{ImportRef, LanguageFrontend, ParseResult};

pub struct PythonFrontend;

impl PythonFrontend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PythonFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageFrontend for PythonFrontend {
    fn name(&self) -> &str {
        "Python"
    }

    fn extensions(&self) -> &[&str] {
        &["py", "pyi"]
    }

    fn parse_file(&self, path: &Path, source: &[u8]) -> Result<ParseResult> {
        parser::parse(path, source)
    }

    fn extract_imports(&self, source: &[u8]) -> Result<Vec<ImportRef>> {
        parser::extract_imports(source)
    }
}
