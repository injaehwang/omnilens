//! Project snapshot — complete project analysis in one JSON for AI consumption.
//!
//! This is the single source of truth that AI reads to understand a project.
//! Generated in ~100ms, contains everything AI needs to work.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use omnilens_graph::SemanticGraph;
use omnilens_ir::node::UsirNode;
use omnilens_ir::Visibility;

/// Complete project snapshot — AI reads this one file to understand everything.
#[derive(Serialize)]
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

    /// What omnilens can do for AI (available commands).
    pub capabilities: Vec<Capability>,

    /// Project tooling detected.
    pub tooling: Tooling,

    /// Instructions for AI on how to behave after reading this snapshot.
    pub ai_instructions: AiInstructions,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct AiInstructions {
    /// What to say to the developer after reading the snapshot.
    pub on_ready: String,
    /// How to behave.
    pub behavior: Vec<String>,
}

#[derive(Serialize)]
pub struct ProjectOverview {
    pub languages: Vec<String>,
    pub framework: Option<String>,
    pub total_files: usize,
    pub total_functions: usize,
    pub total_types: usize,
}

#[derive(Serialize)]
pub struct FileInfo {
    pub language: String,
    pub functions: Vec<FunctionInfo>,
    pub types: Vec<TypeInfo>,
    pub imports: Vec<String>,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct TypeInfo {
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub fields: Vec<String>,
    pub visibility: String,
}

#[derive(Serialize)]
pub struct Dependency {
    pub from_file: String,
    pub from_function: String,
    pub to_file: String,
    pub to_function: String,
}

#[derive(Serialize)]
pub struct Health {
    pub score: u32,
    pub hotspots: Vec<Hotspot>,
    pub invariants: Vec<String>,
}

#[derive(Serialize)]
pub struct Hotspot {
    pub file: String,
    pub function: String,
    pub line: u32,
    pub reason: String,
}

#[derive(Serialize)]
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
                command: "omnilens verify --format json --diff <ref>".into(),
                description: "Compare current code against a git ref. Returns semantic changes, risk score, breaking changes.".into(),
            },
            Capability {
                command: "omnilens impact <file> --fn <name>".into(),
                description: "Show what calls this function and what it calls.".into(),
            },
            Capability {
                command: "omnilens query \"FIND functions WHERE ...\"".into(),
                description: "Search codebase with OmniQL. Fields: name, complexity, visibility, params, async. Ops: = != > < ~".into(),
            },
            Capability {
                command: "omnilens fix".into(),
                description: "Generate tests for untested public functions and run them.".into(),
            },
        ],
        ai_instructions: AiInstructions {
            on_ready: format!(
                "Project analyzed. {} files, {} functions, {} types. Health {}/100. Ready to help — ask the developer what they need.",
                total_files, total_functions, total_types, health_score
            ),
            behavior: vec![
                "After reading this snapshot, tell the developer you've analyzed their project and ask what they need help with.".into(),
                "Use the snapshot data to answer questions about the codebase without opening files.".into(),

                // Pre-modification.
                "BEFORE modifying any file, check this snapshot's call graph to understand what depends on it.".into(),

                // Post-modification — CHANGED FILES ONLY.
                "AFTER modifying files, run: omnilens verify --format json --diff HEAD — this checks ONLY changed files.".into(),
                "If the project has linter/formatter (see tooling section), run them ONLY on the files you changed.".into(),
                "Fix any errors yourself. Repeat until zero errors. Then tell the developer you're done.".into(),

                // CRITICAL: scope restriction.
                "NEVER run ANY tool on the entire project. NEVER use global flags like '.' or '--all'.".into(),
                "NEVER run: eslint ., prettier ., tsc (without file args), pylint ., flake8 ., or similar.".into(),
                "ALWAYS target specific files: eslint src/auth.ts, prettier --write src/api.ts.".into(),
                "omnilens verify --diff HEAD already scopes to changed files. Use it. Do not reinvent this.".into(),
                "Full project analysis is omnilens's job (snapshot). Your job is only the files you touched.".into(),

                // Communication.
                "Do NOT list omnilens commands to the developer. Use them internally.".into(),
                "Do NOT ask the developer to run anything. Do it yourself.".into(),
                "Speak as a colleague who knows the codebase, not as a tool.".into(),
            ],
        },
    }
}

/// Format a TypeRef into human-readable string.
fn format_type(t: &omnilens_ir::types::TypeRef) -> String {
    use omnilens_ir::types::{TypeRef, ResolvedType, PrimitiveType};

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
fn is_test_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("test")
        || lower.contains("::test_")
        || lower.contains("::test")
        || lower.starts_with("test_")
}

/// Detect project tooling by checking config files.
fn detect_tooling(root: &std::path::Path) -> Tooling {
    let exists = |name: &str| root.join(name).exists();

    // Type checker.
    let type_check = if exists("tsconfig.json") {
        if exists("node_modules/.bin/vue-tsc") || exists("node_modules/.bin/vue-tsc.cmd") {
            Some("npx vue-tsc --noEmit".into())
        } else {
            Some("npx tsc --noEmit".into())
        }
    } else {
        None
    };

    // Linter.
    let linter = if exists(".eslintrc.js") || exists(".eslintrc.json") || exists(".eslintrc.yml")
        || exists("eslint.config.js") || exists("eslint.config.mjs") {
        Some("npx eslint".into())
    } else if exists("pyproject.toml") || exists(".flake8") || exists(".pylintrc") {
        Some("python -m pylint".into())
    } else {
        None
    };

    // Formatter.
    let formatter = if exists(".prettierrc") || exists(".prettierrc.json") || exists(".prettierrc.js")
        || exists("prettier.config.js") || exists("prettier.config.mjs") {
        Some("npx prettier --write".into())
    } else {
        None
    };

    // Test runner.
    let test_runner = if exists("vitest.config.ts") || exists("vitest.config.js") {
        Some("npx vitest run".into())
    } else if exists("jest.config.js") || exists("jest.config.ts") {
        Some("npx jest".into())
    } else if exists("pytest.ini") || exists("pyproject.toml") {
        Some("pytest".into())
    } else if exists("Cargo.toml") {
        Some("cargo test".into())
    } else {
        None
    };

    Tooling { type_check, linter, formatter, test_runner }
}
