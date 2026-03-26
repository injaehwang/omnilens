//! Python tree-sitter parser → USIR conversion.
//!
//! Handles: functions (def), async functions, classes, methods,
//! decorators, imports, call expressions, type hints.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use omnilens_core::frontend::{ImportRef, ParseResult};
use omnilens_ir::edge::{CallCondition, CallEdge, ContainsEdge, UsirEdge};
use omnilens_ir::node::*;
use omnilens_ir::types::{PrimitiveType, ResolvedType, TypeRef};
use omnilens_ir::{NodeId, QualifiedName, SourceSpan, Visibility};
use tree_sitter::{Node, Parser};

static NEXT_ID: AtomicU64 = AtomicU64::new(200_000);

fn next_node_id() -> NodeId {
    NodeId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

pub fn parse(path: &Path, source: &[u8]) -> Result<ParseResult> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .context("Failed to set Python language")?;

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
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .context("Failed to set Python language")?;

    let tree = parser.parse(source, None).context("parse failed")?;
    let root = tree.root_node();

    let mut imports = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "import_statement" | "import_from_statement" => {
                if let Some(imp) = parse_import_node(&child, source) {
                    imports.push(imp);
                }
            }
            _ => {}
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
    current_class: Option<NodeId>,
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
            current_class: None,
        }
    }

    fn visit(&mut self, node: Node) {
        match node.kind() {
            "function_definition" => self.extract_function(node),
            "class_definition" => self.extract_class(node),
            "decorated_definition" => {
                // Unwrap decorator to get the actual definition.
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "function_definition" | "class_definition" => {
                            self.visit(child);
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit(child);
                }
            }
        }
    }

    fn extract_function(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();
        let params = self.extract_params(&node);
        let return_type = self.extract_return_type(&node);
        let is_async = node
            .parent()
            .map_or(false, |p| self.node_text(&p).map_or(false, |t| t.starts_with("async")))
            || self.node_text(&node).map_or(false, |t| t.starts_with("async"));

        // Visibility: Python convention — _private, __dunder__, public
        let vis = if name.starts_with("__") && name.ends_with("__") {
            Visibility::Public // dunder methods
        } else if name.starts_with('_') {
            Visibility::Private
        } else {
            Visibility::Public
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

        if let Some(cid) = self.current_class {
            self.edges.push(UsirEdge::Contains(ContainsEdge {
                parent: cid,
                child: id,
            }));
        }

        // Extract calls from function body.
        if let Some(body) = node.child_by_field_name("body") {
            self.extract_calls(id, body, None);
        }
    }

    fn extract_class(&mut self, node: Node) {
        let name = self.child_text(&node, "name").unwrap_or_default();
        let id = next_node_id();

        // Extract base classes.
        let mut implements = Vec::new();
        if let Some(args) = node.child_by_field_name("superclasses") {
            let text = self.node_text(&args).unwrap_or_default();
            let text = text.trim_matches(|c| c == '(' || c == ')');
            for base in text.split(',') {
                let base = base.trim();
                if !base.is_empty() {
                    implements.push(TypeRef::Unresolved(base.to_string()));
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
            visibility: if name.starts_with('_') {
                Visibility::Private
            } else {
                Visibility::Public
            },
            span: self.span(&node),
        }));

        // Extract methods.
        let prev = self.current_class;
        self.current_class = Some(id);
        self.scope.push(name);

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                match child.kind() {
                    "function_definition" => self.extract_function(child),
                    "decorated_definition" => self.visit(child),
                    _ => {}
                }
            }
        }

        self.scope.pop();
        self.current_class = prev;
    }

    // ─── Call extraction ────────────────────────────────────────

    fn extract_calls(&mut self, caller: NodeId, node: Node, cond: Option<CallCondition>) {
        match node.kind() {
            "call" => {
                if let Some(func) = node.child_by_field_name("function") {
                    let name = self.node_text(&func).unwrap_or_default();
                    let callee_id = self.resolve_or_placeholder(&name, &func);
                    self.edges.push(UsirEdge::Calls(CallEdge {
                        caller,
                        callee: callee_id,
                        call_site: self.span(&node),
                        condition: cond.clone(),
                        is_dynamic: name.contains('.'),
                    }));
                }
            }
            "if_statement" | "elif_clause" | "conditional_expression" => {
                let c = Some(CallCondition::Conditional(node.kind().into()));
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.extract_calls(caller, child, c.clone());
                }
                return;
            }
            "for_statement" | "while_statement" | "list_comprehension"
            | "dictionary_comprehension" | "set_comprehension" | "generator_expression" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.extract_calls(caller, child, Some(CallCondition::InLoop));
                }
                return;
            }
            "try_statement" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    let c = if child.kind() == "except_clause" {
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
            complexity: None,
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
                "identifier" => {
                    let name = self.node_text(&child).unwrap_or_default();
                    if name != "self" && name != "cls" {
                        params.push(Param {
                            name,
                            type_ref: None,
                            has_default: false,
                        });
                    }
                }
                "typed_parameter" => {
                    let name = child
                        .child(0)
                        .and_then(|n| self.node_text(&n))
                        .unwrap_or_default();
                    if name == "self" || name == "cls" {
                        continue;
                    }
                    let type_ref = child
                        .child_by_field_name("type")
                        .and_then(|t| self.node_text(&t))
                        .map(|t| parse_py_type(&t));
                    params.push(Param {
                        name,
                        type_ref,
                        has_default: false,
                    });
                }
                "default_parameter" | "typed_default_parameter" => {
                    let name = child
                        .child_by_field_name("name")
                        .and_then(|n| self.node_text(&n))
                        .unwrap_or_default();
                    if name == "self" || name == "cls" {
                        continue;
                    }
                    let type_ref = child
                        .child_by_field_name("type")
                        .and_then(|t| self.node_text(&t))
                        .map(|t| parse_py_type(&t));
                    params.push(Param {
                        name,
                        type_ref,
                        has_default: true,
                    });
                }
                _ => {}
            }
        }
        params
    }

    fn extract_return_type(&self, node: &Node) -> Option<TypeRef> {
        node.child_by_field_name("return_type")
            .and_then(|rt| self.node_text(&rt))
            .map(|t| parse_py_type(t.trim()))
    }

    // ─── Complexity ─────────────────────────────────────────────

    fn compute_complexity(&self, node: &Node) -> u32 {
        let mut c = 1u32;
        self.count_decisions(node, &mut c);
        c
    }

    fn count_decisions(&self, node: &Node, c: &mut u32) {
        match node.kind() {
            "if_statement" | "elif_clause" | "while_statement" | "for_statement"
            | "except_clause" | "conditional_expression" | "boolean_operator" | "match_statement" => {
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
}

// ─── Python type parsing ────────────────────────────────────────

fn parse_py_type(s: &str) -> TypeRef {
    let s = s.trim();
    match s {
        "str" => TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::String)),
        "int" => TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Int64)),
        "float" => TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Float64)),
        "bool" => TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Bool)),
        "bytes" => TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Bytes)),
        "None" => TypeRef::Resolved(ResolvedType::Unit),
        "Any" => TypeRef::Unknown,
        _ => {
            // list[T]
            if let Some(inner) = strip_generic(s, "list")
                .or_else(|| strip_generic(s, "List"))
            {
                return TypeRef::Resolved(ResolvedType::Array(Box::new(parse_py_type(&inner))));
            }
            // dict[K, V]
            if let Some(inner) = strip_generic(s, "dict")
                .or_else(|| strip_generic(s, "Dict"))
            {
                if let Some((k, v)) = split_generic(&inner) {
                    return TypeRef::Resolved(ResolvedType::Map {
                        key: Box::new(parse_py_type(&k)),
                        value: Box::new(parse_py_type(&v)),
                    });
                }
            }
            // Optional[T]
            if let Some(inner) = strip_generic(s, "Optional") {
                return TypeRef::Resolved(ResolvedType::Optional(Box::new(parse_py_type(&inner))));
            }
            // T | None (Python 3.10+ union syntax)
            if s.contains('|') {
                let parts: Vec<&str> = s.split('|').map(|p| p.trim()).collect();
                let non_none: Vec<&&str> = parts.iter().filter(|p| **p != "None").collect();
                if non_none.len() == 1 && parts.len() == 2 {
                    return TypeRef::Resolved(ResolvedType::Optional(Box::new(parse_py_type(
                        non_none[0],
                    ))));
                }
                return TypeRef::Resolved(ResolvedType::Union(
                    parts.iter().map(|p| parse_py_type(p)).collect(),
                ));
            }
            // tuple[A, B, ...]
            if let Some(inner) = strip_generic(s, "tuple")
                .or_else(|| strip_generic(s, "Tuple"))
            {
                let parts: Vec<TypeRef> = inner.split(',').map(|p| parse_py_type(p.trim())).collect();
                return TypeRef::Resolved(ResolvedType::Tuple(parts));
            }
            TypeRef::Unresolved(s.to_string())
        }
    }
}

