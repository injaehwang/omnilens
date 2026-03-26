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

    let mut files: BTreeMap<String, FileInfo> = BTreeMap::new();
    let mut languages = std::collections::HashSet::new();
    let mut total_functions = 0usize;
    let mut total_types = 0usize;
    let mut hotspots = Vec::new();
    let mut dependencies = Vec::new();

    for id in &all_ids {
        let Some(node) = graph.get_node(*id) else { continue };
        if graph.is_placeholder(*id) { continue; }

        let file_path = node.span().file.to_string_lossy().replace('\\', "/");
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

                // Get calls (forward impact depth=1).
                let forward = graph.impact_forward(f.id, 1);
                let calls: Vec<String> = forward.direct.iter()
                    .filter_map(|n| graph.get_node(n.node_id))
                    .filter(|n| !graph.is_placeholder(n.id()))
                    .map(|n| n.name().display())
                    .collect();

                // Get callers (reverse impact depth=1).
                let reverse = graph.impact_reverse(f.id, 1);
                let called_by: Vec<String> = reverse.direct.iter()
                    .filter_map(|n| graph.get_node(n.node_id))
                    .filter(|n| !graph.is_placeholder(n.id()))
                    .map(|n| n.name().display())
                    .collect();

                // Cross-file dependencies.
                for caller in &reverse.direct {
                    if let Some(caller_node) = graph.get_node(caller.node_id) {
                        let caller_file = caller_node.span().file.to_string_lossy().replace('\\', "/");
                        if caller_file != file_path {
                            dependencies.push(Dependency {
                                from_file: caller_file,
                                from_function: caller_node.name().display(),
                                to_file: file_path.clone(),
                                to_function: f.name.display(),
                            });
                        }
                    }
                }

                // Hotspots.
                if complexity > 15 {
                    hotspots.push(Hotspot {
                        file: file_path.clone(),
                        function: f.name.display(),
                        line: f.span.start_line,
                        reason: format!("complexity {}", complexity),
                    });
                }
                if f.visibility == Visibility::Public && reverse.total_affected > 5 {
                    hotspots.push(Hotspot {
                        file: file_path.clone(),
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
                    return_type: f.return_type.as_ref().map(|t| format!("{:?}", t)),
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
    }
}
