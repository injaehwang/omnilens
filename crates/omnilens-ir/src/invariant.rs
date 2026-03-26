//! Invariant definitions — rules that must always hold true in the codebase.
//!
//! Invariants are automatically discovered by analyzing existing code patterns
//! and used to verify that new (especially AI-generated) code doesn't violate them.

use serde::{Deserialize, Serialize};

use crate::NodeId;

/// An invariant discovered or declared in the codebase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invariant {
    pub id: InvariantId,
    pub kind: InvariantKind,
    pub description: String,
    /// How confident we are that this is a real invariant (0.0 - 1.0).
    pub confidence: f64,
    /// Number of code locations that conform to this invariant.
    pub evidence_count: usize,
    /// Nodes that this invariant applies to.
    pub scope: Vec<NodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvariantId(pub u64);

/// Categories of invariants that omnilens can discover and enforce.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InvariantKind {
    /// All calls to X must go through Y (e.g., DB access through connection pool).
    MustGoThrough {
        target: NodeId,
        gateway: NodeId,
    },

    /// X must always be called before Y (e.g., auth before resource access).
    MustPrecede {
        before: NodeId,
        after: NodeId,
    },

    /// Return type of functions matching pattern must be X.
    ReturnTypeConstraint {
        pattern: String,
        expected_type: String,
    },

    /// Errors must be handled, not silently ignored.
    ErrorsMustBeHandled {
        error_source: NodeId,
    },

    /// Data of type X must never flow to Y without passing through Z.
    DataFlowConstraint {
        source_taint: String,
        forbidden_sink: String,
        required_sanitizer: Option<String>,
    },

    /// All instances of pattern X must follow convention Y.
    ConventionConstraint {
        pattern: String,
        convention: String,
    },

    /// Type X must only be used in context Y (e.g., Decimal for money, never f64).
    TypeUsageConstraint {
        type_name: String,
        allowed_contexts: Vec<String>,
        forbidden_alternatives: Vec<String>,
    },

    /// Custom invariant defined via OmniQL expression.
    Custom {
        query: String,
    },
}

/// Result of checking an invariant against new code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantViolation {
    pub invariant: InvariantId,
    pub location: crate::SourceSpan,
    pub description: String,
    pub severity: ViolationSeverity,
    /// Suggested fix, if available.
    pub suggested_fix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationSeverity {
    /// Must fix before merge.
    Error,
    /// Likely a problem, review needed.
    Warning,
    /// Informational, may be intentional.
    Info,
}
