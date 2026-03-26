//! Behavioral contract definitions — pre/post conditions and invariants for functions.
//!
//! Contracts are inferred from existing code behavior and used to verify
//! that AI-generated code honors the implicit expectations of the codebase.

use serde::{Deserialize, Serialize};

use crate::NodeId;
use crate::types::TypeRef;

/// A behavioral contract for a function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    /// The function this contract applies to.
    pub function: NodeId,
    /// Preconditions that must hold when the function is called.
    pub preconditions: Vec<Condition>,
    /// Postconditions that must hold after the function returns.
    pub postconditions: Vec<Condition>,
    /// Properties that hold throughout execution (no side effects, idempotent, etc.).
    pub properties: Vec<FunctionProperty>,
    /// How this contract was determined.
    pub origin: ContractOrigin,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f64,
}

/// A condition (pre or post) expressed as a constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    /// Parameter must satisfy a constraint.
    ParamConstraint {
        param_name: String,
        constraint: ValueConstraint,
    },

    /// Return value must satisfy a constraint.
    ReturnConstraint {
        constraint: ValueConstraint,
    },

    /// A relationship between parameters.
    RelationalConstraint {
        left: String,
        relation: Relation,
        right: String,
    },

    /// Type must match expected.
    TypeConstraint {
        name: String,
        expected: TypeRef,
    },

    /// Custom expression (OmniQL syntax).
    Expression(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValueConstraint {
    NotNull,
    NotEmpty,
    Positive,
    NonNegative,
    InRange { min: String, max: String },
    MatchesPattern(String),
    OneOf(Vec<String>),
    LengthConstraint { min: Option<usize>, max: Option<usize> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Relation {
    LessThan,
    LessOrEqual,
    GreaterThan,
    GreaterOrEqual,
    Equal,
    NotEqual,
}

/// Properties that describe function behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunctionProperty {
    /// Function has no side effects.
    Pure,
    /// Calling multiple times with same input gives same result.
    Idempotent,
    /// Function doesn't modify its arguments.
    NoMutation,
    /// Function is safe to call from multiple threads.
    ThreadSafe,
    /// Function always terminates.
    Terminating,
    /// Function doesn't perform I/O.
    NoIO,
    /// Function doesn't allocate heap memory.
    NoAlloc,
    /// Function doesn't panic/throw.
    NoPanic,
}

/// How the contract was determined.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractOrigin {
    /// Inferred from analyzing existing code patterns.
    Inferred,
    /// Extracted from documentation/comments.
    Documented,
    /// Specified by user via annotation or config.
    UserDefined,
    /// Derived from type system constraints.
    TypeDerived,
    /// Discovered from test assertions.
    TestDerived,
}
