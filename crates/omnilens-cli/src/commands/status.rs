//! `omnilens status` — project health dashboard.

use anyhow::Result;
use colored::Colorize;
use omnilens_ir::node::UsirNode;
use omnilens_ir::Visibility;

pub fn run() -> Result<()> {
    let mut engine = super::create_engine()?;
    let idx = engine.index()?;

    let graph = &engine.graph;
    let all_ids = graph.all_node_ids();

    // Collect stats.
    let mut total_functions = 0u32;
    let mut public_functions = 0u32;
    let mut async_functions = 0u32;
    let mut total_types = 0u32;
    let mut total_modules = 0u32;
    let mut high_complexity: Vec<(String, String, u32, u32)> = Vec::new(); // (name, file, line, complexity)
    let mut many_params: Vec<(String, String, u32, usize)> = Vec::new();
    let mut files: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut lang_counts: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();

    for id in &all_ids {
        let Some(node) = graph.get_node(*id) else { continue };
        if graph.is_placeholder(*id) { continue; }

        let file = node.span().file.to_string_lossy().replace('\\', "/");
        files.insert(file.clone());

        // Count by language.
        let ext = file.rsplit('.').next().unwrap_or("");
        let lang = match ext {
            "rs" => "Rust",
            "ts" | "tsx" | "js" | "jsx" => "TypeScript",
            "py" | "pyi" => "Python",
            _ => "Other",
        };
        *lang_counts.entry(lang).or_default() += 1;

        match node {
            UsirNode::Function(f) => {
                if f.complexity.is_none() { continue; } // placeholder
                total_functions += 1;
                if f.visibility == Visibility::Public { public_functions += 1; }
                if f.is_async { async_functions += 1; }

                let c = f.complexity.unwrap_or(0);
                if c > 15 {
                    let short = f.name.segments.last().cloned().unwrap_or_default();
                    let file_short = file.rsplit('/').next().unwrap_or(&file).to_string();
                    high_complexity.push((short, file_short, f.span.start_line, c));
                }

                if f.params.len() > 4 {
                    let short = f.name.segments.last().cloned().unwrap_or_default();
                    let file_short = file.rsplit('/').next().unwrap_or(&file).to_string();
                    many_params.push((short, file_short, f.span.start_line, f.params.len()));
                }
            }
            UsirNode::DataType(_) => { total_types += 1; }
            UsirNode::Module(_) => { total_modules += 1; }
            _ => {}
        }
    }

    high_complexity.sort_by(|a, b| b.3.cmp(&a.3));
    many_params.sort_by(|a, b| b.3.cmp(&a.3));

    // Invariants.
    let invs = omnilens_core::invariants::discover(graph);

    // Health score (0-100).
    let complexity_penalty = (high_complexity.len() as f64 * 3.0).min(30.0);
    let param_penalty = (many_params.len() as f64 * 2.0).min(15.0);
    let health = (100.0 - complexity_penalty - param_penalty).max(0.0) as u32;

    // ─── Output ─────────────────────────────────────────────

    println!();
    println!("  {}", "omnilens project status".bold());
    println!("  {}", "─".repeat(45));

    // Health score bar.
    let bar_len = 20;
    let filled = (health as usize * bar_len) / 100;
    let bar_color = if health >= 80 { "green" } else if health >= 50 { "yellow" } else { "red" };
    let bar: String = "█".repeat(filled) + &"░".repeat(bar_len - filled);
    let bar_display = match bar_color {
        "green" => bar.green(),
        "yellow" => bar.yellow(),
        _ => bar.red(),
    };
    println!("  Health  {} {}/100", bar_display, health);
    println!();

    // Overview.
    println!("  {} {} files, {} functions, {} types",
        "Overview".bold(),
        files.len(),
        total_functions,
        total_types,
    );

    // Language breakdown.
    let mut langs: Vec<_> = lang_counts.iter().collect();
    langs.sort_by(|a, b| b.1.cmp(a.1));
    let lang_str: Vec<String> = langs.iter().map(|(l, c)| format!("{} {}", l, c)).collect();
    println!("           {}", lang_str.join(" · ").dimmed());
    println!();

    // Hotspots.
    if !high_complexity.is_empty() {
        println!("  {} (complexity > 15)", "Hotspots".red().bold());
        for (name, file, line, c) in high_complexity.iter().take(5) {
            println!(
                "    {} {}:{} — {} (complexity {})",
                "●".red(),
                file,
                line,
                name,
                c.to_string().red()
            );
        }
        if high_complexity.len() > 5 {
            println!(
                "    {} ... and {} more",
                "●".dimmed(),
                high_complexity.len() - 5
            );
        }
        println!();
    }

    if !many_params.is_empty() {
        println!("  {} (params > 4)", "Wide functions".yellow().bold());
        for (name, file, line, p) in many_params.iter().take(5) {
            println!(
                "    {} {}:{} — {} ({} params)",
                "●".yellow(),
                file,
                line,
                name,
                p
            );
        }
        println!();
    }

    // Invariants summary.
    let high_conf = invs.invariants.iter().filter(|i| i.confidence >= 0.9).count();
    if invs.stats.invariants_found > 0 {
        println!(
            "  {} {} discovered ({} high confidence)",
            "Rules".cyan().bold(),
            invs.stats.invariants_found,
            high_conf,
        );
        for inv in invs.invariants.iter().filter(|i| i.confidence >= 0.9).take(3) {
            println!("    {} {}", "·".cyan(), inv.description.dimmed());
        }
        println!();
    }

    println!("  {}", "─".repeat(45));
    println!(
        "  Run {} to scan for problems",
        "omnilens check".cyan()
    );
    println!();

    Ok(())
}
