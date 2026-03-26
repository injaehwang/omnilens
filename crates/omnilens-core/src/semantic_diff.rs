//! Semantic Diff Engine
//!
//! Compares old and new versions of files at the USIR level to detect
//! behavioral changes, not just textual diffs.
//!
//! Flow:
//! 1. `git show <base>:<file>` → old source
//! 2. Parse old source → old USIR nodes
//! 3. Parse new source (current) → new USIR nodes (already in graph)
//! 4. Compare old vs new: added/removed/modified functions, changed signatures, etc.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use omnilens_ir::node::{FunctionNode, UsirNode};
use omnilens_ir::Visibility;

use crate::frontend::{LanguageFrontend, ParseResult};
use crate::verify::{ChangeRisk, SemanticChange, SemanticChangeKind};

/// A single file's semantic diff result.
pub struct FileDiff {
    pub file: String,
    pub changes: Vec<SemanticChange>,
}

/// Compare old (git ref) and new (current) versions of changed files.
pub fn compute_semantic_diff(
    base_ref: &str,
    changed_files: &[String],
    frontends: &[Box<dyn LanguageFrontend>],
    graph: &omnilens_graph::SemanticGraph,
) -> Vec<SemanticChange> {
    let mut all_changes = Vec::new();

    for file in changed_files {
        let ext = Path::new(file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let frontend = frontends
            .iter()
            .find(|f| f.extensions().contains(&ext));

        let Some(fe) = frontend else { continue };

        // Get old version from git.
        let old_source = git_show(base_ref, file);
        // Get new version from disk.
        let new_source = std::fs::read(file).ok();

        match (old_source, new_source) {
            (None, Some(new_src)) => {
                // New file — all functions are "added".
                if let Ok(parsed) = fe.parse_file(Path::new(file), &new_src) {
                    for node in &parsed.nodes {
                        if let UsirNode::Function(f) = node {
                            all_changes.push(SemanticChange {
                                location: f.span.clone(),
                                kind: SemanticChangeKind::FunctionAdded,
                                description: format!("New function '{}'", f.name.display()),
                                risk: if f.visibility == Visibility::Public {
                                    ChangeRisk::NeedsReview
                                } else {
                                    ChangeRisk::Safe
                                },
                            });
                        }
                    }
                }
            }
            (Some(old_src), None) => {
                // Deleted file — all functions are "removed".
                if let Ok(parsed) = fe.parse_file(Path::new(file), &old_src) {
                    for node in &parsed.nodes {
                        if let UsirNode::Function(f) = node {
                            all_changes.push(SemanticChange {
                                location: f.span.clone(),
                                kind: SemanticChangeKind::FunctionRemoved,
                                description: format!("Removed function '{}'", f.name.display()),
                                risk: if f.visibility == Visibility::Public {
                                    ChangeRisk::Breaking
                                } else {
                                    ChangeRisk::Safe
                                },
                            });
                        }
                    }
                }
            }
            (Some(old_src), Some(new_src)) => {
                // Modified file — compare old vs new at USIR level.
                let old_parsed = fe.parse_file(Path::new(file), &old_src).ok();
                let new_parsed = fe.parse_file(Path::new(file), &new_src).ok();

                if let (Some(old), Some(new)) = (old_parsed, new_parsed) {
                    let changes = diff_parse_results(file, &old, &new, graph);
                    all_changes.extend(changes);
                }
            }
            (None, None) => {}
        }
    }

    all_changes
}

/// Compare two parse results (old vs new) for the same file.
fn diff_parse_results(
    _file: &str,
    old: &ParseResult,
    new: &ParseResult,
    graph: &omnilens_graph::SemanticGraph,
) -> Vec<SemanticChange> {
    let mut changes = Vec::new();

    // Build maps: function short name → FunctionNode
    let old_fns = extract_function_map(&old.nodes);
    let new_fns = extract_function_map(&new.nodes);

    // Detect added functions.
    for (name, new_fn) in &new_fns {
        if !old_fns.contains_key(name) {
            changes.push(SemanticChange {
                location: new_fn.span.clone(),
                kind: SemanticChangeKind::FunctionAdded,
                description: format!("New function '{}'", new_fn.name.display()),
                risk: if new_fn.visibility == Visibility::Public {
                    ChangeRisk::NeedsReview
                } else {
                    ChangeRisk::Safe
                },
            });
        }
    }

    // Detect removed functions.
    for (name, old_fn) in &old_fns {
        if !new_fns.contains_key(name) {
            changes.push(SemanticChange {
                location: old_fn.span.clone(),
                kind: SemanticChangeKind::FunctionRemoved,
                description: format!("Removed function '{}'", old_fn.name.display()),
                risk: if old_fn.visibility == Visibility::Public {
                    ChangeRisk::Breaking
                } else {
                    ChangeRisk::Safe
                },
            });
        }
    }

    // Detect modified functions.
    for (name, new_fn) in &new_fns {
        if let Some(old_fn) = old_fns.get(name) {
            let fn_changes = diff_functions(old_fn, new_fn, graph);
            changes.extend(fn_changes);
        }
    }

    changes
}

/// Compare two versions of the same function.
fn diff_functions(
    old: &FunctionNode,
    new: &FunctionNode,
    graph: &omnilens_graph::SemanticGraph,
) -> Vec<SemanticChange> {
    let mut changes = Vec::new();

    // 1. Signature change: params count or types changed.
    if old.params.len() != new.params.len() {
        let is_public = new.visibility == Visibility::Public;
        changes.push(SemanticChange {
            location: new.span.clone(),
            kind: SemanticChangeKind::SignatureChange,
            description: format!(
                "'{}': parameter count changed ({} → {})",
                new.name.display(),
                old.params.len(),
                new.params.len()
            ),
            risk: if is_public {
                ChangeRisk::Breaking
            } else {
                ChangeRisk::NeedsReview
            },
        });
    } else {
        // Check individual param types.
        for (_i, (op, np)) in old.params.iter().zip(new.params.iter()).enumerate() {
            if op.type_ref != np.type_ref {
                changes.push(SemanticChange {
                    location: new.span.clone(),
                    kind: SemanticChangeKind::TypeChange,
                    description: format!(
                        "'{}': param '{}' type changed",
                        new.name.display(),
                        np.name
                    ),
                    risk: if new.visibility == Visibility::Public {
                        ChangeRisk::Breaking
                    } else {
                        ChangeRisk::NeedsReview
                    },
                });
            }
        }
    }

    // 2. Return type change.
    if old.return_type != new.return_type {
        changes.push(SemanticChange {
            location: new.span.clone(),
            kind: SemanticChangeKind::SignatureChange,
            description: format!(
                "'{}': return type changed ({:?} → {:?})",
                new.name.display(),
                old.return_type,
                new.return_type
            ),
            risk: if new.visibility == Visibility::Public {
                ChangeRisk::Breaking
            } else {
                ChangeRisk::NeedsReview
            },
        });
    }

    // 3. Visibility change.
    if old.visibility != new.visibility {
        let risk = match (&old.visibility, &new.visibility) {
            (Visibility::Public, _) => ChangeRisk::Breaking, // public → anything = breaking
            (_, Visibility::Public) => ChangeRisk::NeedsReview, // anything → public = API surface
            _ => ChangeRisk::Safe,
        };
        changes.push(SemanticChange {
            location: new.span.clone(),
            kind: SemanticChangeKind::ApiSurfaceChange,
            description: format!(
                "'{}': visibility changed ({:?} → {:?})",
                new.name.display(),
                old.visibility,
                new.visibility
            ),
            risk,
        });
    }

    // 4. Complexity change.
    if let (Some(old_c), Some(new_c)) = (old.complexity, new.complexity) {
        let delta = (new_c as i64 - old_c as i64).abs();
        if delta >= 3 {
            changes.push(SemanticChange {
                location: new.span.clone(),
                kind: SemanticChangeKind::ComplexityChange {
                    old: old_c,
                    new: new_c,
                },
                description: format!(
                    "'{}': complexity changed ({} → {}, {}{})",
                    new.name.display(),
                    old_c,
                    new_c,
                    if new_c > old_c { "+" } else { "" },
                    new_c as i64 - old_c as i64
                ),
                risk: if new_c > 20 {
                    ChangeRisk::NeedsReview
                } else {
                    ChangeRisk::Safe
                },
            });
        }
    }

    // 5. Async change.
    if old.is_async != new.is_async {
        changes.push(SemanticChange {
            location: new.span.clone(),
            kind: SemanticChangeKind::ControlFlowChange,
            description: format!(
                "'{}': async changed ({} → {})",
                new.name.display(),
                old.is_async,
                new.is_async
            ),
            risk: if new.visibility == Visibility::Public {
                ChangeRisk::Breaking
            } else {
                ChangeRisk::NeedsReview
            },
        });
    }

    // 6. Impact-weighted risk for public functions.
    if new.visibility == Visibility::Public && !changes.is_empty() {
        let impact = graph.impact_reverse(new.id, 2);
        if impact.total_affected > 5 {
            changes.push(SemanticChange {
                location: new.span.clone(),
                kind: SemanticChangeKind::ApiSurfaceChange,
                description: format!(
                    "'{}': {} callers affected by above changes",
                    new.name.display(),
                    impact.total_affected
                ),
                risk: ChangeRisk::NeedsReview,
            });
        }
    }

    changes
}

/// Extract a map of function short_name → FunctionNode from parsed nodes.
fn extract_function_map(nodes: &[UsirNode]) -> HashMap<String, &FunctionNode> {
    let mut map = HashMap::new();
    for node in nodes {
        if let UsirNode::Function(f) = node {
            if f.complexity.is_some() {
                // Only real functions, not placeholders.
                let short = f.name.segments.last().cloned().unwrap_or_default();
                map.insert(short, f);
            }
        }
    }
    map
}

/// Get file contents at a specific git ref.
fn git_show(ref_name: &str, file: &str) -> Option<Vec<u8>> {
    let spec = format!("{}:{}", ref_name, file.replace('\\', "/"));
    let output = Command::new("git")
        .args(["show", &spec])
        .output()
        .ok()?;

    if output.status.success() {
        Some(output.stdout)
    } else {
        None
    }
}
