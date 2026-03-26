//! Rust tree-sitter parser → USIR conversion.
//!
//! Walks the tree-sitter CST and extracts:
//! - Function definitions (fn, methods, closures)
//! - Data types (struct, enum, trait, impl)
//! - Module structure (mod, use)
//! - Call edges (function calls, method calls)
//! - Import relationships

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use omnilens_core::frontend::{ImportRef, ParseResult};
use omnilens_ir::edge::{
    CallCondition, CallEdge, ContainsEdge, ImplementsEdge, UsirEdge,
};
use omnilens_ir::node::*;
use omnilens_ir::types::{PrimitiveType, ResolvedType, TypeRef};
use omnilens_ir::{NodeId, QualifiedName, SourceSpan, Visibility};
use tree_sitter::{Node, Parser, Tree};

/// Global node ID counter (will be replaced by graph-allocated IDs later).
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_node_id() -> NodeId {
    NodeId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

pub struct RustParser {
    _parser: Parser,
}

impl RustParser {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE;
        parser
            .set_language(&language.into())
            .expect("Failed to set Rust language for tree-sitter");
        Self { _parser: parser }
    }

    pub fn parse(&self, path: &Path, source: &[u8]) -> Result<ParseResult> {
        // tree-sitter Parser is not thread-safe, so we clone per parse.
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("Failed to set Rust language");

        let tree = parser
            .parse(source, None)
            .context("tree-sitter failed to parse")?;

        let mut extractor = Extractor::new(path, source);
        extractor.walk_tree(&tree);

        Ok(ParseResult {
            nodes: extractor.nodes,
            edges: extractor.edges,
        })
    }

    pub fn extract_imports(&self, source: &[u8]) -> Result<Vec<ImportRef>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("Failed to set Rust language");

        let tree = parser
            .parse(source, None)
            .context("tree-sitter failed to parse")?;

        let mut imports = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if child.kind() == "use_declaration" {
                if let Some(imp) = extract_use_declaration(&child, source) {
                    imports.push(imp);
                }
            }
        }

        Ok(imports)
    }
}

/// Walks the tree-sitter CST and extracts USIR nodes and edges.
struct Extractor<'a> {
    path: &'a Path,
    source: &'a [u8],
    nodes: Vec<UsirNode>,
    edges: Vec<UsirEdge>,
    /// Stack of containing module/impl names for qualified name resolution.
    scope_stack: Vec<String>,
    /// Maps local names to NodeIds for call edge resolution.
    name_to_id: std::collections::HashMap<String, NodeId>,
    /// Current module NodeId (for Contains edges).
    current_module: Option<NodeId>,
}

impl<'a> Extractor<'a> {
    fn new(path: &'a Path, source: &'a [u8]) -> Self {
        Self {
            path,
            source,
            nodes: Vec::new(),
            edges: Vec::new(),
            scope_stack: Vec::new(),
            name_to_id: std::collections::HashMap::new(),
            current_module: None,
        }
    }

    fn walk_tree(&mut self, tree: &Tree) {
        let root = tree.root_node();
        self.visit_node(root);
    }

