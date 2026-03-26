//! Type system representation for USIR.

use serde::{Deserialize, Serialize};

/// A reference to a type (may be unresolved).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeRef {
    /// Fully resolved type.
    Resolved(ResolvedType),
    /// Unresolved type name (best-effort for dynamic languages).
    Unresolved(String),
    /// Type could not be determined.
    Unknown,
}

/// A resolved type in the USIR type system.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResolvedType {
    /// Primitive types (int, float, string, bool, etc.).
    Primitive(PrimitiveType),
    /// Named type from the codebase.
    Named {
        name: String,
        generic_args: Vec<TypeRef>,
    },
    /// Function/callable type.
    Function {
        params: Vec<TypeRef>,
        return_type: Box<TypeRef>,
    },
    /// Array/list type.
    Array(Box<TypeRef>),
    /// Map/dictionary type.
    Map {
        key: Box<TypeRef>,
        value: Box<TypeRef>,
    },
    /// Optional/nullable type.
    Optional(Box<TypeRef>),
    /// Result/Either type (success + error).
    Result {
        ok: Box<TypeRef>,
        err: Box<TypeRef>,
    },
    /// Tuple type.
    Tuple(Vec<TypeRef>),
    /// Union type (TypeScript union, Python Union, etc.).
    Union(Vec<TypeRef>),
    /// Void / unit type.
    Unit,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimitiveType {
    Bool,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    String,
    Bytes,
}
