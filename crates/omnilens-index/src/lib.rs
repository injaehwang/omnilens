//! # omnilens-index
//!
//! Incremental indexing engine. Detects file changes via git diff
//! or content-hash comparison and triggers minimal re-analysis.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;
use omnilens_ir::ContentHash;

/// Tracks file changes for incremental indexing.
pub struct Indexer {
    project_root: PathBuf,
    /// Maps file paths to their last known content hash.
    file_hashes: HashMap<PathBuf, ContentHash>,
}

/// A file that has changed since last index.
pub struct ChangedFile {
    pub path: PathBuf,
    pub content: Vec<u8>,
    pub kind: ChangeKind,
}

#[derive(Debug)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

impl Indexer {
    pub fn new(project_root: &Path) -> Result<Self> {
        Ok(Self {
            project_root: project_root.to_owned(),
            file_hashes: HashMap::new(),
        })
    }

    /// Discover all source files in the project.
    /// Respects .gitignore and common ignore patterns.
    pub fn discover_files(&self, extensions: &[&str]) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        let walker = WalkBuilder::new(&self.project_root)
            .hidden(true) // skip hidden dirs
            .git_ignore(true) // respect .gitignore
            .git_global(true)
            .build();

        for entry in walker {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext) {
                    files.push(path.to_owned());
                }
            }
        }

        Ok(files)
    }

    /// Detect changed files since last index.
    /// First run: all files are "Added".
    /// Subsequent runs: compare content hashes.
    pub fn detect_changes(&self, extensions: &[&str]) -> Result<Vec<ChangedFile>> {
        let files = self.discover_files(extensions)?;
        let mut changes = Vec::new();

        for path in files {
            let content = std::fs::read(&path)?;
            let hash = ContentHash::from_bytes(&content);

            let kind = match self.file_hashes.get(&path) {
                None => ChangeKind::Added,
                Some(old_hash) if old_hash != &hash => ChangeKind::Modified,
                Some(_) => continue, // unchanged
            };

            changes.push(ChangedFile {
                path,
                content,
                kind,
            });
        }

        // Check for deleted files.
        for old_path in self.file_hashes.keys() {
            if !old_path.exists() {
                changes.push(ChangedFile {
                    path: old_path.clone(),
                    content: Vec::new(),
                    kind: ChangeKind::Deleted,
                });
            }
        }

        Ok(changes)
    }

    /// Record that changes have been indexed.
    pub fn commit_changes(&mut self, changes: &[ChangedFile]) -> Result<()> {
        for change in changes {
            match change.kind {
                ChangeKind::Deleted => {
                    self.file_hashes.remove(&change.path);
                }
                _ => {
                    let hash = ContentHash::from_bytes(&change.content);
                    self.file_hashes.insert(change.path.clone(), hash);
                }
            }
        }
        Ok(())
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }
}
