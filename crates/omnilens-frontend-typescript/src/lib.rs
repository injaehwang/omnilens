//! # omnilens-frontend-typescript
//!
//! TypeScript/JavaScript language frontend. Parses TS/JS/TSX/JSX
//! source files via tree-sitter and converts to USIR.

mod parser;

use std::path::Path;

use anyhow::Result;
use omnilens_core::frontend::{ImportRef, LanguageFrontend, ParseResult};

pub struct TypeScriptFrontend;

impl TypeScriptFrontend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TypeScriptFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageFrontend for TypeScriptFrontend {
    fn name(&self) -> &str {
        "TypeScript"
    }

    fn extensions(&self) -> &[&str] {
        &["ts", "tsx", "js", "jsx", "mts", "mjs"]
    }

    fn parse_file(&self, path: &Path, source: &[u8]) -> Result<ParseResult> {
        let is_tsx = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e, "tsx" | "jsx"))
            .unwrap_or(false);

        parser::parse(path, source, is_tsx)
    }

    fn extract_imports(&self, source: &[u8]) -> Result<Vec<ImportRef>> {
        parser::extract_imports(source)
    }
}
