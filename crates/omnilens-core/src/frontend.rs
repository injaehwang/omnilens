//! Language frontend trait and auto-detection.

use std::path::Path;

use anyhow::Result;
use omnilens_ir::edge::UsirEdge;
use omnilens_ir::node::UsirNode;

/// Result of parsing a single file.
pub struct ParseResult {
    pub nodes: Vec<UsirNode>,
    pub edges: Vec<UsirEdge>,
}

/// Trait that all language frontends must implement.
pub trait LanguageFrontend: Send + Sync {
    /// Display name (e.g., "Rust", "TypeScript").
    fn name(&self) -> &str;

    /// Supported file extensions (e.g., ["rs"], ["ts", "tsx"]).
    fn extensions(&self) -> &[&str];

    /// Parse a single file into USIR nodes and edges.
    fn parse_file(&self, path: &Path, source: &[u8]) -> Result<ParseResult>;

    /// Extract cross-file references for dependency resolution.
    fn extract_imports(&self, source: &[u8]) -> Result<Vec<ImportRef>>;
}

/// A reference to an imported symbol.
pub struct ImportRef {
    pub source_module: String,
    pub symbols: Vec<String>,
    pub is_wildcard: bool,
}

/// Auto-detect which language frontends to activate based on project files.
pub fn detect_frontends(_project_root: &Path) -> Vec<Box<dyn LanguageFrontend>> {
    // For now, always register all compiled-in frontends.
    // Phase 2: scan for Cargo.toml, package.json, etc. and selectively enable.
    vec![
        // Rust frontend is always available.
    ]
}
