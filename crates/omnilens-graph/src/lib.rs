//! # omnilens-graph
//!
//! In-memory semantic graph database built on petgraph.
//! Provides fast traversal, impact analysis, and pattern matching
//! over USIR nodes and edges.

use std::collections::HashMap;
use std::path::PathBuf;

use omnilens_ir::edge::UsirEdge;
use omnilens_ir::node::UsirNode;
use omnilens_ir::{NodeId, QualifiedName};
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::EdgeRef;

pub mod impact;
pub mod linker;
pub mod query;

/// Index type within petgraph.
pub type GraphNodeIdx = petgraph::graph::NodeIndex;
pub type GraphEdgeIdx = petgraph::graph::EdgeIndex;

/// The core semantic graph.
pub struct SemanticGraph {
    /// Directed graph storing USIR nodes and edges.
    graph: StableDiGraph<UsirNode, UsirEdge>,
    /// Fast lookup: NodeId → petgraph index.
    id_to_idx: HashMap<NodeId, GraphNodeIdx>,
    /// Fast lookup: qualified name → NodeId.
    name_index: HashMap<QualifiedName, NodeId>,
    /// Fast lookup: file path → nodes in that file.
    file_index: HashMap<PathBuf, Vec<NodeId>>,
    /// Next available NodeId.
    next_id: u64,
}

