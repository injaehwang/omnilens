//! Project snapshot — complete project analysis in one JSON for AI consumption.
//!
//! This is the single source of truth that AI reads to understand a project.
//! Generated in ~100ms, contains everything AI needs to work.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use houndlens_graph::SemanticGraph;
use houndlens_ir::node::UsirNode;
use houndlens_ir::Visibility;

/// Complete project snapshot — AI reads this one file to understand everything.
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    /// When this snapshot was generated.
    pub generated_at: String,
    /// How long analysis took.
    pub analysis_ms: u64,

    /// Project overview.
    pub project: ProjectOverview,

    /// Every file in the project with its symbols.
    pub files: BTreeMap<String, FileInfo>,

    /// Cross-file dependencies (who calls what).
    pub dependencies: Vec<Dependency>,

    /// Project health indicators.
    pub health: Health,

    /// What houndlens can do for AI (available commands).
    pub capabilities: Vec<Capability>,

    /// Project tooling detected.
    pub tooling: Tooling,

    /// Instructions for AI on how to behave after reading this snapshot.
    pub ai_instructions: AiInstructions,
}

#[derive(Serialize, Deserialize)]
pub struct Tooling {
    /// Type checker command (e.g., "npx vue-tsc --noEmit", "npx tsc --noEmit").
    pub type_check: Option<String>,
    /// Linter command (e.g., "npx eslint").
    pub linter: Option<String>,
    /// Formatter command (e.g., "npx prettier --write").
    pub formatter: Option<String>,
    /// Test runner command (e.g., "npx vitest run", "pytest").
    pub test_runner: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct AiInstructions {
    /// What to say to the developer after reading the snapshot.
    pub on_ready: String,
    /// How to behave.
    pub behavior: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ProjectOverview {
    pub languages: Vec<String>,
    pub framework: Option<String>,
    pub total_files: usize,
    pub total_functions: usize,
    pub total_types: usize,
}

#[derive(Serialize, Deserialize)]
pub struct FileInfo {
    pub language: String,
    pub functions: Vec<FunctionInfo>,
    pub types: Vec<TypeInfo>,
    pub imports: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub line: u32,
    pub visibility: String,
    pub params: Vec<String>,
    pub return_type: Option<String>,
    pub complexity: u32,
    pub is_async: bool,
    /// Functions this one calls.
    pub calls: Vec<String>,
    /// Functions that call this one.
    pub called_by: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TypeInfo {
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub fields: Vec<String>,
    pub visibility: String,
}

#[derive(Serialize, Deserialize)]
pub struct Dependency {
    pub from_file: String,
    pub from_function: String,
    pub to_file: String,
    pub to_function: String,
}

#[derive(Serialize, Deserialize)]
pub struct Health {
    pub score: u32,
    pub hotspots: Vec<Hotspot>,
    pub invariants: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Hotspot {
    pub file: String,
    pub function: String,
    pub line: u32,
    pub reason: String,
}

#[derive(Serialize, Deserialize)]
pub struct Capability {
    pub command: String,
    pub description: String,
}

/// Generate a complete project snapshot.
pub fn generate(graph: &SemanticGraph, duration_ms: u64) -> Snapshot {
    let all_ids = graph.all_node_ids();
    let cwd = std::env::current_dir().unwrap_or_default();
    let cwd_str = cwd.to_string_lossy().replace('\\', "/");

    let mut files: BTreeMap<String, FileInfo> = BTreeMap::new();
    let mut languages = std::collections::HashSet::new();
    let mut total_functions = 0usize;
    let mut total_types = 0usize;
    let mut hotspots = Vec::new();
    let mut dependencies = Vec::new();

    for id in &all_ids {
        let Some(node) = graph.get_node(*id) else { continue };
        if graph.is_placeholder(*id) { continue; }

        // Convert to relative path.
        let abs_path = node.span().file.to_string_lossy().replace('\\', "/");
        let file_path = abs_path
            .strip_prefix(&format!("{}/", cwd_str))
            .unwrap_or(&abs_path)
            .to_string();
        let ext = file_path.rsplit('.').next().unwrap_or("");
        let lang = match ext {
            "rs" => "rust",
            "ts" | "tsx" | "js" | "jsx" => "typescript",
            "py" => "python",
            "vue" => "vue",
            _ => "other",
        };
        languages.insert(lang.to_string());

        let file_info = files.entry(file_path.clone()).or_insert_with(|| FileInfo {
            language: lang.to_string(),
            functions: Vec::new(),
            types: Vec::new(),
            imports: Vec::new(),
        });

        match node {
            UsirNode::Function(f) if f.complexity.is_some() => {
                total_functions += 1;
                let complexity = f.complexity.unwrap_or(0);

                // Get calls (forward impact depth=1), deduplicate.
                let forward = graph.impact_forward(f.id, 1);
                let mut calls: Vec<String> = forward.direct.iter()
                    .filter_map(|n| graph.get_node(n.node_id))
                    .filter(|n| !graph.is_placeholder(n.id()))
                    .map(|n| n.name().display())
                    .collect();
                calls.sort();
                calls.dedup();

                // Get callers (reverse impact depth=1), exclude tests, deduplicate.
                let reverse = graph.impact_reverse(f.id, 1);
                let mut called_by: Vec<String> = reverse.direct.iter()
                    .filter_map(|n| graph.get_node(n.node_id))
                    .filter(|n| !graph.is_placeholder(n.id()))
                    .map(|n| n.name().display())
                    .filter(|name| !is_test_name(name))
                    .collect();
                called_by.sort();
                called_by.dedup();

                // Cross-file dependencies (exclude tests).
                for caller in &reverse.direct {
                    if let Some(caller_node) = graph.get_node(caller.node_id) {
                        let caller_name = caller_node.name().display();
                        if is_test_name(&caller_name) { continue; }

                        let caller_abs = caller_node.span().file.to_string_lossy().replace('\\', "/");
                        let caller_rel = caller_abs
                            .strip_prefix(&format!("{}/", cwd_str))
                            .unwrap_or(&caller_abs)
                            .to_string();
                        if caller_rel != file_path {
                            dependencies.push(Dependency {
                                from_file: caller_rel,
                                from_function: caller_name,
                                to_file: file_path.clone(),
                                to_function: f.name.display(),
                            });
                        }
                    }
                }

                // Hotspots.
                if complexity > 15 {
                    hotspots.push(Hotspot {
                        file: file_path.to_string(),
                        function: f.name.display(),
                        line: f.span.start_line,
                        reason: format!("complexity {}", complexity),
                    });
                }
                if f.visibility == Visibility::Public && reverse.total_affected > 5 {
                    hotspots.push(Hotspot {
                        file: file_path.to_string(),
                        function: f.name.display(),
                        line: f.span.start_line,
                        reason: format!("{} callers", reverse.total_affected),
                    });
                }

                file_info.functions.push(FunctionInfo {
                    name: f.name.display(),
                    line: f.span.start_line,
                    visibility: format!("{:?}", f.visibility).to_lowercase(),
                    params: f.params.iter().map(|p| p.name.clone()).collect(),
                    return_type: f.return_type.as_ref().map(|t| format_type(t)),
                    complexity,
                    is_async: f.is_async,
                    calls,
                    called_by,
                });
            }
            UsirNode::DataType(dt) => {
                total_types += 1;
                file_info.types.push(TypeInfo {
                    name: dt.name.display(),
                    kind: format!("{:?}", dt.kind),
                    line: dt.span.start_line,
                    fields: dt.fields.iter().map(|f| f.name.clone()).collect(),
                    visibility: format!("{:?}", dt.visibility).to_lowercase(),
                });
            }
            _ => {}
        }
    }

    // Health score.
    let complexity_penalty = (hotspots.len() as f64 * 3.0).min(30.0);
    let health_score = (100.0 - complexity_penalty).max(0.0) as u32;

    // Invariants.
    let invs = crate::invariants::discover(graph);
    let invariant_descriptions: Vec<String> = invs.invariants.iter()
        .filter(|i| i.confidence >= 0.9)
        .map(|i| i.description.clone())
        .collect();

    let now = chrono::Utc::now().to_rfc3339();
    let total_files = files.len();

    // Detect project tooling.
    let project_root = std::env::current_dir().unwrap_or_default();
    let tooling = detect_tooling(&project_root);

    // Extract imports from source files.
    for (file_path, file_info) in files.iter_mut() {
        let full_path = project_root.join(file_path);
        if let Ok(content) = std::fs::read_to_string(&full_path) {
            file_info.imports = extract_imports(&content, &file_info.language);
        }
    }

    Snapshot {
        generated_at: now,
        analysis_ms: duration_ms,
        project: ProjectOverview {
            languages: languages.into_iter().collect(),
            framework: None,
            total_files: files.len(),
            total_functions,
            total_types,
        },
        files,
        dependencies,
        health: Health {
            score: health_score,
            hotspots,
            invariants: invariant_descriptions,
        },
        tooling,
        capabilities: vec![
            Capability {
                command: "houndlens verify --format json --diff <ref>".into(),
                description: "Compare current code against a git ref. Returns semantic changes, risk score, breaking changes.".into(),
            },
            Capability {
                command: "houndlens impact <file> --fn <name>".into(),
                description: "Show what calls this function and what it calls.".into(),
            },
            Capability {
                command: "houndlens query \"FIND functions WHERE ...\"".into(),
                description: "Search codebase with HoundQL. Fields: name, complexity, visibility, params, async. Ops: = != > < ~".into(),
            },
            Capability {
                command: "houndlens fix".into(),
                description: "Generate tests for untested public functions and run them.".into(),
            },
        ],
        ai_instructions: AiInstructions {
            on_ready: format!(
                "Project analyzed. {} files, {} functions, {} types. Health {}/100. Ready to help — ask the developer what they need.",
                total_files, total_functions, total_types, health_score
            ),
            behavior: vec![
                "After reading this, tell the developer the project is analyzed and ask what they need.".into(),
                "Use summary.json data to answer questions without opening files.".into(),

                // Pre-modification.
                "BEFORE modifying any file, check the call graph in summary.json to understand dependencies.".into(),

                // Post-modification — rescan + verify (once per logical change, not per file).
                "After completing a logical unit of work (not per file), run `houndlens` (~10ms) then read .houndlens/changes.json.".into(),
                "changes.json shows exactly what was affected: broken deps, added/removed functions, health delta.".into(),
                "Then run `houndlens verify --format json --diff HEAD` for syntax and breaking change check.".into(),
                "Run project linter/formatter from tooling on ONLY the files you changed.".into(),
                "If errors found, fix and repeat the cycle. If editing 5 files for one task, finish all 5 first, then check once.".into(),

                // Scope.
                "NEVER run any tool on the entire project. Only on files you changed.".into(),
                "Full project analysis is houndlens's job. Your job is only the files you touched.".into(),

                // Communication.
                "Do NOT show houndlens commands to the developer. Use them internally.".into(),
                "Speak as a colleague who knows the codebase, not as a tool.".into(),
            ],
        },
    }
}

/// Format a TypeRef into human-readable string.
fn format_type(t: &houndlens_ir::types::TypeRef) -> String {
    use houndlens_ir::types::{TypeRef, ResolvedType, PrimitiveType};

    match t {
        TypeRef::Resolved(resolved) => match resolved {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::Bool => "bool".into(),
                PrimitiveType::Int8 => "i8".into(),
                PrimitiveType::Int16 => "i16".into(),
                PrimitiveType::Int32 => "i32".into(),
                PrimitiveType::Int64 => "i64".into(),
                PrimitiveType::Uint8 => "u8".into(),
                PrimitiveType::Uint16 => "u16".into(),
                PrimitiveType::Uint32 => "u32".into(),
                PrimitiveType::Uint64 => "u64".into(),
                PrimitiveType::Float32 => "f32".into(),
                PrimitiveType::Float64 => "f64".into(),
                PrimitiveType::String => "string".into(),
                PrimitiveType::Bytes => "bytes".into(),
            },
            ResolvedType::Named { name, generic_args } => {
                if generic_args.is_empty() {
                    name.clone()
                } else {
                    let args: Vec<String> = generic_args.iter().map(format_type).collect();
                    format!("{}<{}>", name, args.join(", "))
                }
            }
            ResolvedType::Function { params, return_type } => {
                let p: Vec<String> = params.iter().map(format_type).collect();
                format!("({}) -> {}", p.join(", "), format_type(return_type))
            }
            ResolvedType::Array(inner) => format!("{}[]", format_type(inner)),
            ResolvedType::Map { key, value } => format!("Map<{}, {}>", format_type(key), format_type(value)),
            ResolvedType::Optional(inner) => format!("{} | null", format_type(inner)),
            ResolvedType::Result { ok, err } => format!("Result<{}, {}>", format_type(ok), format_type(err)),
            ResolvedType::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(format_type).collect();
                format!("({})", parts.join(", "))
            }
            ResolvedType::Union(items) => {
                let parts: Vec<String> = items.iter().map(format_type).collect();
                parts.join(" | ")
            }
            ResolvedType::Unit => "void".into(),
        },
        TypeRef::Unresolved(name) => name.clone(),
        TypeRef::Unknown => "unknown".into(),
    }
}

/// Check if a function name looks like a test.
/// Extract import statements from source code.
fn extract_imports(source: &str, language: &str) -> Vec<String> {
    let mut imports = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        match language {
            "typescript" | "javascript" => {
                // import { X } from "Y"  or  import X from "Y"
                if trimmed.starts_with("import ") {
                    if let Some(from_pos) = trimmed.find("from ") {
                        let module = trimmed[from_pos + 5..]
                            .trim()
                            .trim_matches(|c| c == '"' || c == '\'' || c == ';' || c == ' ');
                        if !module.is_empty() {
                            imports.push(module.to_string());
                        }
                    }
                }
                // const X = require("Y")
                if trimmed.contains("require(") {
                    if let Some(start) = trimmed.find("require(\"").or_else(|| trimmed.find("require('")) {
                        let rest = &trimmed[start + 9..];
                        if let Some(end) = rest.find(|c| c == '"' || c == '\'') {
                            imports.push(rest[..end].to_string());
                        }
                    }
                }
            }
            "python" => {
                // from X import Y  or  import X  or  import X, Y
                if trimmed.starts_with("from ") {
                    let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
                    if parts.len() >= 2 {
                        imports.push(parts[1].to_string());
                    }
                } else if trimmed.starts_with("import ") {
                    // Handle: import json, base64 → ["json", "base64"]
                    let rest = &trimmed[7..];
                    for module in rest.split(',') {
                        let module = module.split(" as ").next().unwrap_or("").trim();
                        if !module.is_empty() {
                            imports.push(module.to_string());
                        }
                    }
                }
            }
            "rust" => {
                // use X::Y;  or  use X::Y::{A, B};
                if trimmed.starts_with("use ") {
                    let path = trimmed[4..].trim_end_matches(';').trim();
                    // Get the crate/module path (first segment).
                    if let Some(first) = path.split("::").next() {
                        let first = first.trim();
                        if !first.is_empty() && first != "self" && first != "super" {
                            imports.push(path.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    imports.sort();
    imports.dedup();
    imports
}

fn is_test_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("test")
        || lower.contains("::test_")
        || lower.contains("::test")
        || lower.starts_with("test_")
}

/// Detect project tooling by checking config files and package.json.
pub fn detect_tooling(root: &std::path::Path) -> Tooling {
    let exists = |name: &str| root.join(name).exists();

    // Read package.json devDependencies + dependencies for fallback detection.
    let pkg_deps = read_package_deps(root);

    // Type checker.
    let type_check = if exists("tsconfig.json") {
        if exists("node_modules/.bin/vue-tsc") || exists("node_modules/.bin/vue-tsc.cmd")
            || pkg_deps.contains("vue-tsc") {
            Some("npx vue-tsc --noEmit".into())
        } else {
            Some("npx tsc --noEmit".into())
        }
    } else if pkg_deps.contains("typescript") {
        Some("npx tsc --noEmit".into())
    } else {
        None
    };

    // Linter.
    let linter = if exists(".eslintrc.js") || exists(".eslintrc.json") || exists(".eslintrc.yml")
        || exists("eslint.config.js") || exists("eslint.config.mjs")
        || pkg_deps.contains("eslint") {
        Some("npx eslint".into())
    } else if exists(".flake8") || exists(".pylintrc") || pkg_deps.contains("pylint") || pkg_deps.contains("flake8") {
        Some("python -m pylint".into())
    } else if exists("pyproject.toml") && (pkg_deps.contains("ruff") || exists("ruff.toml")) {
        Some("ruff check".into())
    } else {
        None
    };

    // Formatter.
    let formatter = if exists(".prettierrc") || exists(".prettierrc.json") || exists(".prettierrc.js")
        || exists("prettier.config.js") || exists("prettier.config.mjs")
        || pkg_deps.contains("prettier") {
        Some("npx prettier --write".into())
    } else {
        None
    };

    // Test runner.
    let test_runner = if exists("vitest.config.ts") || exists("vitest.config.js")
        || pkg_deps.contains("vitest") {
        Some("npx vitest run".into())
    } else if exists("jest.config.js") || exists("jest.config.ts")
        || pkg_deps.contains("jest") {
        Some("npx jest".into())
    } else if exists("pytest.ini") || pkg_deps.contains("pytest") {
        Some("pytest".into())
    } else if exists("Cargo.toml") {
        Some("cargo test".into())
    } else {
        None
    };

    Tooling { type_check, linter, formatter, test_runner }
}

/// Read dependency names from package.json (both dependencies and devDependencies).
fn read_package_deps(root: &std::path::Path) -> std::collections::HashSet<String> {
    let mut deps = std::collections::HashSet::new();
    let pkg_path = root.join("package.json");

    if let Ok(content) = std::fs::read_to_string(&pkg_path) {
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
            for key in &["dependencies", "devDependencies"] {
                if let Some(obj) = pkg.get(key).and_then(|v| v.as_object()) {
                    for name in obj.keys() {
                        deps.insert(name.clone());
                    }
                }
            }
        }
    }

    deps
}
