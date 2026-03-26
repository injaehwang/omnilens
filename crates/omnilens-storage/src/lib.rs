//! # omnilens-storage
//!
//! Persistent storage layer using content-addressed objects and redb.
//! Provides fast serialization/deserialization of the semantic graph.

use std::path::{Path, PathBuf};

use anyhow::Result;
use omnilens_ir::ContentHash;

/// Storage backend for omnilens index data.
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    /// Open or create storage at the given directory.
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_owned(),
        })
    }

    /// Store a content-addressed object.
    pub fn put(&self, hash: &ContentHash, data: &[u8]) -> Result<()> {
        let path = self.object_path(hash);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        Ok(())
    }

    /// Retrieve a content-addressed object.
    pub fn get(&self, hash: &ContentHash) -> Result<Option<Vec<u8>>> {
        let path = self.object_path(hash);
        if path.exists() {
            Ok(Some(std::fs::read(&path)?))
        } else {
            Ok(None)
        }
    }

    /// Check if an object exists.
    pub fn exists(&self, hash: &ContentHash) -> bool {
        self.object_path(hash).exists()
    }

    fn object_path(&self, hash: &ContentHash) -> PathBuf {
        let hex = hex::encode(hash.0);
        self.root
            .join("objects")
            .join(&hex[..2])
            .join(&hex[2..])
    }
}
