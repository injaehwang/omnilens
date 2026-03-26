//! USIR Node definitions — the building blocks of the semantic graph.

use serde::{Deserialize, Serialize};

use crate::{NodeId, QualifiedName, SourceSpan, Visibility};
use crate::types::TypeRef;

/// A node in the Universal Semantic IR graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsirNode {
    /// A callable unit (function, method, closure, lambda).
    Function(FunctionNode),

    /// A data structure (struct, class, interface, enum).
    DataType(DataTypeNode),

    /// A module/namespace boundary.
    Module(ModuleNode),

    /// An API endpoint (HTTP, gRPC, GraphQL).
    ApiEndpoint(ApiEndpointNode),

    /// A variable or constant binding.
    Binding(BindingNode),
}

impl UsirNode {
    pub fn id(&self) -> NodeId {
        match self {
            UsirNode::Function(n) => n.id,
            UsirNode::DataType(n) => n.id,
            UsirNode::Module(n) => n.id,
            UsirNode::ApiEndpoint(n) => n.id,
            UsirNode::Binding(n) => n.id,
        }
    }

    pub fn name(&self) -> &QualifiedName {
        match self {
            UsirNode::Function(n) => &n.name,
            UsirNode::DataType(n) => &n.name,
            UsirNode::Module(n) => &n.name,
            UsirNode::ApiEndpoint(n) => &n.name,
            UsirNode::Binding(n) => &n.name,
        }
    }

    pub fn span(&self) -> &SourceSpan {
        match self {
            UsirNode::Function(n) => &n.span,
            UsirNode::DataType(n) => &n.span,
            UsirNode::Module(n) => &n.span,
            UsirNode::ApiEndpoint(n) => &n.span,
            UsirNode::Binding(n) => &n.span,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionNode {
    pub id: NodeId,
    pub name: QualifiedName,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub visibility: Visibility,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub span: SourceSpan,
    /// Cyclomatic complexity (computed during analysis).
    pub complexity: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub type_ref: Option<TypeRef>,
    pub has_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTypeNode {
    pub id: NodeId,
    pub name: QualifiedName,
    pub kind: DataTypeKind,
    pub fields: Vec<Field>,
    pub methods: Vec<NodeId>,
    pub implements: Vec<TypeRef>,
    pub visibility: Visibility,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataTypeKind {
    Struct,
    Class,
    Interface,
    Trait,
    Enum,
    Union,
    TypeAlias,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub type_ref: Option<TypeRef>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    pub id: NodeId,
    pub name: QualifiedName,
    pub exports: Vec<NodeId>,
    pub imports: Vec<Import>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub source: String,
    pub symbols: Vec<String>,
    pub is_wildcard: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpointNode {
    pub id: NodeId,
    pub name: QualifiedName,
    pub protocol: ApiProtocol,
    pub method: Option<HttpMethod>,
    pub path: String,
    pub handler: NodeId,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiProtocol {
    Http,
    Grpc,
    GraphQL,
    WebSocket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingNode {
    pub id: NodeId,
    pub name: QualifiedName,
    pub type_ref: Option<TypeRef>,
    pub is_mutable: bool,
    pub is_constant: bool,
    pub visibility: Visibility,
    pub span: SourceSpan,
}
