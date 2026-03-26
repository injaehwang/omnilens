use std::path::PathBuf;

use anyhow::Result;
use colored::Colorize;

pub fn run(target: &str, fn_name: Option<&str>, depth: usize) -> Result<()> {
    let mut engine = super::create_engine()?;

    // Index first to populate the graph.
    let idx = engine.index()?;
    if idx.files_analyzed > 0 {
        eprintln!(
            "{} {} files indexed",
            "Index".dimmed(),
            idx.files_analyzed
        );
    }

    let path = PathBuf::from(target);

    let (target_id, target_name) = engine.find_target(&path, fn_name)?;

    // Forward: what does this function affect?
    let forward = engine.graph.impact_forward(target_id, depth);
    // Reverse: what calls this function?
    let reverse = engine.graph.impact_reverse(target_id, depth);

    // Header
    println!(
        "\n{} {}",
        "Impact Analysis:".bold().cyan(),
        target_name.bold()
    );
    println!("{}", "═".repeat(60));

    // Reverse (who calls this?)
    println!("\n  {} ({} total)", "Who calls this?".bold().yellow(), reverse.total_affected);
    for node in reverse.direct.iter().chain(reverse.transitive.iter()).take(15) {
        if let Some(n) = engine.graph.get_node(node.node_id) {
            let name = n.name().display();
            let span = n.span();
            let depth_indicator = "→".repeat(node.distance.min(5));
            println!(
                "    {} {} ({}:{})",
                depth_indicator.green(),
                name,
                span.file.file_name().unwrap_or_default().to_string_lossy(),
                span.start_line
            );
        }
    }

    // Forward (what does this call?)
    println!("\n  {} ({} total)", "What does it call?".bold().yellow(), forward.total_affected);
    for node in forward.direct.iter().chain(forward.transitive.iter()).take(15) {
        if let Some(n) = engine.graph.get_node(node.node_id) {
            let name = n.name().display();
            let span = n.span();
            let depth_indicator = "→".repeat(node.distance.min(5));
            println!(
                "    {} {} ({}:{})",
                depth_indicator.cyan(),
                name,
                span.file.file_name().unwrap_or_default().to_string_lossy(),
                span.start_line
            );
        }
    }

    // Summary
    let total = forward.total_affected + reverse.total_affected;
    let risk = (forward.risk_score + reverse.risk_score) / 2.0;
    println!("\n{}", "═".repeat(60));
    println!(
        "  {} {} nodes | {} {} ",
        "Total blast radius:".bold(),
        total,
        "Risk:".bold(),
        format!("{:.0}%", risk * 100.0).red()
    );

    Ok(())
}
