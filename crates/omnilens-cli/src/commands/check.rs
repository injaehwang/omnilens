//! `omnilens check` — the ONE command developers need.
//!
//! Scans code changes, detects problems, and tells you exactly what to do.
//! No query language, no flags, no configuration needed.

use anyhow::Result;
use colored::Colorize;
use omnilens_core::verify::{ChangeRisk, DiffSpec, SemanticChangeKind};
use omnilens_ir::node::UsirNode;

use crate::OutputFormat;

pub fn run(files: Vec<String>, format: &OutputFormat) -> Result<()> {
    let mut engine = super::create_engine()?;
    let idx = engine.index()?;

    // Auto-detect what to check.
    let diff_spec = if !files.is_empty() {
        DiffSpec::Files(files.into_iter().map(Into::into).collect())
    } else {
        // Smart detection: staged > working dir > last commit.
        detect_best_diff()
    };

    let result = engine.verify(&diff_spec)?;

    // JSON output for CI/AI agents.
    if matches!(format, OutputFormat::Json) {
        println!("{}", omnilens_core::output::to_json(&result));
        if result.has_errors() {
            std::process::exit(1);
        }
        return Ok(());
    }
    if matches!(format, OutputFormat::Sarif) {
        println!("{}", omnilens_core::output::to_sarif(&result));
        if result.has_errors() {
            std::process::exit(1);
        }
        return Ok(());
    }

    // ─── Human-friendly output ──────────────────────────────

    if result.semantic_changes.is_empty()
        && result.invariant_violations.is_empty()
        && result.suggested_tests.is_empty()
    {
        println!("\n  {} No issues found.\n", "✓".green().bold());
        return Ok(());
    }

    println!();

    // Group problems by file.
    let mut file_issues: std::collections::BTreeMap<String, Vec<Issue>> =
        std::collections::BTreeMap::new();

    // Semantic changes → issues.
    for change in &result.semantic_changes {
        let file = change
            .location
            .file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let full_path = change.location.file.to_string_lossy().replace('\\', "/");

        let (icon, severity) = match change.risk {
            ChangeRisk::Breaking => ("✗".red().bold(), Severity::Error),
            ChangeRisk::SecuritySensitive => ("!".red().bold(), Severity::Error),
            ChangeRisk::NeedsReview => ("●".yellow(), Severity::Warning),
            ChangeRisk::Safe => ("○".dimmed(), Severity::Info),
        };

        file_issues.entry(file.clone()).or_default().push(Issue {
            line: change.location.start_line,
            icon: icon.to_string(),
            severity,
            message: simplify_description(&change.kind, &change.description),
            action: suggest_action(&change.kind, &change.risk),
        });
    }

    // Missing test suggestions → issues.
    for test in &result.suggested_tests {
        if let Some(node) = engine.graph.get_node(test.target) {
            let file = node
                .span()
                .file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            file_issues.entry(file).or_default().push(Issue {
                line: node.span().start_line,
                icon: "△".yellow().to_string(),
                severity: Severity::Warning,
                message: format!("No tests for `{}`", short_name(node)),
                action: Some("Write tests or run `omnilens testgen`".into()),
            });
        }
    }

    // Print grouped by file.
    let mut error_count = 0;
    let mut warning_count = 0;

    for (file, issues) in &file_issues {
        println!("  {}", file.bold());
        for issue in issues {
            println!(
                "    {} L{:<4} {}",
                issue.icon, issue.line, issue.message
            );
            if let Some(ref action) = issue.action {
                println!("    {}  → {}", " ".repeat(6), action.dimmed());
            }
            match issue.severity {
                Severity::Error => error_count += 1,
                Severity::Warning => warning_count += 1,
                _ => {}
            }
        }
        println!();
    }

    // Summary line.
    let risk_pct = (result.risk_score * 100.0) as u32;
    let risk_display = if risk_pct >= 50 {
        format!("{}%", risk_pct).red().bold()
    } else if risk_pct >= 20 {
        format!("{}%", risk_pct).yellow()
    } else {
        format!("{}%", risk_pct).green()
    };

    if error_count > 0 {
        println!(
            "  {} {} errors, {} warnings — risk {}",
            "FAIL".red().bold(),
            error_count,
            warning_count,
            risk_display
        );
        println!();
        std::process::exit(1);
    } else if warning_count > 0 {
        println!(
            "  {} {} warnings — risk {}",
            "OK".yellow().bold(),
            warning_count,
            risk_display
        );
    } else {
        println!("  {} risk {}", "OK".green().bold(), risk_display);
    }
    println!();

    Ok(())
}

/// Auto-detect the best diff spec.
fn detect_best_diff() -> DiffSpec {
    // 1. Staged changes?
    if has_staged_changes() {
        return DiffSpec::Staged;
    }
    // 2. Working directory changes?
    if has_working_changes() {
        return DiffSpec::WorkingDir;
    }
    // 3. Fall back to last commit.
    DiffSpec::GitDiff {
        base: "HEAD~1".into(),
        head: "HEAD".into(),
    }
}

fn has_staged_changes() -> bool {
    std::process::Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false)
}

fn has_working_changes() -> bool {
    std::process::Command::new("git")
        .args(["diff", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false)
}

/// Make descriptions human-friendly.
fn simplify_description(kind: &SemanticChangeKind, original: &str) -> String {
    match kind {
        SemanticChangeKind::FunctionAdded => {
            let name = original.strip_prefix("New function '").unwrap_or(original);
            let name = name.strip_suffix("'").unwrap_or(name);
            format!("New function {}", name.cyan())
        }
        SemanticChangeKind::FunctionRemoved => {
            let name = original.strip_prefix("Removed function '").unwrap_or(original);
            let name = name.strip_suffix("'").unwrap_or(name);
            format!("Removed {}", name.red())
        }
        SemanticChangeKind::SignatureChange => {
            format!("{}", original.yellow())
        }
        SemanticChangeKind::ComplexityChange { old, new } => {
            if *new > *old {
                format!("Complexity increased {} → {}", old, new.to_string().red())
            } else {
                format!("Complexity decreased {} → {}", old, new.to_string().green())
            }
        }
        SemanticChangeKind::ApiSurfaceChange => {
            format!("{}", original.yellow())
        }
        SemanticChangeKind::ControlFlowChange => {
            format!("{}", original.red())
        }
        _ => original.to_string(),
    }
}

/// Suggest what the developer should DO.
fn suggest_action(kind: &SemanticChangeKind, risk: &ChangeRisk) -> Option<String> {
    match (kind, risk) {
        (SemanticChangeKind::SignatureChange, ChangeRisk::Breaking) => {
            Some("Update all callers or add a backward-compatible wrapper".into())
        }
        (SemanticChangeKind::FunctionRemoved, ChangeRisk::Breaking) => {
            Some("Verify no external consumers depend on this".into())
        }
        (SemanticChangeKind::FunctionAdded, ChangeRisk::NeedsReview) => {
            Some("Add tests for the new function".into())
        }
        (SemanticChangeKind::ComplexityChange { new, .. }, _) if *new > 20 => {
            Some("Consider splitting into smaller functions".into())
        }
        (SemanticChangeKind::ControlFlowChange, ChangeRisk::SecuritySensitive) => {
            Some("Requires security review".into())
        }
        _ => None,
    }
}

fn short_name(node: &UsirNode) -> String {
    node.name().segments.last().cloned().unwrap_or_default()
}

struct Issue {
    line: u32,
    icon: String,
    severity: Severity,
    message: String,
    action: Option<String>,
}

enum Severity {
    Error,
    Warning,
    Info,
}
