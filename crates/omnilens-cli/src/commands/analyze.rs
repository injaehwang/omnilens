//! `omnilens` (no args) — the main experience.
//!
//! Analyzes the entire project and outputs a snapshot for AI.
//! This is the ONLY command developers need to know.

use std::time::Instant;

use anyhow::Result;
use colored::Colorize;

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;

    // Auto-init.
    let omnilens_dir = cwd.join(".omnilens");
    std::fs::create_dir_all(&omnilens_dir)?;

    // Index.
    let start = Instant::now();
    let mut engine = super::create_engine()?;
    let idx = engine.index()?;
    let duration = start.elapsed();

    // Generate snapshot.
    let snapshot = omnilens_core::snapshot::generate(
        &engine.graph,
        duration.as_millis() as u64,
    );

    // Write snapshot.
    let snapshot_json = serde_json::to_string_pretty(&snapshot)?;
    std::fs::write(omnilens_dir.join("snapshot.json"), &snapshot_json)?;

    // Write AI context files — covers all major AI tools.
    write_ai_context(&cwd)?;

    // Output for human + AI.
    println!();
    println!(
        "  {} {}ms | {} files | {} functions | {} types",
        "omnilens".green().bold(),
        duration.as_millis(),
        snapshot.project.total_files,
        snapshot.project.total_functions,
        snapshot.project.total_types,
    );

    // Health.
    let health = &snapshot.health;
    let score_display = if health.score >= 80 {
        format!("{}/100", health.score).green()
    } else if health.score >= 50 {
        format!("{}/100", health.score).yellow()
    } else {
        format!("{}/100", health.score).red()
    };
    println!("  Health: {}", score_display);

    // Hotspots.
    if !health.hotspots.is_empty() {
        let count = health.hotspots.len();
        println!("  Hotspots: {}", format!("{} found", count).yellow());
    }

    // Cross-file deps.
    if !snapshot.dependencies.is_empty() {
        println!("  Cross-file deps: {}", snapshot.dependencies.len());
    }

    println!();
    println!("  Tell your AI: {}", "\"Review the omnilens snapshot\"".cyan());
    println!();

    Ok(())
}

/// One-line pointer that goes into AI config files at project root.
const AI_POINTER: &str = "Read `.omnilens/ai-instructions.md` for project analysis and coding rules.";

fn write_ai_context(cwd: &std::path::Path) -> Result<()> {
    let omnilens_dir = cwd.join(".omnilens");

    // Full instructions go inside .omnilens/ — not in project root.
    std::fs::write(omnilens_dir.join("ai-instructions.md"), r#"# omnilens

This project uses omnilens for code analysis. Read `.omnilens/snapshot.json` for the full project map.

Follow the `ai_instructions` in snapshot.json.

## MANDATORY: Verify after every modification

After modifying ANY file, you MUST run:
```
omnilens verify --format json --diff HEAD
```

If the result contains breaking changes or errors:
1. Fix them yourself immediately.
2. Run verify again.
3. Repeat until zero errors.
4. Only then tell the developer the work is done.

Do NOT skip this. Do NOT ask the developer to verify. Do it yourself.

## MANDATORY: No extra files

- Do NOT create helper scripts, temporary files, migration scripts, or new folders for your work.
- Modify existing source files directly.
- If you need to store anything, put it in `.omnilens/` only.
- Do NOT create `scripts/`, `tools/`, `migrations/`, or similar directories.

## Available commands (use internally, do NOT show to developer)
- `omnilens verify --format json --diff HEAD` — MUST run after every file modification
- `omnilens impact <file> --fn <name>` — check before modifying a function
- `omnilens query "FIND functions WHERE ..."` — search codebase
"#)?;

    // Minimal pointers in AI config files. Append only if not already present.
    append_if_missing(&cwd.join("CLAUDE.md"), AI_POINTER)?;
    append_if_missing(&cwd.join(".cursorrules"), AI_POINTER)?;
    append_if_missing(&cwd.join(".windsurfrules"), AI_POINTER)?;

    let copilot_dir = cwd.join(".github");
    std::fs::create_dir_all(&copilot_dir).ok();
    append_if_missing(&copilot_dir.join("copilot-instructions.md"), AI_POINTER)?;

    Ok(())
}

fn append_if_missing(path: &std::path::Path, line: &str) -> Result<()> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        if !content.contains("omnilens") {
            std::fs::write(path, format!("{}\n{}\n", content.trim_end(), line))?;
        }
    } else {
        std::fs::write(path, format!("{}\n", line))?;
    }
    Ok(())
}