fn strip_generic<'a>(s: &'a str, prefix: &str) -> Option<String> {
    s.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('['))
        .and_then(|rest| rest.strip_suffix(']'))
        .map(|inner| inner.to_string())
}

fn split_generic(s: &str) -> Option<(String, String)> {
    let mut depth = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '[' | '<' => depth += 1,
            ']' | '>' => depth -= 1,
            ',' if depth == 0 => {
                return Some((s[..i].trim().into(), s[i + 1..].trim().into()));
            }
            _ => {}
        }
    }
    None
}

fn parse_import_node(node: &Node, source: &[u8]) -> Option<ImportRef> {
    let text = node.utf8_text(source).ok()?;

    if text.starts_with("from ") {
        // from module import X, Y
        let parts: Vec<&str> = text.splitn(2, " import ").collect();
        if parts.len() == 2 {
            let module = parts[0].trim_start_matches("from ").trim().to_string();
            let imports_text = parts[1].trim().trim_end_matches('\n');

            let is_wildcard = imports_text == "*";
            let symbols = if is_wildcard {
                Vec::new()
            } else {
                imports_text
                    .split(',')
                    .map(|s| {
                        let s = s.trim();
                        s.split(" as ").next().unwrap_or(s).trim().to_string()
                    })
                    .filter(|s| !s.is_empty())
                    .collect()
            };

            return Some(ImportRef {
                source_module: module,
                symbols,
                is_wildcard,
            });
        }
    } else if text.starts_with("import ") {
        // import module
        let module = text
            .trim_start_matches("import ")
            .trim()
            .split(" as ")
            .next()?
            .trim()
            .to_string();

        return Some(ImportRef {
            source_module: module.clone(),
            symbols: vec![module],
            is_wildcard: false,
        });
    }

    None
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function() {
        let src = br#"
def greet(name: str) -> str:
    return f"Hello {name}"
"#;
        let result = parse(Path::new("test.py"), src).unwrap();

        let fns: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| matches!(n, UsirNode::Function(f) if f.complexity.is_some()))
            .collect();
        assert!(!fns.is_empty());
        match &fns[0] {
            UsirNode::Function(f) => {
                assert_eq!(f.name.display(), "greet");
                assert_eq!(f.params.len(), 1);
                assert_eq!(f.params[0].name, "name");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_parse_class() {
        let src = br#"
class UserService:
    def __init__(self, db):
        self.db = db

    async def get_user(self, user_id: int) -> User:
        return await self.db.find_one(user_id)

    def _private_method(self):
        pass
"#;
        let result = parse(Path::new("test.py"), src).unwrap();

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
        assert!(methods.len() >= 3);

        // Check private method visibility.
        let private: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| {
                matches!(n, UsirNode::Function(f) if f.visibility == Visibility::Private && f.complexity.is_some())
            })
            .collect();
        assert!(!private.is_empty());
    }

    #[test]
    fn test_parse_imports() {
        let src = br#"
from flask import Flask, request
import os
from typing import *
"#;
        let imports = extract_imports(src).unwrap();
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].source_module, "flask");
        assert_eq!(imports[0].symbols, vec!["Flask", "request"]);
        assert_eq!(imports[1].source_module, "os");
        assert!(imports[2].is_wildcard);
    }

    #[test]
    fn test_parse_calls() {
        let src = br#"
def process():
    data = fetch_data()
    result = transform(data)
    print(result)
"#;
        let result = parse(Path::new("test.py"), src).unwrap();

        let calls = result
            .edges
            .iter()
            .filter(|e| matches!(e, UsirEdge::Calls(_)))
            .count();
        assert!(calls >= 3);
    }

    #[test]
    fn test_py_type_parsing() {
        assert_eq!(
            parse_py_type("str"),
            TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::String))
        );
        assert_eq!(
            parse_py_type("int"),
            TypeRef::Resolved(ResolvedType::Primitive(PrimitiveType::Int64))
        );

        match parse_py_type("list[str]") {
            TypeRef::Resolved(ResolvedType::Array(_)) => {}
            other => panic!("Expected Array, got {:?}", other),
        }

        match parse_py_type("str | None") {
            TypeRef::Resolved(ResolvedType::Optional(_)) => {}
            other => panic!("Expected Optional, got {:?}", other),
        }

        match parse_py_type("dict[str, int]") {
            TypeRef::Resolved(ResolvedType::Map { .. }) => {}
            other => panic!("Expected Map, got {:?}", other),
        }
    }
}
