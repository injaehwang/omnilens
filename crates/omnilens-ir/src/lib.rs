//! # omnilens-ir
//!
//! Universal Semantic IR (USIR) — a language-independent intermediate representation
//! that captures code semantics for cross-language analysis and AI-native verification.

pub mod node;
pub mod edge;
pub mod types;
pub mod invariant;
pub mod contract;

use serde::{Deserialize, Serialize};

/// Unique identifier for a node in the semantic graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// Unique identifier for an edge in the semantic graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeId(pub u64);

/// A span in source code, referencing a file and byte range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub file: std::path::PathBuf,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

/// Fully qualified name (e.g., `crate::auth::token::verify`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QualifiedName {
    pub segments: Vec<String>,
}

impl QualifiedName {
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }

    pub fn display(&self) -> String {
        self.segments.join("::")
    }
}

/// Visibility of a symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal, // crate/package-level
}

/// Content hash for content-addressed storage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// Compute a hash with an optional salt for extra security.
    pub fn from_bytes(data: &[u8], salt: Option<&[u8]>) -> Self {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        Self(hash)
    }
}
