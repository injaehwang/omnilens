use anyhow::Result;
use colored::Colorize;

pub fn run() -> Result<()> {
    let mut engine = super::create_engine()?;

    println!("{}", "Building semantic index...".cyan());
    let result = engine.index()?;

    println!(
        "{} {} files | {} nodes | {} edges | {:.2?}",
        "Indexed".green().bold(),
        result.files_analyzed,
        result.nodes_added,
        result.edges_added,
        result.duration
    );

    println!(
        "  {} {} resolved, {} unresolved",
        "Links:".bold(),
        result.links_resolved,
        result.links_unresolved
    );

    println!(
        "  {} {} nodes, {} edges",
        "Graph:".bold(),
        engine.graph.node_count(),
        engine.graph.edge_count()
    );

    Ok(())
}
