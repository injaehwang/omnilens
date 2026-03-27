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
    pub ai_instructions: crate::snapshot::AiInstructions,
    pub capabilities: Vec<crate::snapshot::Capability>,
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

    // Signature changes (same name, different params).
    let mut sig_changes = Vec::new();
    for (key, curr_f) in &curr_funcs {
        if let Some(prev_f) = prev_funcs.get(key) {
            // Compare by checking if the function info differs.
            // Simple heuristic: if line changed significantly, it was modified.
            if (curr_f.line as i32 - prev_f.line as i32).abs() > 2 {
                sig_changes.push(SignatureChange {
                    file: curr_f.file.clone(),
                    name: curr_f.name.clone(),
                    line: curr_f.line,
                    description: format!("moved from line {} to {}", prev_f.line, curr_f.line),
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
            map.insert(key, FunctionChange {
                file: file.clone(),
                name: func.name.clone(),
                line: func.line,
            });
        }
    }
    map
}