impl SemanticGraph {
    pub fn new() -> Self {
        Self {
            graph: StableDiGraph::new(),
            id_to_idx: HashMap::new(),
            name_index: HashMap::new(),
            file_index: HashMap::new(),
            next_id: 0,
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&mut self, node: UsirNode) -> NodeId {
        let id = node.id();
        let name = node.name().clone();
        let file = node.span().file.clone();

        let idx = self.graph.add_node(node);
        self.id_to_idx.insert(id, idx);
        self.name_index.insert(name, id);
        self.file_index.entry(file).or_default().push(id);

        id
    }

    /// Add an edge to the graph.
    pub fn add_edge(&mut self, edge: UsirEdge) -> Option<GraphEdgeIdx> {
        let (from, to) = edge_endpoints(&edge);
        let from_idx = self.id_to_idx.get(&from)?;
        let to_idx = self.id_to_idx.get(&to)?;
        Some(self.graph.add_edge(*from_idx, *to_idx, edge))
    }

    /// Look up a node by its ID.
    pub fn get_node(&self, id: NodeId) -> Option<&UsirNode> {
        let idx = self.id_to_idx.get(&id)?;
        self.graph.node_weight(*idx)
    }

    /// Look up a node by qualified name.
    pub fn get_by_name(&self, name: &QualifiedName) -> Option<&UsirNode> {
        let id = self.name_index.get(name)?;
        self.get_node(*id)
    }

    /// Get all nodes in a file.
    pub fn nodes_in_file(&self, path: &PathBuf) -> &[NodeId] {
        self.file_index.get(path).map_or(&[], |v| v.as_slice())
    }

    /// Remove all nodes from a file (for incremental re-indexing).
    pub fn remove_file(&mut self, path: &PathBuf) {
        if let Some(node_ids) = self.file_index.remove(path) {
            for id in node_ids {
                if let Some(idx) = self.id_to_idx.remove(&id) {
                    self.graph.remove_node(idx);
                }
            }
        }
    }

    /// Find nodes by file path suffix matching.
    /// Useful when exact path form doesn't match (e.g., relative vs absolute).
    pub fn find_file_by_suffix(&self, suffix: &str) -> Option<Vec<NodeId>> {
        let suffix_normalized = suffix.replace('\\', "/");
        for (path, nodes) in &self.file_index {
            let path_str = path.to_string_lossy().replace('\\', "/");
            if path_str.ends_with(&suffix_normalized) || suffix_normalized.ends_with(&path_str) {
                return Some(nodes.clone());
            }
        }
        None
    }

    /// Total node count.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Total edge count.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Allocate a fresh NodeId.
    pub fn next_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Find all NodeIds whose short name (last segment) matches.
    pub fn find_by_short_name(&self, short_name: &str) -> Vec<NodeId> {
        self.name_index
            .iter()
            .filter(|(qname, _)| qname.segments.last().map(|s| s.as_str()) == Some(short_name))
            .map(|(_, id)| *id)
            .collect()
    }

    /// Check if a node is a placeholder (has no complexity, no real span).
    pub fn is_placeholder(&self, id: NodeId) -> bool {
        match self.get_node(id) {
            Some(UsirNode::Function(f)) => f.complexity.is_none(),
            _ => false,
        }
    }

    /// Get all edges in the graph.
    pub fn all_edges(&self) -> Vec<(GraphNodeIdx, GraphNodeIdx, &UsirEdge)> {
        self.graph
            .edge_indices()
            .filter_map(|ei| {
                let (a, b) = self.graph.edge_endpoints(ei)?;
                let w = self.graph.edge_weight(ei)?;
                Some((a, b, w))
            })
            .collect()
    }

    /// Get the petgraph index for a NodeId.
    pub fn get_idx(&self, id: NodeId) -> Option<GraphNodeIdx> {
        self.id_to_idx.get(&id).copied()
    }

    /// Get the NodeId for a petgraph index.
    pub fn get_node_id(&self, idx: GraphNodeIdx) -> Option<NodeId> {
        self.graph.node_weight(idx).map(|n| n.id())
    }

    /// Remove a node by NodeId and return it.
    pub fn remove_node(&mut self, id: NodeId) -> Option<UsirNode> {
        let idx = self.id_to_idx.remove(&id)?;
        let node = self.graph.remove_node(idx)?;
        self.name_index.retain(|_, v| *v != id);
        for nodes in self.file_index.values_mut() {
            nodes.retain(|n| *n != id);
        }
        Some(node)
    }

    /// Replace an edge's target: redirect all edges pointing to `old_id` to point to `new_id`.
    pub fn retarget_edges(&mut self, old_id: NodeId, new_id: NodeId) {
        let Some(old_idx) = self.id_to_idx.get(&old_id).copied() else {
            return;
        };
        let Some(new_idx) = self.id_to_idx.get(&new_id).copied() else {
            return;
        };

        // Collect edges to retarget (can't mutate while iterating).
        let mut edges_to_retarget = Vec::new();

        // Incoming edges to old_id → should point to new_id.
        for edge_idx in self
            .graph
            .edges_directed(old_idx, petgraph::Direction::Incoming)
        {
            edges_to_retarget.push((edge_idx.source(), edge_idx.id(), true));
        }

        // Outgoing edges from old_id → should come from new_id.
        for edge_idx in self
            .graph
            .edges_directed(old_idx, petgraph::Direction::Outgoing)
        {
            edges_to_retarget.push((edge_idx.target(), edge_idx.id(), false));
        }

        for (other_idx, edge_id, is_incoming) in edges_to_retarget {
            if let Some(weight) = self.graph.remove_edge(edge_id) {
                if is_incoming {
                    self.graph.add_edge(other_idx, new_idx, weight);
                } else {
                    self.graph.add_edge(new_idx, other_idx, weight);
                }
            }
        }
    }

    /// Get all node IDs in the graph.
    pub fn all_node_ids(&self) -> Vec<NodeId> {
        self.id_to_idx.keys().copied().collect()
    }
}

impl Default for SemanticGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract (from, to) node IDs from an edge.
fn edge_endpoints(edge: &UsirEdge) -> (NodeId, NodeId) {
    match edge {
        UsirEdge::Calls(e) => (e.caller, e.callee),
        UsirEdge::References(e) => (e.from, e.to),
        UsirEdge::Implements(e) => (e.implementor, e.interface),
        UsirEdge::DataFlow(e) => (e.source, e.sink),
        UsirEdge::Imports(e) => (e.importer, e.imported),
        UsirEdge::Contains(e) => (e.parent, e.child),
    }
}
