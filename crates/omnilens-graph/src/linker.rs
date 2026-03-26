//! Cross-file symbol linker.
//!
//! After all files are parsed (Pass 1), the linker resolves placeholder nodes
//! to their actual definitions across files (Pass 2).
//!
//! The problem: when file A calls `foo()` defined in file B, the parser for A
//! doesn't know about B yet. So it creates a placeholder node for `foo`.
//! The linker finds the real `foo` in B and redirects all edges from the
//! placeholder to the real definition.

use omnilens_ir::NodeId;
use tracing::debug;

use crate::SemanticGraph;

/// Statistics from the linking pass.
#[derive(Debug, Default)]
pub struct LinkResult {
    /// Number of placeholder nodes successfully resolved.
    pub resolved: usize,
    /// Number of placeholder nodes that couldn't be resolved (external deps).
    pub unresolved: usize,
    /// Number of edges retargeted.
    pub edges_retargeted: usize,
    /// Number of duplicate placeholders merged.
    pub duplicates_merged: usize,
}

/// Run the cross-file linking pass on the graph.
///
/// Strategy:
/// 1. Find all placeholder nodes (functions with complexity=None)
/// 2. For each placeholder, find a real definition with the same short name
/// 3. If exactly one match → retarget edges and remove placeholder
/// 4. If multiple matches → use heuristics (same module path, visibility)
/// 5. If no match → keep placeholder (external dependency)
pub fn link(graph: &mut SemanticGraph) -> LinkResult {
    let mut result = LinkResult::default();

    // Collect all placeholder node IDs.
    let all_ids = graph.all_node_ids();
    let placeholders: Vec<NodeId> = all_ids
        .iter()
        .filter(|id| graph.is_placeholder(**id))
        .copied()
        .collect();

    debug!("Linker: {} placeholder nodes to resolve", placeholders.len());

    // Build a map: short_name → Vec<(NodeId, is_placeholder)>
    // Short name = the last meaningful identifier (after splitting on ::)
    let mut name_map: std::collections::HashMap<String, Vec<(NodeId, bool)>> =
        std::collections::HashMap::new();

    for id in &all_ids {
        if let Some(node) = graph.get_node(*id) {
            let is_ph = graph.is_placeholder(*id);
            for short in extract_short_names(node.name()) {
                name_map
                    .entry(short)
                    .or_default()
                    .push((*id, is_ph));
            }
        }
    }

    // Resolve each placeholder.
    let mut to_remove: Vec<NodeId> = Vec::new();

    for ph_id in &placeholders {
        let short_name = match graph.get_node(*ph_id) {
            Some(node) => {
                let names = extract_short_names(node.name());
                match names.into_iter().last() {
                    Some(name) => name,
                    None => continue,
                }
            }
            None => continue,
        };

        // Find real definitions with the same short name.
        let candidates: Vec<NodeId> = name_map
            .get(&short_name)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|(id, is_ph)| !is_ph && *id != *ph_id)
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default();

        match candidates.len() {
            0 => {
                // No real definition found — external dependency, keep placeholder.
                result.unresolved += 1;
            }
            1 => {
                // Exactly one match — retarget and remove.
                let real_id = candidates[0];
                graph.retarget_edges(*ph_id, real_id);
                to_remove.push(*ph_id);
                result.resolved += 1;
                result.edges_retargeted += 1;
            }
            _ => {
                // Multiple matches — try heuristics.
                if let Some(best) = pick_best_candidate(graph, *ph_id, &candidates) {
                    graph.retarget_edges(*ph_id, best);
                    to_remove.push(*ph_id);
                    result.resolved += 1;
                    result.edges_retargeted += 1;
                } else {
                    // Can't decide — pick first and log.
                    let first = candidates[0];
                    graph.retarget_edges(*ph_id, first);
                    to_remove.push(*ph_id);
                    result.resolved += 1;
                    result.edges_retargeted += 1;
                }
            }
        }
    }

    // Also merge duplicate placeholders with the same name.
    // After linking, multiple placeholders may point to the same external function.
    let mut seen_names: std::collections::HashMap<String, NodeId> =
        std::collections::HashMap::new();

    let remaining_phs: Vec<NodeId> = graph
        .all_node_ids()
        .into_iter()
        .filter(|id| graph.is_placeholder(*id) && !to_remove.contains(id))
        .collect();

    for ph_id in remaining_phs {
        let name = match graph.get_node(ph_id) {
            Some(node) => node.name().display(),
            None => continue,
        };

        if let Some(&existing_id) = seen_names.get(&name) {
            // Duplicate — merge into existing.
            graph.retarget_edges(ph_id, existing_id);
            to_remove.push(ph_id);
            result.duplicates_merged += 1;
        } else {
            seen_names.insert(name, ph_id);
        }
    }

    // Remove resolved/merged placeholders.
    for id in to_remove {
        graph.remove_node(id);
    }

    debug!(
        "Linker: resolved={}, unresolved={}, merged={}",
        result.resolved, result.unresolved, result.duplicates_merged
    );

    result
}

/// Pick the best candidate when multiple definitions match.
/// Heuristics:
/// 1. Prefer public over private
/// 2. Prefer same module path prefix
/// 3. Prefer non-test modules
fn pick_best_candidate(
    graph: &SemanticGraph,
    placeholder_id: NodeId,
    candidates: &[NodeId],
) -> Option<NodeId> {
    use omnilens_ir::node::UsirNode;
    use omnilens_ir::Visibility;

    let ph_path = graph
        .get_node(placeholder_id)
        .map(|n| n.span().file.clone())?;

    let mut scored: Vec<(NodeId, i32)> = candidates
        .iter()
        .filter_map(|&id| {
            let node = graph.get_node(id)?;
            let mut score: i32 = 0;

            // Prefer public definitions.
            match node {
                UsirNode::Function(f) => {
                    if f.visibility == Visibility::Public {
                        score += 10;
                    } else if f.visibility == Visibility::Internal {
                        score += 5;
                    }
                }
                _ => {}
            }

            // Prefer definitions in the same directory.
            let node_path = node.span().file.clone();
            if node_path.parent() == ph_path.parent() {
                score += 20;
            }

            // Penalize test files.
            let path_str = node_path.to_string_lossy();
            if path_str.contains("test") {
                score -= 5;
            }

            Some((id, score))
        })
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.first().map(|(id, _)| *id)
}

/// Extract short names for matching.
/// A QualifiedName with segments ["ContentHash::from_bytes"] (single segment containing ::)
/// produces: ["ContentHash::from_bytes", "from_bytes"]
/// A QualifiedName with segments ["Extractor", "extract_function"]
/// produces: ["extract_function"]
fn extract_short_names(qname: &omnilens_ir::QualifiedName) -> Vec<String> {
    let mut names = Vec::new();

    if let Some(last) = qname.segments.last() {
        // Always include the last segment as-is.
        names.push(last.clone());

        // If the last segment contains ::, also extract the part after the last ::.
        if let Some(pos) = last.rfind("::") {
            let suffix = &last[pos + 2..];
            if !suffix.is_empty() {
                names.push(suffix.to_string());
            }
        }

        // If the last segment contains '.', extract the part after the last '.'.
        // This handles method calls like "self.graph.add_node" → "add_node".
        if let Some(pos) = last.rfind('.') {
            let suffix = &last[pos + 1..];
            if !suffix.is_empty() {
                names.push(suffix.to_string());
            }
        }
    }

    names
}
