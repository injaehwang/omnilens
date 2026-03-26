//! Impact analysis — compute the blast radius of code changes.

use std::collections::{HashSet, VecDeque};

use omnilens_ir::NodeId;

use crate::{GraphNodeIdx, SemanticGraph};

/// Result of an impact analysis.
pub struct ImpactResult {
    /// Directly affected nodes (distance = 1).
    pub direct: Vec<ImpactedNode>,
    /// Transitively affected nodes (distance > 1).
    pub transitive: Vec<ImpactedNode>,
    /// Total unique nodes affected.
    pub total_affected: usize,
    /// Overall risk score (0.0 - 1.0).
    pub risk_score: f64,
}

/// A node affected by a change.
pub struct ImpactedNode {
    pub node_id: NodeId,
    /// Shortest distance from the changed node.
    pub distance: usize,
    /// Path from changed node to this node.
    pub path: Vec<NodeId>,
    /// Confidence that this node is actually affected (0.0 - 1.0).
    pub confidence: f64,
}

impl SemanticGraph {
    /// Compute forward impact: "what does this node affect?"
    /// BFS from the changed node, following outgoing edges.
    pub fn impact_forward(&self, node_id: NodeId, max_depth: usize) -> ImpactResult {
        self.bfs_impact(node_id, max_depth, Direction::Forward)
    }

    /// Compute reverse impact: "what affects this node?"
    /// BFS from the changed node, following incoming edges.
    pub fn impact_reverse(&self, node_id: NodeId, max_depth: usize) -> ImpactResult {
        self.bfs_impact(node_id, max_depth, Direction::Reverse)
    }

    fn bfs_impact(&self, start: NodeId, max_depth: usize, direction: Direction) -> ImpactResult {
        let Some(&start_idx) = self.id_to_idx.get(&start) else {
            return ImpactResult {
                direct: vec![],
                transitive: vec![],
                total_affected: 0,
                risk_score: 0.0,
            };
        };

        let mut visited: HashSet<GraphNodeIdx> = HashSet::new();
        let mut queue: VecDeque<(GraphNodeIdx, usize, Vec<NodeId>)> = VecDeque::new();
        let mut direct = Vec::new();
        let mut transitive = Vec::new();

        visited.insert(start_idx);
        queue.push_back((start_idx, 0, vec![start]));

        while let Some((current_idx, depth, path)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            let neighbors: Vec<GraphNodeIdx> = match direction {
                Direction::Forward => self
                    .graph
                    .neighbors_directed(current_idx, petgraph::Direction::Outgoing)
                    .collect(),
                Direction::Reverse => self
                    .graph
                    .neighbors_directed(current_idx, petgraph::Direction::Incoming)
                    .collect(),
            };

            for neighbor_idx in neighbors {
                if visited.contains(&neighbor_idx) {
                    continue;
                }
                visited.insert(neighbor_idx);

                if let Some(neighbor_node) = self.graph.node_weight(neighbor_idx) {
                    let neighbor_id = neighbor_node.id();
                    let mut new_path = path.clone();
                    new_path.push(neighbor_id);

                    let new_depth = depth + 1;
                    let confidence = 1.0 / (new_depth as f64); // Simple decay

                    let impacted = ImpactedNode {
                        node_id: neighbor_id,
                        distance: new_depth,
                        path: new_path.clone(),
                        confidence,
                    };

                    if new_depth == 1 {
                        direct.push(impacted);
                    } else {
                        transitive.push(impacted);
                    }

                    queue.push_back((neighbor_idx, new_depth, new_path));
                }
            }
        }

        let total = direct.len() + transitive.len();
        let risk_score = compute_risk_score(&direct, &transitive);

        ImpactResult {
            direct,
            transitive,
            total_affected: total,
            risk_score,
        }
    }
}

enum Direction {
    Forward,
    Reverse,
}

fn compute_risk_score(direct: &[ImpactedNode], transitive: &[ImpactedNode]) -> f64 {
    let total = direct.len() + transitive.len();
    if total == 0 {
        return 0.0;
    }

    // Simple heuristic: more affected nodes = higher risk, with diminishing returns.
    let base = (total as f64).ln() / 10.0;
    base.min(1.0)
}