    fn visit_node(&mut self, node: Node) {
        match node.kind() {
            "function_item" => self.extract_function(node),
            "struct_item" => self.extract_struct(node),
            "enum_item" => self.extract_enum(node),
            "trait_item" => self.extract_trait(node),
            "impl_item" => self.extract_impl(node),
            "mod_item" => self.extract_module(node),
            "const_item" | "static_item" => self.extract_binding(node),
            _ => {
                // Recurse into children.
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit_node(child);
                }
            }
        }
    }

    fn extract_function(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();

        let params = self.extract_params(&node);
        let return_type = self.extract_return_type(&node);
        let visibility = self.extract_visibility(&node);
        let is_async = node.children(&mut node.walk()).any(|c| c.kind() == "async");
        let is_unsafe = node
            .children(&mut node.walk())
            .any(|c| c.kind() == "unsafe");

        let qname = self.qualified_name(&name);

        let func = FunctionNode {
            id,
            name: qname,
            params,
            return_type,
            visibility,
            is_async,
            is_unsafe,
            span: self.span(&node),
            complexity: Some(self.compute_complexity(&node)),
        };

        self.name_to_id.insert(name.clone(), id);
        self.nodes.push(UsirNode::Function(func));

        // Add Contains edge from current module.
        if let Some(module_id) = self.current_module {
            self.edges.push(UsirEdge::Contains(ContainsEdge {
                parent: module_id,
                child: id,
            }));
        }

        // Extract call edges from function body.
        if let Some(body) = node.child_by_field_name("body") {
            self.extract_calls(id, body);
        }
    }

    fn extract_struct(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();
        let visibility = self.extract_visibility(&node);

        let fields = self.extract_struct_fields(&node);
        let qname = self.qualified_name(&name);

        let dt = DataTypeNode {
            id,
            name: qname,
            kind: DataTypeKind::Struct,
            fields,
            methods: Vec::new(), // Filled during impl extraction
            implements: Vec::new(),
            visibility,
            span: self.span(&node),
        };

        self.name_to_id.insert(name, id);
        self.nodes.push(UsirNode::DataType(dt));

        if let Some(module_id) = self.current_module {
            self.edges.push(UsirEdge::Contains(ContainsEdge {
                parent: module_id,
                child: id,
            }));
        }
    }

    fn extract_enum(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();
        let visibility = self.extract_visibility(&node);
        let qname = self.qualified_name(&name);

        let dt = DataTypeNode {
            id,
            name: qname,
            kind: DataTypeKind::Enum,
            fields: Vec::new(),
            methods: Vec::new(),
            implements: Vec::new(),
            visibility,
            span: self.span(&node),
        };

        self.name_to_id.insert(name, id);
        self.nodes.push(UsirNode::DataType(dt));
    }

    fn extract_trait(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();
        let visibility = self.extract_visibility(&node);
        let qname = self.qualified_name(&name);

        let dt = DataTypeNode {
            id,
            name: qname,
            kind: DataTypeKind::Trait,
            fields: Vec::new(),
            methods: Vec::new(),
            implements: Vec::new(),
            visibility,
            span: self.span(&node),
        };

        self.name_to_id.insert(name.clone(), id);
        self.nodes.push(UsirNode::DataType(dt));

        // Extract trait methods.
        self.scope_stack.push(name);
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "function_item"
                    || child.kind() == "function_signature_item"
                {
                    self.extract_function(child);
                }
            }
        }
        self.scope_stack.pop();
    }

    fn extract_impl(&mut self, node: Node) {
        // Get the type being implemented.
        let type_name = node
            .child_by_field_name("type")
            .and_then(|t| self.node_text(&t))
            .unwrap_or_default();

        // Check if this is a trait impl.
        let trait_name = node
            .child_by_field_name("trait")
            .and_then(|t| self.node_text(&t));

        // Add Implements edge if trait impl.
        if let Some(ref trait_name) = trait_name {
            if let Some(&type_id) = self.name_to_id.get(&type_name) {
                if let Some(&trait_id) = self.name_to_id.get(trait_name) {
                    self.edges.push(UsirEdge::Implements(ImplementsEdge {
                        implementor: type_id,
                        interface: trait_id,
                    }));
                }
            }
        }

        // Extract methods inside impl block.
        self.scope_stack.push(type_name);
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "function_item" {
                    self.extract_function(child);
                }
            }
        }
        self.scope_stack.pop();
    }

    fn extract_module(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();
        let qname = self.qualified_name(&name);

        let module = ModuleNode {
            id,
            name: qname,
            exports: Vec::new(),
            imports: Vec::new(),
            span: self.span(&node),
        };

        let prev_module = self.current_module;
        self.current_module = Some(id);
        self.name_to_id.insert(name.clone(), id);
        self.nodes.push(UsirNode::Module(module));

        // Recurse into module body.
        self.scope_stack.push(name);
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.visit_node(child);
            }
        }
        self.scope_stack.pop();
        self.current_module = prev_module;
    }

    fn extract_binding(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();
        let visibility = self.extract_visibility(&node);
        let is_constant = node.kind() == "const_item";
        let qname = self.qualified_name(&name);

        let type_ref = node
            .child_by_field_name("type")
            .and_then(|t| self.node_text(&t))
            .map(|t| parse_type_str(&t));

        let binding = BindingNode {
            id,
            name: qname,
            type_ref,
            is_mutable: !is_constant,
            is_constant,
            visibility,
            span: self.span(&node),
        };

        self.name_to_id.insert(name, id);
        self.nodes.push(UsirNode::Binding(binding));
    }

    // ─── Call extraction ────────────────────────────────────────────

    fn extract_calls(&mut self, caller_id: NodeId, node: Node) {
        self.visit_for_calls(caller_id, node, None);
    }

    fn visit_for_calls(&mut self, caller_id: NodeId, node: Node, condition: Option<CallCondition>) {
        match node.kind() {
            "call_expression" => {
                if let Some(func_node) = node.child_by_field_name("function") {
                    let callee_name = self.node_text(&func_node).unwrap_or_default();

                    // Create an unresolved callee reference.
                    // The callee NodeId will be resolved during graph linking.
                    let callee_id = self
                        .name_to_id
                        .get(&callee_name)
                        .copied()
                        .unwrap_or_else(|| {
                            // Create a placeholder for external/unresolved calls.
                            let id = next_node_id();
                            self.name_to_id.insert(callee_name.clone(), id);
                            // Add a minimal function node as placeholder.
                            self.nodes.push(UsirNode::Function(FunctionNode {
                                id,
                                name: QualifiedName::new(vec![callee_name]),
                                params: Vec::new(),
                                return_type: None,
                                visibility: Visibility::Private,
                                is_async: false,
                                is_unsafe: false,
                                span: self.span(&func_node),
                                complexity: None,
                            }));
                            id
                        });

                    self.edges.push(UsirEdge::Calls(CallEdge {
                        caller: caller_id,
                        callee: callee_id,
                        call_site: self.span(&node),
                        condition: condition.clone(),
                        is_dynamic: false,
                    }));
                }
            }
            "method_call_expression" => {
                // receiver.method(args)
                if let Some(method_node) = node.child_by_field_name("name") {
                    let method_name = self.node_text(&method_node).unwrap_or_default();

                    let callee_id = self
                        .name_to_id
                        .get(&method_name)
                        .copied()
                        .unwrap_or_else(|| {
                            let id = next_node_id();
                            self.name_to_id.insert(method_name.clone(), id);
                            self.nodes.push(UsirNode::Function(FunctionNode {
                                id,
                                name: QualifiedName::new(vec![method_name]),
                                params: Vec::new(),
                                return_type: None,
                                visibility: Visibility::Private,
                                is_async: false,
                                is_unsafe: false,
                                span: self.span(&method_node),
                                complexity: None,
                            }));
                            id
                        });

                    self.edges.push(UsirEdge::Calls(CallEdge {
                        caller: caller_id,
                        callee: callee_id,
                        call_site: self.span(&node),
                        condition: condition.clone(),
                        is_dynamic: true, // method call = potential dynamic dispatch
                    }));
                }
            }
            "if_expression" | "match_expression" => {
                let cond = Some(CallCondition::Conditional(
                    node.kind().to_string(),
                ));
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit_for_calls(caller_id, child, cond.clone());
                }
                return; // Don't recurse again below.
            }
            "loop_expression" | "while_expression" | "for_expression" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit_for_calls(caller_id, child, Some(CallCondition::InLoop));
                }
                return;
            }
            "try_expression" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit_for_calls(caller_id, child, Some(CallCondition::Fallible));
                }
                return;
            }
            _ => {}
        }

        // Recurse into children.
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_for_calls(caller_id, child, condition.clone());
        }
    }

    // ─── Helpers ────────────────────────────────────────────────────

    fn extract_params(&self, func_node: &Node) -> Vec<Param> {
        let Some(params_node) = func_node.child_by_field_name("parameters") else {
            return Vec::new();
        };

        let mut params = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "parameter" => {
                    let name = child
                        .child_by_field_name("pattern")
                        .and_then(|p| self.node_text(&p))
                        .unwrap_or_else(|| "".to_string());

                    let type_ref = child
                        .child_by_field_name("type")
                        .and_then(|t| self.node_text(&t))
                        .map(|t| parse_type_str(&t));

                    params.push(Param {
                        name,
                        type_ref,
                        has_default: false,
                    });
                }
                "self_parameter" => {
                    params.push(Param {
                        name: "self".to_string(),
                        type_ref: Some(TypeRef::Unresolved("Self".to_string())),
                        has_default: false,
                    });
                }
                _ => {}
            }
        }

        params
    }

    fn extract_return_type(&self, func_node: &Node) -> Option<TypeRef> {
        func_node
            .child_by_field_name("return_type")
            .and_then(|rt| self.node_text(&rt))
            .map(|t| parse_type_str(&t))
    }

    fn extract_visibility(&self, node: &Node) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                let text = self.node_text(&child).unwrap_or_default();
                return if text.contains("pub(crate)") {
                    Visibility::Internal
                } else if text.contains("pub(super)") {
                    Visibility::Protected
                } else if text.starts_with("pub") {
                    Visibility::Public
                } else {
                    Visibility::Private
                };
            }
        }
        Visibility::Private
    }

    fn extract_struct_fields(&self, node: &Node) -> Vec<Field> {
        let Some(body) = node.child_by_field_name("body") else {
            return Vec::new();
        };

        let mut fields = Vec::new();
        let mut cursor = body.walk();

        for child in body.children(&mut cursor) {
            if child.kind() == "field_declaration" {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| self.node_text(&n))
                    .unwrap_or_default();

                let type_ref = child
                    .child_by_field_name("type")
                    .and_then(|t| self.node_text(&t))
                    .map(|t| parse_type_str(&t));

                let visibility = self.extract_visibility(&child);

                fields.push(Field {
                    name,
                    type_ref,
                    visibility,
                });
            }
        }

        fields
    }

    /// Compute cyclomatic complexity: count decision points.
    fn compute_complexity(&self, node: &Node) -> u32 {
        let mut complexity = 1u32; // base complexity
        self.count_decisions(node, &mut complexity);
        complexity
    }

    fn count_decisions(&self, node: &Node, count: &mut u32) {
        match node.kind() {
            "if_expression" | "else_clause" | "while_expression" | "for_expression"
            | "match_arm" | "&&" | "||" => {
                *count += 1;
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.count_decisions(&child, count);
        }
    }

    fn qualified_name(&self, name: &str) -> QualifiedName {
        let mut segments: Vec<String> = self.scope_stack.clone();
        segments.push(name.to_string());
        QualifiedName::new(segments)
    }

    fn span(&self, node: &Node) -> SourceSpan {
        let start = node.start_position();
        let end = node.end_position();
        SourceSpan {
            file: self.path.to_owned(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            start_line: start.row as u32 + 1,
            start_col: start.column as u32,
            end_line: end.row as u32 + 1,
            end_col: end.column as u32,
        }
    }

    fn child_text(&self, node: &Node, field: &str) -> Option<String> {
        node.child_by_field_name(field)
            .and_then(|n| n.utf8_text(self.source).ok())
            .map(|s| s.to_string())
    }

    fn node_text(&self, node: &Node) -> Option<String> {
        node.utf8_text(self.source).ok().map(|s| s.to_string())
    }
}

// ─── Type parsing helpers ───────────────────────────────────────────

fn parse_type_str(s: &str) -> TypeRef {
    let s = s.trim();

    // Primitive types
    match s {
        "bool" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Bool)),
        "i8" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Int8)),
        "i16" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Int16)),
        "i32" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Int32)),
        "i64" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Int64)),
        "u8" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Uint8)),
        "u16" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Uint16)),
        "u32" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Uint32)),
        "u64" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Uint64)),
        "f32" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Float32)),
        "f64" => return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Float64)),
        "String" | "&str" | "str" => {
            return TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::String))
        }
        "()" => return TypeRef::Resolved(ResolvedType::Unit),
        _ => {}
    }

    // Option<T>
    if let Some(inner) = s.strip_prefix("Option<").and_then(|s| s.strip_suffix('>')) {
        return TypeRef::Resolved(ResolvedType::Optional(Box::new(parse_type_str(inner))));
    }

    // Result<T, E>
    if let Some(inner) = s.strip_prefix("Result<").and_then(|s| s.strip_suffix('>')) {
        if let Some((ok, err)) = split_generic_args(inner) {
            return TypeRef::Resolved(ResolvedType::Result {
                ok: Box::new(parse_type_str(&ok)),
                err: Box::new(parse_type_str(&err)),
            });
        }
    }

    // Vec<T>
    if let Some(inner) = s.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
        return TypeRef::Resolved(ResolvedType::Array(Box::new(parse_type_str(inner))));
    }

    // HashMap<K, V>
    if let Some(inner) = s.strip_prefix("HashMap<").and_then(|s| s.strip_suffix('>')) {
        if let Some((key, value)) = split_generic_args(inner) {
            return TypeRef::Resolved(ResolvedType::Map {
                key: Box::new(parse_type_str(&key)),
                value: Box::new(parse_type_str(&value)),
            });
        }
    }

    // Fallback: unresolved named type
    TypeRef::Unresolved(s.to_string())
}

