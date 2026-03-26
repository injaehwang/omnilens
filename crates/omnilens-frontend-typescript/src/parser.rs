//! TypeScript/JavaScript tree-sitter parser → USIR conversion.
//!
//! Handles: functions, arrow functions, classes, interfaces, methods,
//! imports/exports, call expressions, type annotations.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use omnilens_core::frontend::{ImportRef, ParseResult};
use omnilens_ir::edge::{
    CallCondition, CallEdge, ContainsEdge, UsirEdge,
};
use omnilens_ir::node::*;
use omnilens_ir::types::{PrimitiveType, ResolvedType, TypeRef};
use omnilens_ir::{NodeId, QualifiedName, SourceSpan, Visibility};
use tree_sitter::{Node, Parser};

static NEXT_ID: AtomicU64 = AtomicU64::new(100_000);

fn next_node_id() -> NodeId {
    NodeId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

pub fn parse(path: &Path, source: &[u8], is_tsx: bool) -> Result<ParseResult> {
    let mut parser = Parser::new();
    let lang = if is_tsx {
        tree_sitter_typescript::LANGUAGE_TSX
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT
    };
    parser
        .set_language(&lang.into())
        .context("Failed to set TypeScript language")?;

    let tree = parser.parse(source, None).context("tree-sitter parse failed")?;

    let mut ext = Extractor::new(path, source);
    ext.visit(tree.root_node());

    Ok(ParseResult {
        nodes: ext.nodes,
        edges: ext.edges,
    })
}

pub fn extract_imports(source: &[u8]) -> Result<Vec<ImportRef>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .context("Failed to set TypeScript language")?;

    let tree = parser.parse(source, None).context("tree-sitter parse failed")?;
    let root = tree.root_node();

    let mut imports = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "import_statement" {
            if let Some(imp) = parse_import(&child, source) {
                imports.push(imp);
            }
        }
    }
    Ok(imports)
}

// ─── Extractor ──────────────────────────────────────────────────

struct Extractor<'a> {
    path: &'a Path,
    source: &'a [u8],
    nodes: Vec<UsirNode>,
    edges: Vec<UsirEdge>,
    scope: Vec<String>,
    name_to_id: std::collections::HashMap<String, NodeId>,
    current_container: Option<NodeId>,
}

impl<'a> Extractor<'a> {
    fn new(path: &'a Path, source: &'a [u8]) -> Self {
        Self {
            path,
            source,
            nodes: Vec::new(),
            edges: Vec::new(),
            scope: Vec::new(),
            name_to_id: std::collections::HashMap::new(),
            current_container: None,
        }
    }

