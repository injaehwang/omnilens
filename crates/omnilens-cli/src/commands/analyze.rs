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

    // Backup previous snapshot for diff.
    let snapshot_path = omnilens_dir.join("snapshot.json");
    let prev_path = omnilens_dir.join("snapshot.prev.json");
    if snapshot_path.exists() {
        let _ = std::fs::copy(&snapshot_path, &prev_path);
    }

    // Write full snapshot (for omnilens internal use).
    let snapshot_json = serde_json::to_string_pretty(&snapshot)?;
    std::fs::write(&snapshot_path, &snapshot_json)?;

    // Write summary (lightweight, AI reads this).
    let summary = omnilens_core::summary::generate_summary(&snapshot);
    let summary_json = serde_json::to_string_pretty(&summary)?;
    std::fs::write(omnilens_dir.join("summary.json"), &summary_json)?;

    // Write changes (if previous snapshot exists).
    if prev_path.exists() {
        if let Some(changes) = omnilens_core::summary::generate_changes(&snapshot, &prev_path) {
            let changes_json = serde_json::to_string_pretty(&changes)?;
            std::fs::write(omnilens_dir.join("changes.json"), &changes_json)?;
        } else {
            // No changes — remove stale changes.json.
            let _ = std::fs::remove_file(omnilens_dir.join("changes.json"));
        }
    }

    // Write AI context files.
    write_ai_context(&cwd)?;

    // Install git pre-commit hook (harness — works with ALL AI tools).
    install_harness_hook(&cwd)?;

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
const AI_POINTER: &str = "When the developer mentions \"omnilens\", read `.omnilens/ai-instructions.md` and `.omnilens/summary.json`, then follow the instructions.";

