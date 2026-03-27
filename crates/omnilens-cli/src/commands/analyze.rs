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
    println!("  Tell your AI: {}", "\"let's start omnilens\"".cyan());
    println!();

    Ok(())
}

/// One-line pointer that goes into AI config files at project root.
const AI_POINTER: &str = "When the developer says \"omnilens\", \"let's start omnilens\", \"start omnilens\", or anything mentioning omnilens, read `.omnilens/ai-instructions.md` and `.omnilens/snapshot.json`, then follow the instructions.";

fn write_ai_context(cwd: &std::path::Path) -> Result<()> {
    let omnilens_dir = cwd.join(".omnilens");

    // Full instructions go inside .omnilens/ — not in project root.
    std::fs::write(omnilens_dir.join("ai-instructions.md"), r#"# omnilens — AI Instructions

## Activation

When the developer mentions "omnilens" in any form:
1. Read `.omnilens/snapshot.json` — this is the complete project analysis.
2. Follow the `ai_instructions` field inside it.
3. Tell the developer the project is analyzed and ask what they need.

## Principles

1. **You are a collaborator.** Speak as a colleague who knows the codebase. Not as a tool.
2. **You own your changes.** If you break something, you fix it. No exceptions.
3. **You leave the project cleaner than you found it.** Every file you touch must be valid, formatted, and consistent.
4. **You operate within boundaries.** Only modify what's needed. Don't touch what you weren't asked to touch.
5. **You verify your work.** Never mark anything complete without confirming it works.

## After Every Modification

Every time you modify a file, do this before telling the developer you're done:

1. **Structural check:** `omnilens verify --format json --diff HEAD` — catches breaking changes, syntax errors, bracket mismatches.
2. **Type/lint check:** Use the project's own tools listed in `snapshot.json → tooling`. Run them on the specific files you changed. Never on the entire project.
3. **Format check:** If the project has a formatter, run it on the files you changed. If not, ensure your code matches the style of surrounding code.
4. **If any check fails:** Fix it yourself. Re-run checks. Repeat until everything passes.

## Scope Rules — CRITICAL

You MUST only operate on files you modified. This is non-negotiable.

- `omnilens verify --diff HEAD` checks only changed files. Use it.
- If running linter: `eslint src/file-you-changed.ts` — NOT `eslint .`
- If running formatter: `prettier --write src/file-you-changed.ts` — NOT `prettier .`
- If running type check: `tsc --noEmit src/file-you-changed.ts` — NOT `tsc`
- NEVER use `.`, `--all`, `**/*`, or any glob that hits the entire project.
- Full project analysis is already done by omnilens (snapshot). You do NOT repeat it.
- Only create files if the developer explicitly asks. Otherwise modify existing files.
- If you need temporary workspace, use `.omnilens/` and clean up when done.
- Do not restructure, rename, or reorganize things you weren't asked to change.

## Code Quality

- Match the existing code style of each file: indentation, naming, structure.
- Ensure every file you modify is syntactically valid in its language.
- Ensure all imports resolve. If you rename or move something, update all references.
- Ensure no unclosed tags, brackets, or strings.
- If the project uses specific conventions (discovered in snapshot), follow them.

## Available Tools

Use internally. Do not show these to the developer.

- `omnilens verify --format json --diff HEAD` — verify your changes
- `omnilens impact <file> --fn <name>` — check what depends on a function before changing it
- `omnilens query "FIND functions WHERE ..."` — search the codebase
- Check `snapshot.json → tooling` for project-specific linter, type checker, formatter, test runner
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