    fn visit(&mut self, node: Node) {
        match node.kind() {
            // Functions
            "function_declaration" | "generator_function_declaration" => {
                self.extract_function(node, false);
            }
            "export_statement" => {
                self.visit_export(node);
                return; // don't recurse again
            }
            // Arrow / function expressions in variable declarations
            "lexical_declaration" | "variable_declaration" => {
                self.extract_var_decl(node);
                return;
            }
            // Classes
            "class_declaration" => self.extract_class(node),
            // Interfaces / type aliases
            "interface_declaration" => self.extract_interface(node),
            "type_alias_declaration" => self.extract_type_alias(node),
            // Enum
            "enum_declaration" => self.extract_enum(node),
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit(child);
                }
            }
        }
    }

    fn visit_export(&mut self, node: Node) {
        // `export function ...`, `export class ...`, `export default ...`
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "function_declaration" | "generator_function_declaration" => {
                    self.extract_function(child, true);
                }
                "class_declaration" => self.extract_class(child),
                "interface_declaration" => self.extract_interface(child),
                "type_alias_declaration" => self.extract_type_alias(child),
                "enum_declaration" => self.extract_enum(child),
                "lexical_declaration" | "variable_declaration" => {
                    self.extract_var_decl(child);
                }
                _ => self.visit(child),
            }
        }
    }

    // ─── Functions ──────────────────────────────────────────────

    fn extract_function(&mut self, node: Node, exported: bool) {
        let name = self
            .child_text(&node, "name")
            .unwrap_or_else(|| "anonymous".into());
        let id = next_node_id();
        let params = self.extract_params(&node);
        let return_type = self.extract_ts_return_type(&node);
        let is_async = self.has_child_kind(&node, "async");
        let _is_generator = node.kind().contains("generator");

        let vis = if exported {
            Visibility::Public
        } else {
            Visibility::Private
        };

        let complexity = self.compute_complexity(&node);

        self.name_to_id.insert(name.clone(), id);
        self.nodes.push(UsirNode::Function(FunctionNode {
            id,
            name: self.qname(&name),
            params,
            return_type,
            visibility: vis,
            is_async,
            is_unsafe: false,
            span: self.span(&node),
            complexity: Some(complexity),
        }));

        if let Some(cid) = self.current_container {
            self.edges.push(UsirEdge::Contains(ContainsEdge {
                parent: cid,
                child: id,
            }));
        }

        // Extract calls from body.
        if let Some(body) = node.child_by_field_name("body") {
            self.extract_calls(id, body, None);
        }
    }

    /// Handle `const foo = () => {}` or `const bar = function() {}`
    fn extract_var_decl(&mut self, node: Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                let name = self.child_text(&child, "name").unwrap_or_default();
                let value = child.child_by_field_name("value");

                if let Some(val) = value {
                    match val.kind() {
                        "arrow_function" | "function_expression"
                        | "generator_function" => {
                            let id = next_node_id();
                            let params = self.extract_params(&val);
                            let return_type = self.extract_ts_return_type(&val);
                            let is_async = self.has_child_kind(&val, "async");
                            let complexity = self.compute_complexity(&val);

                            let is_exported = node.parent().map_or(false, |p| {
                                p.kind() == "export_statement"
                            });

                            self.name_to_id.insert(name.clone(), id);
                            self.nodes.push(UsirNode::Function(FunctionNode {
                                id,
                                name: self.qname(&name),
                                params,
                                return_type,
                                visibility: if is_exported {
                                    Visibility::Public
                                } else {
                                    Visibility::Private
                                },
                                is_async,
                                is_unsafe: false,
                                span: self.span(&child),
                                complexity: Some(complexity),
                            }));

                            if let Some(body) = val.child_by_field_name("body") {
                                self.extract_calls(id, body, None);
                            }
                        }
                        _ => {
                            // Regular variable — create a binding.
                            let id = next_node_id();
                            let type_ref = child
                                .child_by_field_name("type")
                                .and_then(|t| self.node_text(&t))
                                .map(|t| parse_ts_type(&t));

                            let is_const = node.kind() == "lexical_declaration"
                                && self
                                    .node_text(&node)
                                    .map_or(false, |t| t.starts_with("const"));

                            self.name_to_id.insert(name.clone(), id);
                            self.nodes.push(UsirNode::Binding(BindingNode {
                                id,
                                name: self.qname(&name),
                                type_ref,
                                is_mutable: !is_const,
                                is_constant: is_const,
                                visibility: Visibility::Private,
                                span: self.span(&child),
                            }));
                        }
                    }
                }
            }
        }
    }

    // ─── Classes ────────────────────────────────────────────────

    fn extract_class(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_else(|| "Anonymous".into());
        let id = next_node_id();

        let is_exported = node
            .parent()
            .map_or(false, |p| p.kind() == "export_statement");

        // Check for `extends` and `implements`.
        let mut implements = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "class_heritage" {
                let text = self.node_text(&child).unwrap_or_default();
                if text.contains("implements") {
                    // Extract interface names after "implements"
                    if let Some(after) = text.split("implements").nth(1) {
                        for iface in after.split(',') {
                            let iface = iface.trim();
                            if !iface.is_empty() {
                                implements.push(TypeRef::Unresolved(iface.to_string()));
                            }
                        }
                    }
                }
            }
        }

        self.name_to_id.insert(name.clone(), id);
        self.nodes.push(UsirNode::DataType(DataTypeNode {
            id,
            name: self.qname(&name),
            kind: DataTypeKind::Class,
            fields: Vec::new(),
            methods: Vec::new(),
            implements,
            visibility: if is_exported {
                Visibility::Public
            } else {
                Visibility::Private
            },
            span: self.span(&node),
        }));

        // Extract methods inside class body.
        let prev = self.current_container;
        self.current_container = Some(id);
        self.scope.push(name);

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                match child.kind() {
                    "method_definition" | "public_field_definition" => {
                        self.extract_method(child);
                    }
                    _ => {}
                }
            }
        }

        self.scope.pop();
        self.current_container = prev;
    }

    fn extract_method(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_else(|| "anonymous".into());
        let id = next_node_id();
        let params = self.extract_params(&node);
        let return_type = self.extract_ts_return_type(&node);
        let is_async = self.has_child_kind(&node, "async");
        let complexity = self.compute_complexity(&node);

        // Visibility from access modifiers.
        let text = self.node_text(&node).unwrap_or_default();
        let vis = if text.contains("private ") {
            Visibility::Private
        } else if text.contains("protected ") {
            Visibility::Protected
        } else {
            Visibility::Public
        };

        self.name_to_id.insert(name.clone(), id);
        self.nodes.push(UsirNode::Function(FunctionNode {
            id,
            name: self.qname(&name),
            params,
            return_type,
            visibility: vis,
            is_async,
            is_unsafe: false,
            span: self.span(&node),
            complexity: Some(complexity),
        }));

        if let Some(cid) = self.current_container {
            self.edges.push(UsirEdge::Contains(ContainsEdge {
                parent: cid,
                child: id,
            }));
        }

        if let Some(body) = node.child_by_field_name("body") {
            self.extract_calls(id, body, None);
        }
    }

    // ─── Interfaces / Types / Enums ─────────────────────────────

    fn extract_interface(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();

        self.name_to_id.insert(name.clone(), id);
        self.nodes.push(UsirNode::DataType(DataTypeNode {
            id,
            name: self.qname(&name),
            kind: DataTypeKind::Interface,
            fields: self.extract_interface_fields(&node),
            methods: Vec::new(),
            implements: Vec::new(),
            visibility: Visibility::Public,
            span: self.span(&node),
        }));
    }

    fn extract_type_alias(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();

        self.name_to_id.insert(name.clone(), id);
        self.nodes.push(UsirNode::DataType(DataTypeNode {
            id,
            name: self.qname(&name),
            kind: DataTypeKind::TypeAlias,
            fields: Vec::new(),
            methods: Vec::new(),
            implements: Vec::new(),
            visibility: Visibility::Public,
            span: self.span(&node),
        }));
    }

    fn extract_enum(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();

        self.name_to_id.insert(name.clone(), id);
        self.nodes.push(UsirNode::DataType(DataTypeNode {
            id,
            name: self.qname(&name),
            kind: DataTypeKind::Enum,
            fields: Vec::new(),
            methods: Vec::new(),
            implements: Vec::new(),
            visibility: Visibility::Public,
            span: self.span(&node),
        }));
    }

    fn extract_interface_fields(&self, node: &Node) -> Vec<Field> {
        let mut fields = Vec::new();
        let Some(body) = node.child_by_field_name("body") else {
            return fields;
        };
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "property_signature" {
                let name = self
                    .child_text(&child, "name")
                    .unwrap_or_default();
                let type_ref = child
                    .child_by_field_name("type")
                    .and_then(|t| self.node_text(&t))
                    .map(|t| parse_ts_type(&t));
                fields.push(Field {
                    name,
                    type_ref,
                    visibility: Visibility::Public,
                });
            }
        }
        fields
    }

    // ─── Call extraction ────────────────────────────────────────

    fn extract_calls(&mut self, caller: NodeId, node: Node, cond: Option<CallCondition>) {
        match node.kind() {
            "call_expression" => {
                if let Some(func) = node.child_by_field_name("function") {
                    let callee_name = self.node_text(&func).unwrap_or_default();
                    let callee_id = self.resolve_or_placeholder(&callee_name, &func);

                    self.edges.push(UsirEdge::Calls(CallEdge {
                        caller,
                        callee: callee_id,
                        call_site: self.span(&node),
                        condition: cond.clone(),
                        is_dynamic: callee_name.contains('.'),
                    }));
                }
            }
            "new_expression" => {
                if let Some(constructor) = node.child_by_field_name("constructor") {
                    let name = self.node_text(&constructor).unwrap_or_default();
                    let callee_id = self.resolve_or_placeholder(&name, &constructor);
                    self.edges.push(UsirEdge::Calls(CallEdge {
                        caller,
                        callee: callee_id,
                        call_site: self.span(&node),
                        condition: cond.clone(),
                        is_dynamic: false,
                    }));
                }
            }
            "await_expression" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.extract_calls(caller, child, cond.clone());
                }
                return;
            }
            "if_statement" | "ternary_expression" | "switch_statement" => {
                let c = Some(CallCondition::Conditional(node.kind().into()));
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.extract_calls(caller, child, c.clone());
                }
                return;
            }
            "for_statement" | "for_in_statement" | "while_statement" | "do_statement" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.extract_calls(caller, child, Some(CallCondition::InLoop));
                }
                return;
            }
            "try_statement" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    let c = if child.kind() == "catch_clause" {
                        Some(CallCondition::ErrorPath)
                    } else {
                        Some(CallCondition::Fallible)
                    };
                    self.extract_calls(caller, child, c);
                }
                return;
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_calls(caller, child, cond.clone());
        }
    }

    fn resolve_or_placeholder(&mut self, name: &str, node: &Node) -> NodeId {
        // Try short name lookup first.
        let short = name.rsplit('.').next().unwrap_or(name);
        if let Some(&id) = self.name_to_id.get(short) {
            return id;
        }
        if let Some(&id) = self.name_to_id.get(name) {
            return id;
        }

        let id = next_node_id();
        self.name_to_id.insert(name.to_string(), id);
        self.nodes.push(UsirNode::Function(FunctionNode {
            id,
            name: QualifiedName::new(vec![name.to_string()]),
            params: Vec::new(),
            return_type: None,
            visibility: Visibility::Private,
            is_async: false,
            is_unsafe: false,
            span: self.span(node),
            complexity: None, // placeholder
        }));
        id
    }

    // ─── Params ─────────────────────────────────────────────────

    fn extract_params(&self, node: &Node) -> Vec<Param> {
        let Some(params_node) = node.child_by_field_name("parameters") else {
            return Vec::new();
        };

        let mut params = Vec::new();
        let mut cursor = params_node.walk();
        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "required_parameter" | "optional_parameter" => {
                    let name = self
                        .child_text(&child, "pattern")
                        .or_else(|| self.child_text(&child, "name"))
                        .unwrap_or_default();
                    let type_ref = child
                        .child_by_field_name("type")
                        .and_then(|t| self.node_text(&t))
                        .map(|t| parse_ts_type(&t));
                    let has_default = child.child_by_field_name("value").is_some();

                    params.push(Param {
                        name,
                        type_ref,
                        has_default,
                    });
                }
                "rest_parameter" => {
                    let name = self.child_text(&child, "pattern").unwrap_or("...rest".into());
                    params.push(Param {
                        name,
                        type_ref: None,
                        has_default: false,
                    });
                }
                _ => {}
            }
        }
        params
    }

    fn extract_ts_return_type(&self, node: &Node) -> Option<TypeRef> {
        node.child_by_field_name("return_type")
            .and_then(|rt| self.node_text(&rt))
            .map(|t| {
                let t = t.trim().trim_start_matches(':').trim();
                parse_ts_type(t)
            })
    }

    // ─── Complexity ─────────────────────────────────────────────

    fn compute_complexity(&self, node: &Node) -> u32 {
        let mut c = 1u32;
        self.count_decisions(node, &mut c);
        c
    }

    fn count_decisions(&self, node: &Node, c: &mut u32) {
        match node.kind() {
            "if_statement" | "else_clause" | "while_statement" | "for_statement"
            | "for_in_statement" | "switch_case" | "catch_clause" | "ternary_expression"
            | "&&" | "||" | "??" => {
                *c += 1;
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.count_decisions(&child, c);
        }
    }

    // ─── Helpers ────────────────────────────────────────────────

    fn qname(&self, name: &str) -> QualifiedName {
        let mut segs: Vec<String> = self.scope.clone();
        segs.push(name.to_string());
        QualifiedName::new(segs)
    }

    fn span(&self, node: &Node) -> SourceSpan {
        let s = node.start_position();
        let e = node.end_position();
        SourceSpan {
            file: self.path.to_owned(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            start_line: s.row as u32 + 1,
            start_col: s.column as u32,
            end_line: e.row as u32 + 1,
            end_col: e.column as u32,
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

    fn has_child_kind(&self, node: &Node, kind: &str) -> bool {
        let mut cursor = node.walk();
        node.children(&mut cursor).any(|c| c.kind() == kind)
    }
}

// ─── TS type parsing ────────────────────────────────────────────

fn parse_ts_type(s: &str) -> TypeRef {
    let s = s.trim().trim_start_matches(':').trim();
    match s {
        "string" => TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::String)),
        "number" => TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Float64)),
        "boolean" | "bool" => TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Bool)),
        "void" | "undefined" => TypeRef::Resolved(ResolvedType::Unit),
        "null" => TypeRef::Resolved(ResolvedType::Unit),
        "any" | "unknown" => TypeRef::Unknown,
        "never" => TypeRef::Resolved(ResolvedType::Unit),
        _ => {
            // Array<T> or T[]
            if let Some(inner) = s.strip_prefix("Array<").and_then(|s| s.strip_suffix('>')) {
                return TypeRef::Resolved(ResolvedType::Array(Box::new(parse_ts_type(inner))));
            }
            if let Some(inner) = s.strip_suffix("[]") {
                return TypeRef::Resolved(ResolvedType::Array(Box::new(parse_ts_type(inner))));
            }
            // Promise<T>
            if let Some(inner) = s.strip_prefix("Promise<").and_then(|s| s.strip_suffix('>')) {
                return parse_ts_type(inner); // Unwrap promise for semantic analysis.
            }
            // Map<K,V>
            if let Some(inner) = s.strip_prefix("Map<").and_then(|s| s.strip_suffix('>')) {
                if let Some((k, v)) = split_generic(inner) {
                    return TypeRef::Resolved(ResolvedType::Map {
                        key: Box::new(parse_ts_type(&k)),
                        value: Box::new(parse_ts_type(&v)),
                    });
                }
            }
            // Union a | b
            if s.contains('|') {
                let parts: Vec<TypeRef> = s.split('|').map(|p| parse_ts_type(p.trim())).collect();
                // Check if it's T | null/undefined → Optional<T>
                let non_null: Vec<&TypeRef> = parts
                    .iter()
                    .filter(|t| !matches!(t, TypeRef::Resolved(ResolvedType::Unit)))
                    .collect();
                if non_null.len() == 1 && parts.len() == 2 {
                    return TypeRef::Resolved(ResolvedType::Optional(Box::new(
                        non_null[0].clone(),
                    )));
                }
                return TypeRef::Resolved(ResolvedType::Union(parts));
            }
            TypeRef::Unresolved(s.to_string())
        }
    }
}

