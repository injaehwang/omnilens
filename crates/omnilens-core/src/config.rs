//! Project configuration — auto-detection and user overrides.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Project-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub project_root: PathBuf,
    pub languages: Vec<LanguageConfig>,
    pub verification: VerificationConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageConfig {
    pub name: String,
    pub enabled: bool,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Minimum confidence to report an invariant violation.
    pub invariant_min_confidence: f64,
    /// Minimum confidence to report a contract violation.
    pub contract_min_confidence: f64,
    /// Enable AI-specific checks (hallucinated APIs, stale patterns, etc.).
    pub ai_checks_enabled: bool,
    /// Severity threshold for CI/CD gate (error, warning, info).
    pub gate_severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Where to store the omnilens index.
    pub index_dir: PathBuf,
    /// Maximum memory for in-memory graph (bytes).
    pub max_memory: usize,
}

impl Config {
    /// Auto-detect project configuration from filesystem.
    pub fn detect(project_root: &Path) -> Result<Self> {
        // Check for omnilens.toml first, then auto-detect.
        let config_path = project_root.join("omnilens.toml");
        if config_path.exists() {
            return Self::load_from_file(&config_path);
        }

        Ok(Self::auto_detect(project_root))
    }

    fn load_from_file(_path: &Path) -> Result<Self> {
        todo!("Load from omnilens.toml")
    }

    fn auto_detect(project_root: &Path) -> Self {
        Self {
            project_root: project_root.to_owned(),
            languages: Vec::new(), // Will be populated by scanning
            verification: VerificationConfig {
                invariant_min_confidence: 0.8,
                contract_min_confidence: 0.7,
                ai_checks_enabled: true,
                gate_severity: "error".to_string(),
            },
            storage: StorageConfig {
                index_dir: project_root.join(".omnilens"),
                max_memory: 512 * 1024 * 1024, // 512MB default
            },
        }
    }
}