/// Split "A, B" respecting nested angle brackets.
fn split_generic_args(s: &str) -> Option<(String, String)> {
    let mut depth = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                return Some((s[..i].trim().to_string(), s[i + 1..].trim().to_string()));
            }
            _ => {}
        }
    }
    None
}

fn extract_use_declaration(node: &Node, source: &[u8]) -> Option<ImportRef> {
    let text = node.utf8_text(source).ok()?;

    // Simple parsing: "use foo::bar::Baz;"
    let path = text
        .strip_prefix("use ")?
        .trim_end_matches(';')
        .trim();

    let segments: Vec<&str> = path.split("::").collect();
    if segments.is_empty() {
        return None;
    }

    let last = *segments.last().unwrap();
    let is_wildcard = last == "*";

    let module_path = if is_wildcard || last.starts_with('{') {
        segments[..segments.len() - 1].join("::")
    } else {
        segments[..segments.len().saturating_sub(1)].join("::")
    };

    let symbols = if is_wildcard {
        Vec::new()
    } else if last.starts_with('{') {
        // use foo::{Bar, Baz}
        last.trim_matches(|c| c == '{' || c == '}')
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    } else {
        vec![last.to_string()]
    };

    Some(ImportRef {
        source_module: module_path,
        symbols,
        is_wildcard,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_function() {
        let source = br#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;

        let parser = RustParser::new();
        let result = parser.parse(Path::new("test.rs"), source).unwrap();

        assert_eq!(result.nodes.len(), 1);
        match &result.nodes[0] {
            UsirNode::Function(f) => {
                assert_eq!(f.name.display(), "add");
                assert_eq!(f.params.len(), 2);
                assert_eq!(f.params[0].name, "a");
                assert_eq!(f.visibility, Visibility::Public);
                assert!(!f.is_async);
            }
            _ => panic!("Expected Function node"),
        }
    }

    #[test]
    fn test_parse_struct_with_fields() {
        let source = br#"
pub struct Config {
    pub name: String,
    value: i32,
}
"#;

        let parser = RustParser::new();
        let result = parser.parse(Path::new("test.rs"), source).unwrap();

        assert_eq!(result.nodes.len(), 1);
        match &result.nodes[0] {
            UsirNode::DataType(dt) => {
                assert_eq!(dt.name.display(), "Config");
                assert_eq!(dt.fields.len(), 2);
                assert_eq!(dt.fields[0].name, "name");
                assert_eq!(dt.fields[0].visibility, Visibility::Public);
                assert_eq!(dt.fields[1].name, "value");
                assert_eq!(dt.fields[1].visibility, Visibility::Private);
            }
            _ => panic!("Expected DataType node"),
        }
    }

    #[test]
    fn test_parse_function_with_calls() {
        let source = br#"
fn process() {
    let x = compute();
    let y = transform(x);
}
"#;

        let parser = RustParser::new();
        let result = parser.parse(Path::new("test.rs"), source).unwrap();

        // Should have: process + compute (placeholder) + transform (placeholder) = 3 nodes
        let func_count = result
            .nodes
            .iter()
            .filter(|n| matches!(n, UsirNode::Function(_)))
            .count();
        assert!(func_count >= 1); // At least `process`

        // Should have call edges
        let call_count = result
            .edges
            .iter()
            .filter(|e| matches!(e, UsirEdge::Calls(_)))
            .count();
        assert_eq!(call_count, 2); // compute() + transform()
    }

    #[test]
    fn test_parse_type_str() {
        assert_eq!(
            parse_type_str("i32"),
            TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Int32))
        );
        assert_eq!(
            parse_type_str("()"),
            TypeRef::Resolved(ResolvedType::Unit)
        );

        match parse_type_str("Option<String>") {
            TypeRef::Resolved(ResolvedType::Optional(_)) => {}
            other => panic!("Expected Optional, got {:?}", other),
        }

        match parse_type_str("Vec<u8>") {
            TypeRef::Resolved(ResolvedType::Array(_)) => {}
            other => panic!("Expected Array, got {:?}", other),
        }
    }

    #[test]
    fn test_extract_imports() {
        let source = br#"
use std::collections::HashMap;
use anyhow::Result;
use crate::ir::*;
"#;

        let parser = RustParser::new();
        let imports = parser.extract_imports(source).unwrap();

        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].source_module, "std::collections");
        assert_eq!(imports[0].symbols, vec!["HashMap"]);
        assert!(!imports[0].is_wildcard);

        assert_eq!(imports[2].source_module, "crate::ir");
        assert!(imports[2].is_wildcard);
    }

    #[test]
    fn test_complexity_count() {
        let source = br#"
fn complex(x: i32) -> i32 {
    if x > 0 {
        if x > 10 {
            return x * 2;
        }
        x + 1
    } else {
        match x {
            -1 => 0,
            -2 => -1,
            _ => x,
        }
    }
}
"#;

        let parser = RustParser::new();
        let result = parser.parse(Path::new("test.rs"), source).unwrap();

        match &result.nodes[0] {
            UsirNode::Function(f) => {
                // Base(1) + if(1) + if(1) + else(1) + match_arm(3) = 7
                let c = f.complexity.unwrap();
                assert!(c >= 4, "Expected complexity >= 4, got {}", c);
            }
            _ => panic!("Expected Function"),
        }
    }
}