fn split_generic(s: &str) -> Option<(String, String)> {
    let mut depth = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                return Some((s[..i].trim().into(), s[i + 1..].trim().into()));
            }
            _ => {}
        }
    }
    None
}

fn parse_import(node: &Node, source: &[u8]) -> Option<ImportRef> {
    let text = node.utf8_text(source).ok()?;

    // import { Foo, Bar } from "module"
    // import Foo from "module"
    // import * as Foo from "module"
    let from_idx = text.find("from")?;
    let module = text[from_idx + 4..]
        .trim()
        .trim_matches(|c| c == '\'' || c == '"' || c == ';' || c == ' ')
        .to_string();

    let import_part = text[..from_idx].trim().trim_start_matches("import").trim();

    let is_wildcard = import_part.contains('*');

    let symbols = if is_wildcard {
        Vec::new()
    } else if import_part.contains('{') {
        import_part
            .trim_matches(|c: char| c == '{' || c == '}' || c.is_whitespace())
            .split(',')
            .map(|s| {
                let s = s.trim();
                // Handle `Foo as Bar`
                s.split(" as ").next().unwrap_or(s).trim().to_string()
            })
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        // Default import
        vec![import_part.to_string()]
    };

    Some(ImportRef {
        source_module: module,
        symbols,
        is_wildcard,
    })
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function() {
        let src = b"export function greet(name: string): string { return `Hello ${name}`; }";
        let result = parse(Path::new("test.ts"), src, false).unwrap();

        let fns: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| matches!(n, UsirNode::Function(_)))
            .collect();
        assert!(!fns.is_empty());
        match &fns[0] {
            UsirNode::Function(f) => {
                assert_eq!(f.name.display(), "greet");
                assert_eq!(f.params.len(), 1);
                assert_eq!(f.params[0].name, "name");
                assert_eq!(f.visibility, Visibility::Public);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_parse_arrow_function() {
        let src = b"const add = (a: number, b: number): number => a + b;";
        let result = parse(Path::new("test.ts"), src, false).unwrap();

        let fns: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| matches!(n, UsirNode::Function(f) if f.complexity.is_some()))
            .collect();
        assert!(!fns.is_empty(), "Should find arrow function as Function node");
        match &fns[0] {
            UsirNode::Function(f) => {
                assert_eq!(f.name.display(), "add");
                assert_eq!(f.params.len(), 2);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_parse_class() {
        let src = br#"
class UserService {
    private db: Database;

    async getUser(id: string): Promise<User> {
        return this.db.findOne(id);
    }

    deleteUser(id: string): void {
        this.db.delete(id);
    }
}
"#;
        let result = parse(Path::new("test.ts"), src, false).unwrap();

        let classes: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| matches!(n, UsirNode::DataType(_)))
            .collect();
        assert_eq!(classes.len(), 1);

        let methods: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| matches!(n, UsirNode::Function(f) if f.complexity.is_some()))
            .collect();
        assert!(methods.len() >= 2, "Should find at least 2 methods");
    }

    #[test]
    fn test_parse_interface() {
        let src = br#"
interface Config {
    host: string;
    port: number;
    debug?: boolean;
}
"#;
        let result = parse(Path::new("test.ts"), src, false).unwrap();

        let ifaces: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| matches!(n, UsirNode::DataType(dt) if dt.kind == DataTypeKind::Interface))
            .collect();
        assert_eq!(ifaces.len(), 1);
        match &ifaces[0] {
            UsirNode::DataType(dt) => {
                assert_eq!(dt.name.display(), "Config");
                assert!(dt.fields.len() >= 2);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_parse_imports() {
        let src = br#"
import { useState, useEffect } from "react";
import express from "express";
import * as fs from "fs";
"#;
        let imports = extract_imports(src).unwrap();
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].source_module, "react");
        assert_eq!(imports[0].symbols, vec!["useState", "useEffect"]);
        assert_eq!(imports[1].source_module, "express");
        assert!(imports[2].is_wildcard);
    }

    #[test]
    fn test_parse_calls() {
        let src = br#"
function process() {
    const data = fetchData();
    const result = transform(data);
    console.log(result);
}
"#;
        let result = parse(Path::new("test.ts"), src, false).unwrap();

        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|e| matches!(e, UsirEdge::Calls(_)))
            .collect();
        assert!(calls.len() >= 3, "Should find 3 calls: fetchData, transform, console.log");
    }

    #[test]
    fn test_ts_type_parsing() {
        assert_eq!(
            parse_ts_type("string"),
            TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::String))
        );
        assert_eq!(
            parse_ts_type("number"),
            TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Float64))
        );

        match parse_ts_type("string[]") {
            TypeRef::Resolved(ResolvedType::Array(_)) => {}
            other => panic!("Expected Array, got {:?}", other),
        }

        // string | null → Optional<string>
        match parse_ts_type("string | null") {
            TypeRef::Resolved(ResolvedType::Optional(_)) => {}
            other => panic!("Expected Optional, got {:?}", other),
        }
    }
}
