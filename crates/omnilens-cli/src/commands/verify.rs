use anyhow::Result;
use colored::Colorize;
use omnilens_core::verify::DiffSpec;

pub fn run(diff: Option<String>, files: Vec<String>) -> Result<()> {
    let mut engine = super::create_engine()?;

    // Index first.
    let idx = engine.index()?;
    if idx.files_analyzed > 0 {
        eprintln!(
            "{} {} files indexed, {} links resolved",
            "Index".dimmed(),
            idx.files_analyzed,
            idx.links_resolved,
        );
    }

    let diff_spec = if let Some(ref diff_ref) = diff {
        DiffSpec::GitDiff {
            base: diff_ref.clone(),
            head: "HEAD".to_string(),
        }
    } else if !files.is_empty() {
        DiffSpec::Files(files.into_iter().map(Into::into).collect())
    } else {
        DiffSpec::WorkingDir
    };

    println!("{}", "Verifying changes...".cyan());
    let result = engine.verify(&diff_spec)?;

    // Show semantic changes.
    if !result.semantic_changes.is_empty() {
        println!("\n{} ({} total)", "Semantic Changes".bold().yellow(), result.semantic_changes.len());
        for change in &result.semantic_changes {
            let risk_badge = match change.risk {
                omnilens_core::verify::ChangeRisk::Safe => "SAFE".green(),
                omnilens_core::verify::ChangeRisk::NeedsReview => "REVIEW".yellow(),
                omnilens_core::verify::ChangeRisk::Breaking => "BREAKING".red(),
                omnilens_core::verify::ChangeRisk::SecuritySensitive => "SECURITY".red().bold(),
            };

            println!(
                "  [{}] {}:{} — {}",
                risk_badge,
                change.location.file.file_name().unwrap_or_default().to_string_lossy(),
                change.location.start_line,
                change.description,
            );
        }
    }

    // Show invariant violations.
    if !result.invariant_violations.is_empty() {
        println!(
            "\n{} ({} total)",
            "Invariant Warnings".bold().yellow(),
            result.invariant_violations.len()
        );
        for v in &result.invariant_violations {
            let severity_badge = match v.severity {
                omnilens_ir::invariant::ViolationSeverity::Error => "ERR".red().bold(),
                omnilens_ir::invariant::ViolationSeverity::Warning => "WARN".yellow(),
                omnilens_ir::invariant::ViolationSeverity::Info => "INFO".dimmed(),
            };
            println!(
                "  [{}] {}:{} — {}",
                severity_badge,
                v.location.file.file_name().unwrap_or_default().to_string_lossy(),
                v.location.start_line,
                v.description,
            );
        }
    }

    // Show test suggestions.
    if !result.suggested_tests.is_empty() {
        println!(
            "\n{} ({} total)",
            "Suggested Tests".bold().cyan(),
            result.suggested_tests.len()
        );
        for t in result.suggested_tests.iter().take(5) {
            println!("  {} {}", "→".cyan(), t.description);
        }
        if result.suggested_tests.len() > 5 {
            println!("  {} ... and {} more", "→".dimmed(), result.suggested_tests.len() - 5);
        }
    }

    // Summary.
    println!();
    if result.has_errors() {
        println!(
            "{} {} errors, {} warnings | Risk: {:.0}%",
            "FAIL".red().bold(),
            result.error_count(),
            result.warning_count(),
            result.risk_score * 100.0,
        );
        std::process::exit(1);
    } else {
        println!(
            "{} {} semantic changes, {} warnings | Risk: {:.0}%",
            "PASS".green().bold(),
            result.semantic_changes.len(),
            result.warning_count(),
            result.risk_score * 100.0,
        );
    }

    Ok(())
}
