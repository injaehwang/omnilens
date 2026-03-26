//! # omnilens-frontend-rust
//!
//! Rust language frontend. Parses Rust source files via tree-sitter
//! and converts to USIR nodes and edges.

mod parser;

use std::path::Path;

use anyhow::Result;
use omnilens_core::frontend::{ImportRef, LanguageFrontend, ParseResult};

pub struct RustFrontend {
    parser: parser::RustParser,
}

impl RustFrontend {
    pub fn new() -> Self {
        Self {
            parser: parser::RustParser::new(),
        }
    }
}

impl Default for RustFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageFrontend for RustFrontend {
    fn name(&self) -> &str {
        "Rust"
    }

    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn parse_file(&self, path: &Path, source: &[u8]) -> Result<ParseResult> {
        self.parser.parse(path, source)
    }

    fn extract_imports(&self, source: &[u8]) -> Result<Vec<ImportRef>> {
        self.parser.extract_imports(source)
    }
}
