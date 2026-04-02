//! Summary + Changes — lightweight files for AI consumption.
//!
//! summary.json: project overview (~2KB, always generated)
//! changes.json: what changed since last snapshot (~few hundred bytes, only when changes exist)

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::snapshot::Snapshot;

/// Lightweight project summary — AI reads this instead of full snapshot.
#[derive(Serialize)]
pub struct Summary {
    pub analysis_ms: u64,
    pub files: usize,
    pub functions: usize,
    pub types: usize,
    pub health: u32,
    pub languages: Vec<String>,
    pub tooling: crate::snapshot::Tooling,
    pub hotspots: Vec<String>,
    pub top_dependencies: Vec<String>,
    /// File-level overview: functions, types, imports per file.
    pub file_map: BTreeMap<String, FileSummary>,
    pub ai_instructions: crate::snapshot::AiInstructions,
    pub capabilities: Vec<crate::snapshot::Capability>,
}

#[derive(Serialize)]
pub struct FileSummary {
    pub functions: Vec<String>,
    pub types: Vec<String>,
    pub imports: Vec<String>,
}

/// What changed between two snapshots.
#[derive(Serialize)]
pub struct Changes {
    pub modified_files: Vec<String>,
    pub added_functions: Vec<FunctionChange>,
    pub removed_functions: Vec<FunctionChange>,
    pub signature_changes: Vec<SignatureChange>,
    pub new_dependencies: Vec<String>,
    pub lost_dependencies: Vec<String>,
    pub health_delta: i32,
    pub syntax_errors: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct FunctionChange {
    pub file: String,
    pub name: String,
    pub line: u32,
    pub signature: String,
}

#[derive(Serialize)]
pub struct SignatureChange {
    pub file: String,
    pub name: String,
    pub line: u32,
    pub description: String,
}

/// Generate summary from snapshot.
pub fn generate_summary(snapshot: &Snapshot) -> Summary {
    let hotspots: Vec<String> = snapshot.health.hotspots.iter()
        .take(10)
        .map(|h| format!("{}:{} {} ({})", h.file, h.line, h.function, h.reason))
        .collect();

    let top_deps: Vec<String> = snapshot.dependencies.iter()
        .take(20)
        .map(|d| format!("{} → {}", d.from_function, d.to_function))
        .collect();

    // Build file map: compact function signatures, types, imports per file.
    let mut file_map: BTreeMap<String, FileSummary> = BTreeMap::new();
    for (path, info) in &snapshot.files {
        let functions: Vec<String> = info.functions.iter().map(|f| {
            let params = f.params.join(", ");
            let ret = f.return_type.as_deref().unwrap_or("void");
            let prefix = if f.is_async { "async " } else { "" };
            format!("{}{}({}) → {}", prefix, f.name, params, ret)
        }).collect();

        let types: Vec<String> = info.types.iter().map(|t| {
            if t.fields.is_empty() {
                t.name.clone()
            } else {
                format!("{} {{ {} }}", t.name, t.fields.join(", "))
            }
        }).collect();

        let imports = info.imports.clone();

        file_map.insert(path.clone(), FileSummary { functions, types, imports });
    }

    Summary {
        analysis_ms: snapshot.analysis_ms,
        files: snapshot.project.total_files,
        functions: snapshot.project.total_functions,
        types: snapshot.project.total_types,
        health: snapshot.health.score,
        languages: snapshot.project.languages.clone(),
        tooling: crate::snapshot::Tooling {
            type_check: snapshot.tooling.type_check.clone(),
            linter: snapshot.tooling.linter.clone(),
            formatter: snapshot.tooling.formatter.clone(),
            test_runner: snapshot.tooling.test_runner.clone(),
        },
        hotspots,
        top_dependencies: top_deps,
        file_map,
        ai_instructions: crate::snapshot::AiInstructions {
            on_ready: snapshot.ai_instructions.on_ready.clone(),
            behavior: snapshot.ai_instructions.behavior.clone(),
        },
        capabilities: snapshot.capabilities.iter().map(|c| crate::snapshot::Capability {
            command: c.command.clone(),
            description: c.description.clone(),
        }).collect(),
    }
}

/// Generate changes by comparing current snapshot with previous.
pub fn generate_changes(current: &Snapshot, previous_path: &Path) -> Option<Changes> {
    let prev_content = std::fs::read_to_string(previous_path).ok()?;
    let prev: Snapshot = serde_json::from_str(&prev_content).ok()?;

    let prev_funcs = collect_functions(&prev);
    let curr_funcs = collect_functions(current);

    // Added functions.
    let added: Vec<FunctionChange> = curr_funcs.iter()
        .filter(|(key, _)| !prev_funcs.contains_key(*key))
        .map(|(_, f)| f.clone())
        .collect();

    // Removed functions.
    let removed: Vec<FunctionChange> = prev_funcs.iter()
        .filter(|(key, _)| !curr_funcs.contains_key(*key))
        .map(|(_, f)| f.clone())
        .collect();

    // Signature changes — compare actual signatures.
    let mut sig_changes = Vec::new();
    for (key, curr_f) in &curr_funcs {
        if let Some(prev_f) = prev_funcs.get(key) {
            if curr_f.signature != prev_f.signature {
                sig_changes.push(SignatureChange {
                    file: curr_f.file.clone(),
                    name: curr_f.name.clone(),
                    line: curr_f.line,
                    description: format!("{} → {}", prev_f.signature, curr_f.signature),
                });
            }
        }
    }

    // Modified files: files present in both but with different function counts.
    let mut modified_files = Vec::new();
    for (file, curr_info) in &current.files {
        if let Some(prev_info) = prev.files.get(file) {
            if curr_info.functions.len() != prev_info.functions.len()
                || curr_info.types.len() != prev_info.types.len()
            {
                modified_files.push(file.clone());
            }
        } else {
            modified_files.push(file.clone()); // New file.
        }
    }

    // New/lost dependencies.
    let prev_deps: std::collections::HashSet<String> = prev.dependencies.iter()
        .map(|d| format!("{}→{}", d.from_function, d.to_function))
        .collect();
    let curr_deps: std::collections::HashSet<String> = current.dependencies.iter()
        .map(|d| format!("{}→{}", d.from_function, d.to_function))
        .collect();

    let new_deps: Vec<String> = curr_deps.difference(&prev_deps).cloned().collect();
    let lost_deps: Vec<String> = prev_deps.difference(&curr_deps).cloned().collect();

    // Health delta.
    let health_delta = current.health.score as i32 - prev.health.score as i32;

    // Only return if there are actual changes.
    if added.is_empty() && removed.is_empty() && sig_changes.is_empty()
        && modified_files.is_empty() && new_deps.is_empty() && lost_deps.is_empty()
        && health_delta == 0
    {
        return None;
    }

    Some(Changes {
        modified_files,
        added_functions: added,
        removed_functions: removed,
        signature_changes: sig_changes,
        new_dependencies: new_deps,
        lost_dependencies: lost_deps,
        health_delta,
        syntax_errors: Vec::new(),
    })
}

fn collect_functions(snapshot: &Snapshot) -> BTreeMap<String, FunctionChange> {
    let mut map = BTreeMap::new();
    for (file, info) in &snapshot.files {
        for func in &info.functions {
            let key = format!("{}::{}", file, func.name);
            let sig = format!(
                "{}({}) → {}",
                func.name,
                func.params.join(", "),
                func.return_type.as_deref().unwrap_or("void")
            );
            map.insert(key, FunctionChange {
                file: file.clone(),
                name: func.name.clone(),
                line: func.line,
                signature: sig,
            });
        }
    }
    map
}