fn write_ai_context(cwd: &std::path::Path) -> Result<()> {
    let omnilens_dir = cwd.join(".omnilens");

    // Full instructions go inside .omnilens/ — not in project root.
    std::fs::write(omnilens_dir.join("ai-instructions.md"), r#"# omnilens — AI Instructions

## Activation

When the developer mentions "omnilens" in any form:
1. Read `.omnilens/summary.json` — lightweight project overview with instructions.
2. If `.omnilens/changes.json` exists, read it — shows what changed since last analysis.
3. Follow the `ai_instructions` in summary.json.
4. Tell the developer the project is analyzed and ask what they need.

Do NOT read `snapshot.json` directly — it's large. Use `summary.json` instead.
For detailed data, use omnilens commands (verify, impact, query).

## Principles

1. **You are a collaborator.** Speak as a colleague who knows the codebase. Not as a tool.
2. **You own your changes.** If you break something, you fix it. No exceptions.
3. **You leave the project cleaner than you found it.** Every file you touch must be valid, formatted, and consistent.
4. **You operate within boundaries.** Only modify what's needed. Don't touch what you weren't asked to touch.
5. **You verify your work.** Never mark anything complete without confirming it works.

## After Completing a Logical Change

When you finish a logical unit of work (not after every single file edit), run this:

### Step 1: Rescan (~10ms)
```
omnilens
```
This regenerates `summary.json` and `changes.json`. Read `changes.json` — it shows exactly what your modifications affected: broken dependencies, added/removed functions, health delta.

### Step 2: Verify (changed files only)
```
omnilens verify --format json --diff HEAD
```
Catches syntax errors, bracket mismatches, breaking changes.

### Step 3: Project lint (changed files only)
Use the project's own tools from `summary.json → tooling`, targeting ONLY files you changed.

### Step 4: Fix and repeat
If any step finds errors, fix them. Then go back to Step 1.

### When to run this cycle
- After finishing all modifications for a task — NOT after every single file save.
- If you're editing 5 files for one task, finish all 5 first, then run this cycle once.
- Exception: if you're unsure whether a change broke something, run it mid-task. It's only 10ms.

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

/// Install git pre-commit hook that runs omnilens verify.
/// This is the HARNESS — it works with ALL AI tools (Claude, Cursor, Gemini, etc.)
/// because every tool eventually commits through git.
fn install_harness_hook(cwd: &std::path::Path) -> Result<()> {
    // Find git hooks directory.
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(cwd)
        .output();

    let git_dir = match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => return Ok(()), // Not a git repo, skip.
    };

    let hooks_dir = std::path::Path::new(&git_dir).join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;

    let pre_commit = hooks_dir.join("pre-commit");

    // Only install if no hook exists or it's our hook.
    if pre_commit.exists() {
        let content = std::fs::read_to_string(&pre_commit)?;
        if !content.contains("omnilens") {
            return Ok(()); // User has their own hook, don't overwrite.
        }
    }

    std::fs::write(&pre_commit, r#"#!/bin/sh
# omnilens harness — auto-verify before commit
# This runs with ALL AI tools: Claude, Cursor, Gemini, Copilot, etc.

if command -v omnilens > /dev/null 2>&1; then
    echo "[omnilens] Verifying changes..."

    # Rescan project.
    omnilens > /dev/null 2>&1

    # Verify changed files.
    RESULT=$(omnilens verify --format json --diff HEAD 2>/dev/null)
    BREAKING=$(echo "$RESULT" | grep -o '"breaking":[0-9]*' | grep -o '[0-9]*')

    if [ "${BREAKING:-0}" -gt 0 ]; then
        echo "[omnilens] BLOCKED: $BREAKING breaking changes detected."
        echo "[omnilens] Run: omnilens verify --diff HEAD"
        exit 1
    fi

    echo "[omnilens] OK"
fi
exit 0
"#)?;

    // Make executable on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&pre_commit, std::fs::Permissions::from_mode(0o755))?;
    }

    // Also set up Claude Code hooks if .claude directory exists or can be created.
    install_claude_hooks(cwd)?;

    Ok(())
}

/// Install Claude Code hooks for real-time verification.
fn install_claude_hooks(cwd: &std::path::Path) -> Result<()> {
    let claude_dir = cwd.join(".claude");
    let hooks_dir = claude_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;

    // Write the hook script that outputs JSON with additionalContext.
    // This is how we send verify results back to Claude's conversation.
    let hook_script = hooks_dir.join("omnilens-verify.sh");
    std::fs::write(&hook_script, r#"#!/bin/sh
# omnilens harness hook — sends verify results to Claude's context.

if ! command -v omnilens > /dev/null 2>&1; then
    exit 0
fi

# Rescan and verify.
omnilens > /dev/null 2>&1
RESULT=$(omnilens verify --format json --diff HEAD 2>/dev/null)
BREAKING=$(echo "$RESULT" | grep -o '"breaking":[0-9]*' | grep -o '[0-9]*')
CHANGES=$(echo "$RESULT" | grep -o '"total_changes":[0-9]*' | grep -o '[0-9]*')

if [ "${BREAKING:-0}" -gt 0 ]; then
    # Send breaking change info to Claude via additionalContext.
    echo "{\"hookSpecificOutput\":{\"hookEventName\":\"PostToolUse\",\"additionalContext\":\"omnilens: ${BREAKING} breaking changes detected. Read .omnilens/changes.json and fix them before continuing.\"}}"
    exit 0
elif [ "${CHANGES:-0}" -gt 0 ]; then
    echo "{\"hookSpecificOutput\":{\"hookEventName\":\"PostToolUse\",\"additionalContext\":\"omnilens: ${CHANGES} semantic changes detected. Check .omnilens/changes.json if needed.\"}}"
    exit 0
fi

exit 0
"#)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_script, std::fs::Permissions::from_mode(0o755))?;
    }

    // Write settings.local.json with correct Claude Code hook format.
    let settings_path = claude_dir.join("settings.local.json");

    let hooks_config = serde_json::json!({
        "hooks": {
            "PostToolUse": [
                {
                    "matcher": "Edit|Write",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/omnilens-verify.sh",
                            "timeout": 30
                        }
                    ]
                }
            ]
        }
    });

    // Read existing settings and merge.
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Only add hooks if not already present.
    if settings.get("hooks").is_none() {
        if let Some(obj) = settings.as_object_mut() {
            obj.insert("hooks".to_string(), hooks_config["hooks"].clone());
        }
        std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
    }

    Ok(())
}
