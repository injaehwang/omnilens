//! # omnilens-core
//!
//! Core orchestration for the omnilens verification engine.
//! Coordinates language frontends, semantic graph, analysis passes,
//! and verification pipelines.

pub mod ai;
pub mod frontend;
pub mod invariants;
pub mod output;
pub mod semantic_diff;
pub mod verify;
pub mod config;

use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use omnilens_graph::SemanticGraph;
use omnilens_index::{ChangeKind, Indexer};
use tracing::info;

/// The main omnilens engine — entry point for all operations.
pub struct Engine {
    pub config: config::Config,
    pub graph: SemanticGraph,
    index: Indexer,
    frontends: Vec<Box<dyn frontend::LanguageFrontend>>,
}

impl Engine {
    /// Initialize omnilens for a project directory.
    pub fn init(project_root: &Path) -> Result<Self> {
        let config = config::Config::detect(project_root)?;
        let graph = SemanticGraph::new();
        let index = Indexer::new(project_root)?;
        let frontends = frontend::detect_frontends(project_root);

        Ok(Self {
            config,
            graph,
            index,
            frontends,
        })
    }

    /// Register an additional language frontend.
    pub fn register_frontend(&mut self, frontend: Box<dyn frontend::LanguageFrontend>) {
        self.frontends.push(frontend);
    }

    /// Get all supported file extensions across registered frontends.
    fn all_extensions(&self) -> Vec<&str> {
        self.frontends
            .iter()
            .flat_map(|f| f.extensions().iter().copied())
            .collect()
    }

    /// Build or update the semantic index.
    pub fn index(&mut self) -> Result<IndexResult> {
        let extensions = self.all_extensions();
        let changes = self.index.detect_changes(&extensions)?;

        if changes.is_empty() {
            return Ok(IndexResult {
                files_analyzed: 0,
                nodes_added: 0,
                edges_added: 0,
                links_resolved: 0,
                links_unresolved: 0,
                duration: std::time::Duration::ZERO,
            });
        }

        let start = Instant::now();
        let mut nodes_added = 0;
        let mut edges_added = 0;
        let mut files_analyzed = 0;

        for change in &changes {
            match change.kind {
                ChangeKind::Deleted => {
                    self.graph.remove_file(&change.path);
                    continue;
                }
                _ => {}
            }

            if let Some(fe) = self.frontend_for(&change.path) {
                match fe.parse_file(&change.path, &change.content) {
                    Ok(parsed) => {
                        // Remove old nodes for this file before adding new ones.
                        self.graph.remove_file(&change.path);

                        for node in parsed.nodes {
                            self.graph.add_node(node);
                            nodes_added += 1;
                        }
                        for edge in parsed.edges {
                            self.graph.add_edge(edge);
                            edges_added += 1;
                        }
                        files_analyzed += 1;
                    }
                    Err(e) => {
                        info!("Failed to parse {}: {}", change.path.display(), e);
                    }
                }
            }
        }

        self.index.commit_changes(&changes)?;

        // Pass 2: Cross-file symbol resolution.
        let link_result = omnilens_graph::linker::link(&mut self.graph);
        info!(
            "Linker: {} resolved, {} unresolved, {} merged",
            link_result.resolved, link_result.unresolved, link_result.duplicates_merged
        );

        Ok(IndexResult {
            files_analyzed,
            nodes_added,
            edges_added,
            links_resolved: link_result.resolved,
            links_unresolved: link_result.unresolved,
            duration: start.elapsed(),
        })
    }

    /// Verify changes (semantic diff, invariants, contracts).
    pub fn verify(&self, diff: &verify::DiffSpec) -> Result<verify::VerifyResult> {
        verify::run_verification(&self.graph, &self.config, diff, &self.frontends)
    }

    /// Find the impact of changing a specific node.
    pub fn impact(
        &self,
        file: &Path,
        fn_name: Option<&str>,
        depth: usize,
    ) -> Result<omnilens_graph::impact::ImpactResult> {
        use omnilens_ir::node::UsirNode;

        // Try multiple path forms to match against file_index.
        let nodes = self.find_nodes_for_file(file);

        if nodes.is_empty() {
            anyhow::bail!(
                "No symbols found in {} (graph has {} nodes)",
                file.display(),
                self.graph.node_count()
            );
        }

        let target_id = if let Some(name) = fn_name {
            nodes
                .iter()
                .find_map(|id| {
                    let node = self.graph.get_node(*id)?;
                    match node {
                        UsirNode::Function(f)
                            if f.name.segments.last().map(|s| s.as_str()) == Some(name) =>
                        {
                            Some(*id)
                        }
                        _ => None,
                    }
                })
                .ok_or_else(|| anyhow::anyhow!("Function '{}' not found in {}", name, file.display()))?
        } else {
            *nodes.first().ok_or_else(|| {
                anyhow::anyhow!("No symbols found in {}", file.display())
            })?
        };

        Ok(self.graph.impact_reverse(target_id, depth))
    }

    /// Find nodes for a file, handling path normalization.
    fn find_nodes_for_file(&self, file: &Path) -> Vec<omnilens_ir::NodeId> {
        // Try direct lookup first.
        let direct = self.graph.nodes_in_file(&file.to_path_buf());
        if !direct.is_empty() {
            return direct.to_vec();
        }

        // Try canonical path.
        if let Ok(canonical) = file.canonicalize() {
            let nodes = self.graph.nodes_in_file(&canonical);
            if !nodes.is_empty() {
                return nodes.to_vec();
            }
        }

        // Try suffix matching: find any file in the index that ends with
        // the given path segments.
        let file_str = file.to_string_lossy().replace('\\', "/");
        self.graph
            .find_file_by_suffix(&file_str)
            .unwrap_or_default()
    }

    /// Find a target node by file and optional function name.
    /// Returns (NodeId, display_name).
    pub fn find_target(
        &self,
        file: &Path,
        fn_name: Option<&str>,
    ) -> Result<(omnilens_ir::NodeId, String)> {
        use omnilens_ir::node::UsirNode;

        let nodes = self.find_nodes_for_file(file);

        if nodes.is_empty() {
            anyhow::bail!(
                "No symbols found in {} (graph has {} nodes)",
                file.display(),
                self.graph.node_count()
            );
        }

        if let Some(name) = fn_name {
            let id = nodes
                .iter()
                .find_map(|id| {
                    let node = self.graph.get_node(*id)?;
                    match node {
                        UsirNode::Function(f)
                            if f.name.segments.last().map(|s| s.as_str()) == Some(name) =>
                        {
                            Some(*id)
                        }
                        _ => None,
                    }
                })
                .ok_or_else(|| {
                    anyhow::anyhow!("Function '{}' not found in {}", name, file.display())
                })?;
            Ok((id, name.to_string()))
        } else {
            let id = *nodes.first().unwrap();
            let name = self
                .graph
                .get_node(id)
                .map(|n| n.name().display())
                .unwrap_or_else(|| "unknown".to_string());
            Ok((id, name))
        }
    }

    fn frontend_for(&self, path: &Path) -> Option<&dyn frontend::LanguageFrontend> {
        let ext = path.extension()?.to_str()?;
        self.frontends
            .iter()
            .find(|f| f.extensions().contains(&ext))
            .map(|f| f.as_ref())
    }
}

pub struct IndexResult {
    pub files_analyzed: usize,
    pub nodes_added: usize,
    pub edges_added: usize,
    pub links_resolved: usize,
    pub links_unresolved: usize,
    pub duration: std::time::Duration,
}
