use anyhow::Result;
use colored::Colorize;

pub fn run() -> Result<()> {
    let mut engine = super::create_engine()?;

    // Index first.
    let idx = engine.index()?;
    if idx.files_analyzed > 0 {
        eprintln!(
            "{} {} files indexed",
            "Index".dimmed(),
            idx.files_analyzed
        );
    }

    println!("{}", "Discovering invariants...".cyan());
    let result = omnilens_core::invariants::discover(&engine.graph);

    println!(
        "\n{} {} invariants found ({} high confidence)\n",
        "Discovery".green().bold(),
        result.stats.invariants_found,
        result.stats.high_confidence,
    );

    for inv in &result.invariants {
        let confidence_pct = format!("{:.0}%", inv.confidence * 100.0);
        let confidence_colored = if inv.confidence >= 0.9 {
            confidence_pct.green()
        } else if inv.confidence >= 0.7 {
            confidence_pct.yellow()
        } else {
            confidence_pct.dimmed()
        };

        let kind_tag = match &inv.kind {
            omnilens_ir::invariant::InvariantKind::ErrorsMustBeHandled { .. } => {
                "ERROR-HANDLING".red()
            }
            omnilens_ir::invariant::InvariantKind::ConventionConstraint { .. } => {
                "CONVENTION".blue()
            }
            omnilens_ir::invariant::InvariantKind::MustPrecede { .. } => "ORDERING".yellow(),
            omnilens_ir::invariant::InvariantKind::TypeUsageConstraint { .. } => {
                "TYPE-USAGE".cyan()
            }
            _ => "OTHER".dimmed(),
        };

        println!(
            "  {} [{}] {} (evidence: {}, confidence: {})",
            "INV".bold(),
            kind_tag,
            inv.description,
            inv.evidence_count,
            confidence_colored,
        );
    }

    if result.invariants.is_empty() {
        println!("  {}", "No invariants discovered yet. Index more code.".dimmed());
    }

    Ok(())
}
