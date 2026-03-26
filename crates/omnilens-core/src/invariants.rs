//! Invariant Discovery Engine
//!
//! Analyzes the semantic graph to discover patterns that are always followed
//! in the codebase. These become invariants that can be checked against new
//! (especially AI-generated) code.
//!
//! Discovery strategies:
//! 1. **Type usage patterns**: "X type is always used in Y context"
//! 2. **Call ordering**: "A is always called before B"
//! 3. **Error handling**: "Errors from X are always handled"
//! 4. **Gateway patterns**: "All calls to X go through Y"
//! 5. **Visibility conventions**: "Functions matching pattern are always pub"

use omnilens_graph::SemanticGraph;
use omnilens_ir::invariant::{Invariant, InvariantId, InvariantKind};
use omnilens_ir::node::UsirNode;
use omnilens_ir::Visibility;

/// Results from invariant discovery.
pub struct DiscoveryResult {
    pub invariants: Vec<Invariant>,
    pub stats: DiscoveryStats,
}

pub struct DiscoveryStats {
    pub patterns_scanned: usize,
    pub invariants_found: usize,
    pub high_confidence: usize,
}

static NEXT_INV_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_inv_id() -> InvariantId {
    InvariantId(NEXT_INV_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// Run all invariant discovery passes on the graph.
pub fn discover(graph: &SemanticGraph) -> DiscoveryResult {
    let mut invariants = Vec::new();
    let mut patterns_scanned = 0;

    // Pass 1: Error handling invariants.
    let error_invs = discover_error_handling(graph);
    patterns_scanned += graph.node_count();
    invariants.extend(error_invs);

    // Pass 2: Visibility conventions.
    let vis_invs = discover_visibility_conventions(graph);
    patterns_scanned += graph.node_count();
    invariants.extend(vis_invs);

    // Pass 3: Call ordering patterns.
    let order_invs = discover_call_ordering(graph);
    patterns_scanned += graph.edge_count();
    invariants.extend(order_invs);

    // Pass 4: Type usage patterns.
    let type_invs = discover_type_usage(graph);
    patterns_scanned += graph.node_count();
    invariants.extend(type_invs);

    let high_confidence = invariants.iter().filter(|i| i.confidence >= 0.9).count();

    DiscoveryResult {
        stats: DiscoveryStats {
            patterns_scanned,
            invariants_found: invariants.len(),
            high_confidence,
        },
        invariants,
    }
}

/// Discover: "Functions that return Result always have error paths handled by callers"
fn discover_error_handling(graph: &SemanticGraph) -> Vec<Invariant> {
    let mut invariants = Vec::new();
    let all_ids = graph.all_node_ids();

    // Find all functions returning Result<T, E>.
    let result_fns: Vec<_> = all_ids
        .iter()
        .filter_map(|id| {
            let node = graph.get_node(*id)?;
            match node {
                UsirNode::Function(f) => {
                    let ret = f.return_type.as_ref()?;
                    let is_result = match ret {
                        omnilens_ir::types::TypeRef::Resolved(
                            omnilens_ir::types::ResolvedType::Result { .. },
                        ) => true,
                        omnilens_ir::types::TypeRef::Unresolved(s) => s.starts_with("Result"),
                        _ => false,
                    };
                    if is_result {
                        Some(*id)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        })
        .collect();

    if result_fns.len() >= 3 {
        // Check if callers use ? or match on the result.
        // For now, record that Result-returning functions exist as a pattern.
        let confidence = (result_fns.len() as f64 / all_ids.len() as f64).min(1.0) * 0.8 + 0.1;

        invariants.push(Invariant {
            id: next_inv_id(),
            kind: InvariantKind::ErrorsMustBeHandled {
                error_source: result_fns[0],
            },
            description: format!(
                "Found {} functions returning Result — errors should be handled, not silently ignored",
                result_fns.len()
            ),
            confidence,
            evidence_count: result_fns.len(),
            scope: result_fns,
        });
    }

    invariants
}

/// Discover: "All public functions follow naming convention X"
fn discover_visibility_conventions(graph: &SemanticGraph) -> Vec<Invariant> {
    let mut invariants = Vec::new();
    let all_ids = graph.all_node_ids();

    let mut pub_fn_count = 0;
    let mut pub_fn_snake_case = 0;
    let mut pub_struct_count = 0;
    let mut pub_struct_pascal_case = 0;

    for id in &all_ids {
        if let Some(node) = graph.get_node(*id) {
            match node {
                UsirNode::Function(f) if f.visibility == Visibility::Public => {
                    pub_fn_count += 1;
                    if let Some(name) = f.name.segments.last() {
                        if is_snake_case(name) {
                            pub_fn_snake_case += 1;
                        }
                    }
                }
                UsirNode::DataType(dt) if dt.visibility == Visibility::Public => {
                    pub_struct_count += 1;
                    if let Some(name) = dt.name.segments.last() {
                        if is_pascal_case(name) {
                            pub_struct_pascal_case += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // If >90% of public functions use snake_case, it's an invariant.
    if pub_fn_count >= 5 && pub_fn_snake_case as f64 / pub_fn_count as f64 > 0.9 {
        invariants.push(Invariant {
            id: next_inv_id(),
            kind: InvariantKind::ConventionConstraint {
                pattern: "pub fn *".to_string(),
                convention: "snake_case naming".to_string(),
            },
            description: format!(
                "Public functions use snake_case ({}/{} = {:.0}%)",
                pub_fn_snake_case,
                pub_fn_count,
                pub_fn_snake_case as f64 / pub_fn_count as f64 * 100.0
            ),
            confidence: pub_fn_snake_case as f64 / pub_fn_count as f64,
            evidence_count: pub_fn_snake_case,
            scope: Vec::new(),
        });
    }

    // If >90% of public types use PascalCase, it's an invariant.
    if pub_struct_count >= 3 && pub_struct_pascal_case as f64 / pub_struct_count as f64 > 0.9 {
        invariants.push(Invariant {
            id: next_inv_id(),
            kind: InvariantKind::ConventionConstraint {
                pattern: "pub struct/enum/trait *".to_string(),
                convention: "PascalCase naming".to_string(),
            },
            description: format!(
                "Public types use PascalCase ({}/{} = {:.0}%)",
                pub_struct_pascal_case,
                pub_struct_count,
                pub_struct_pascal_case as f64 / pub_struct_count as f64 * 100.0
            ),
            confidence: pub_struct_pascal_case as f64 / pub_struct_count as f64,
            evidence_count: pub_struct_pascal_case,
            scope: Vec::new(),
        });
    }

    invariants
}

/// Discover: "Function A is always called before Function B"
fn discover_call_ordering(graph: &SemanticGraph) -> Vec<Invariant> {
    let mut invariants = Vec::new();
    let all_ids = graph.all_node_ids();

    // Find functions that always appear together in call chains.
    // Heuristic: if A and B are both called by >3 common callers,
    // and A always precedes B in the call order, it's a pattern.

    // For Phase 1: detect "init must be called before other operations" pattern.
    let init_fns: Vec<_> = all_ids
        .iter()
        .filter_map(|id| {
            let node = graph.get_node(*id)?;
            match node {
                UsirNode::Function(f) => {
                    let name = f.name.segments.last()?;
                    if name == "init" || name == "new" || name.starts_with("init") {
                        Some(*id)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        })
        .collect();

    for init_id in &init_fns {
        let forward = graph.impact_forward(*init_id, 3);
        if forward.total_affected >= 3 {
            invariants.push(Invariant {
                id: next_inv_id(),
                kind: InvariantKind::MustPrecede {
                    before: *init_id,
                    after: forward.direct.first().map(|n| n.node_id).unwrap_or(*init_id),
                },
                description: format!(
                    "Initialization function leads to {} downstream operations",
                    forward.total_affected
                ),
                confidence: 0.7,
                evidence_count: forward.total_affected,
                scope: vec![*init_id],
            });
        }
    }

    invariants
}

/// Discover: "Type X is only used in context Y"
fn discover_type_usage(graph: &SemanticGraph) -> Vec<Invariant> {
    let mut invariants = Vec::new();
    let all_ids = graph.all_node_ids();

    // Find types that appear as parameters — track which functions use them.
    let mut type_usage: std::collections::HashMap<String, Vec<omnilens_ir::NodeId>> =
        std::collections::HashMap::new();

    for id in &all_ids {
        if let Some(UsirNode::Function(f)) = graph.get_node(*id) {
            for param in &f.params {
                if let Some(ref type_ref) = param.type_ref {
                    let type_name = format!("{:?}", type_ref);
                    type_usage.entry(type_name).or_default().push(*id);
                }
            }
        }
    }

    // If a type is used in exactly one module/context, that's an invariant.
    for (type_name, users) in &type_usage {
        if users.len() >= 3 {
            // Check if all users are in the same file/module.
            let files: std::collections::HashSet<_> = users
                .iter()
                .filter_map(|id| {
                    graph
                        .get_node(*id)
                        .map(|n| n.span().file.clone())
                })
                .collect();

            if files.len() == 1 {
                invariants.push(Invariant {
                    id: next_inv_id(),
                    kind: InvariantKind::TypeUsageConstraint {
                        type_name: type_name.clone(),
                        allowed_contexts: files.iter().map(|f| f.display().to_string()).collect(),
                        forbidden_alternatives: Vec::new(),
                    },
                    description: format!(
                        "Type {} is only used in {} ({} occurrences)",
                        type_name,
                        files.iter().next().unwrap().display(),
                        users.len()
                    ),
                    confidence: 0.6,
                    evidence_count: users.len(),
                    scope: users.clone(),
                });
            }
        }
    }

    invariants
}

fn is_snake_case(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_lowercase() || c == '_' || c.is_ascii_digit())
}

fn is_pascal_case(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().map_or(false, |c| c.is_uppercase())
        && !s.contains('_')
}
