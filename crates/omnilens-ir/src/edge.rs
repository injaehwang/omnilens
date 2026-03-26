//! USIR Edge definitions — relationships between nodes in the semantic graph.

use serde::{Deserialize, Serialize};

use crate::{NodeId, SourceSpan};

/// An edge in the Universal Semantic IR graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsirEdge {
    /// Function A calls Function B.
    Calls(CallEdge),

    /// Type A references Type B (field type, parameter type, etc.).
    References(ReferenceEdge),

    /// Type A implements Interface/Trait B.
    Implements(ImplementsEdge),

    /// Data flows from source to sink (for taint analysis).
    DataFlow(DataFlowEdge),

    /// Module A imports from Module B.
    Imports(ImportEdge),

    /// A contains B (module contains function, struct contains method).
    Contains(ContainsEdge),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEdge {
    pub caller: NodeId,
    pub callee: NodeId,
    pub call_site: SourceSpan,
    /// Under what condition this call occurs (None = unconditional).
    pub condition: Option<CallCondition>,
    /// Is this a dynamic dispatch (virtual call, trait object, etc.)?
    pub is_dynamic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallCondition {
    /// Inside an if/match branch.
    Conditional(String),
    /// Inside a loop body.
    InLoop,
    /// Inside error handling (catch, Result::Err branch).
    ErrorPath,
    /// Inside a try block or ? operator chain.
    Fallible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: ReferenceKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReferenceKind {
    FieldType,
    ParamType,
    ReturnType,
    LocalType,
    GenericBound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementsEdge {
    pub implementor: NodeId,
    pub interface: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFlowEdge {
    pub source: NodeId,
    pub sink: NodeId,
    pub taint: TaintKind,
    pub through: Vec<NodeId>,
}

/// Classification of data origin for taint analysis.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaintKind {
    /// Direct user input (HTTP body, CLI args, stdin).
    UserInput,
    /// Data from database query results.
    Database,
    /// Data from filesystem reads.
    FileSystem,
    /// Data from network requests.
    Network,
    /// Data from environment variables or config.
    Config,
    /// Sensitive data (passwords, tokens, PII).
    Sensitive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportEdge {
    pub importer: NodeId,
    pub imported: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainsEdge {
    pub parent: NodeId,
    pub child: NodeId,
}
