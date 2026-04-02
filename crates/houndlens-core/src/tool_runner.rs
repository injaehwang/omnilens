//! Project tool runner — executes the project's own linter, type checker,
//! formatter on changed files and parses their output into structured errors.
//!
//! houndlens doesn't replace these tools. It wraps them into a single
//! command with unified JSON output.

use std::path::Path;
use std::process::Command;

use crate::snapshot::Tooling;

/// A tool error parsed from linter/type checker output.
#[derive(Debug)]
pub struct ToolError {
    pub tool: String,
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub message: String,
    pub severity: ToolSeverity,
}

#[derive(Debug)]
pub enum ToolSeverity {
    Error,
    Warning,
}

/// Run all detected project tools on the specified files.
/// Returns structured errors from each tool.
pub fn run_project_tools(
    tooling: &Tooling,
    changed_files: &[String],
    cwd: &Path,
) -> Vec<ToolError> {
    if changed_files.is_empty() {
        return Vec::new();
    }

    let mut all_errors = Vec::new();

    // Filter files by language for each tool.
    let ts_files: Vec<&str> = changed_files.iter()
        .filter(|f| {
            let ext = f.rsplit('.').next().unwrap_or("");
            matches!(ext, "ts" | "tsx" | "js" | "jsx" | "vue" | "mts" | "mjs")
        })
        .map(|f| f.as_str())
        .collect();

    let py_files: Vec<&str> = changed_files.iter()
        .filter(|f| f.ends_with(".py") || f.ends_with(".pyi"))
        .map(|f| f.as_str())
        .collect();

    let rs_files: Vec<&str> = changed_files.iter()
        .filter(|f| f.ends_with(".rs"))
        .map(|f| f.as_str())
        .collect();

    // Type checker (tsc / vue-tsc).
    if let Some(ref type_cmd) = tooling.type_check {
        if !ts_files.is_empty() {
            let errors = run_type_checker(type_cmd, &ts_files, cwd);
            all_errors.extend(errors);
        }
    }

    // Linter (eslint / pylint).
    if let Some(ref lint_cmd) = tooling.linter {
        if lint_cmd.contains("eslint") && !ts_files.is_empty() {
            let errors = run_eslint(&ts_files, cwd);
            all_errors.extend(errors);
        }
        if lint_cmd.contains("pylint") || lint_cmd.contains("flake8") {
            if !py_files.is_empty() {
                let errors = run_python_lint(lint_cmd, &py_files, cwd);
                all_errors.extend(errors);
            }
        }
    }

    // Python syntax check (always, if python files changed).
    if !py_files.is_empty() && tooling.linter.is_none() {
        let errors = run_python_compile(&py_files, cwd);
        all_errors.extend(errors);
    }

    // Rust (cargo check).
    if !rs_files.is_empty() {
        let errors = run_cargo_check(cwd);
        all_errors.extend(errors);
    }

    all_errors
}

fn run_type_checker(cmd: &str, files: &[&str], cwd: &Path) -> Vec<ToolError> {
    // tsc/vue-tsc --noEmit checks the entire project but we filter results to changed files.
    let output = Command::new("sh")
        .args(["-c", &format!("{} 2>&1", cmd)])
        .current_dir(cwd)
        .output();

    // Windows fallback.
    let output = output.or_else(|_| {
        Command::new("cmd")
            .args(["/C", &format!("{} 2>&1", cmd)])
            .current_dir(cwd)
            .output()
    });

    let Ok(out) = output else { return Vec::new() };
    let stdout = String::from_utf8_lossy(&out.stdout);

    parse_tsc_output(&stdout, files)
}

fn run_eslint(files: &[&str], cwd: &Path) -> Vec<ToolError> {
    let file_args = files.join(" ");
    let cmd = format!("npx eslint --format json {} 2>/dev/null", file_args);

    let output = Command::new("sh")
        .args(["-c", &cmd])
        .current_dir(cwd)
        .output()
        .or_else(|_| {
            Command::new("cmd")
                .args(["/C", &cmd.replace("2>/dev/null", "2>nul")])
                .current_dir(cwd)
                .output()
        });

    let Ok(out) = output else { return Vec::new() };
    let stdout = String::from_utf8_lossy(&out.stdout);

    parse_eslint_json(&stdout)
}

fn run_python_lint(cmd: &str, files: &[&str], cwd: &Path) -> Vec<ToolError> {
    let file_args = files.join(" ");
    let full_cmd = format!("{} {} 2>&1", cmd, file_args);

    let output = Command::new("sh")
        .args(["-c", &full_cmd])
        .current_dir(cwd)
        .output()
        .or_else(|_| {
            Command::new("cmd")
                .args(["/C", &full_cmd])
                .current_dir(cwd)
                .output()
        });

    let Ok(out) = output else { return Vec::new() };
    let stdout = String::from_utf8_lossy(&out.stdout);

    parse_pylint_output(&stdout)
}

fn run_python_compile(files: &[&str], cwd: &Path) -> Vec<ToolError> {
    let mut errors = Vec::new();

    for file in files {
        let cmd = format!("python -m py_compile {} 2>&1", file);
        let output = Command::new("sh")
            .args(["-c", &cmd])
            .current_dir(cwd)
            .output()
            .or_else(|_| {
                Command::new("cmd")
                    .args(["/C", &cmd])
                    .current_dir(cwd)
                    .output()
            });

        if let Ok(out) = output {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stdout);
                for line in stderr.lines() {
                    if line.contains("SyntaxError") || line.contains("Error") {
                        errors.push(ToolError {
                            tool: "python".into(),
                            file: file.to_string(),
                            line: extract_line_number(line),
                            col: 0,
                            message: line.trim().to_string(),
                            severity: ToolSeverity::Error,
                        });
                    }
                }
            }
        }
    }

    errors
}

fn run_cargo_check(cwd: &Path) -> Vec<ToolError> {
    let output = Command::new("cargo")
        .args(["check", "--message-format=json"])
        .current_dir(cwd)
        .output();

    let Ok(out) = output else { return Vec::new() };
    let stdout = String::from_utf8_lossy(&out.stdout);

    parse_cargo_output(&stdout)
}

// ─── Output parsers ─────────────────────────────────────────────

fn parse_tsc_output(output: &str, changed_files: &[&str]) -> Vec<ToolError> {
    let mut errors = Vec::new();

    // tsc output format: "src/auth.ts(42,5): error TS2345: ..."
    for line in output.lines() {
        if let Some((file_part, rest)) = line.split_once("): ") {
            if let Some((file, pos)) = file_part.rsplit_once('(') {
                let file_normalized = file.replace('\\', "/");

                // Only include errors from changed files.
                let is_changed = changed_files.iter().any(|f| {
                    file_normalized.ends_with(f) || f.ends_with(&file_normalized)
                });

                if !is_changed { continue; }

                let parts: Vec<&str> = pos.split(',').collect();
                let line_num = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
                let col_num = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

                let is_error = rest.starts_with("error");

                errors.push(ToolError {
                    tool: "tsc".into(),
                    file: file_normalized,
                    line: line_num,
                    col: col_num,
                    message: rest.to_string(),
                    severity: if is_error { ToolSeverity::Error } else { ToolSeverity::Warning },
                });
            }
        }
    }

    errors
}

fn parse_eslint_json(output: &str) -> Vec<ToolError> {
    let mut errors = Vec::new();

    // ESLint JSON format: [{ "filePath": "...", "messages": [{ "line": 1, "column": 1, "message": "...", "severity": 2 }] }]
    if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(output) {
        for result in &results {
            let file = result["filePath"].as_str().unwrap_or("").replace('\\', "/");
            if let Some(messages) = result["messages"].as_array() {
                for msg in messages {
                    let severity = msg["severity"].as_u64().unwrap_or(0);
                    if severity == 0 { continue; }

                    errors.push(ToolError {
                        tool: "eslint".into(),
                        file: file.clone(),
                        line: msg["line"].as_u64().unwrap_or(0) as u32,
                        col: msg["column"].as_u64().unwrap_or(0) as u32,
                        message: format!(
                            "{} ({})",
                            msg["message"].as_str().unwrap_or(""),
                            msg["ruleId"].as_str().unwrap_or(""),
                        ),
                        severity: if severity >= 2 { ToolSeverity::Error } else { ToolSeverity::Warning },
                    });
                }
            }
        }
    }

    errors
}

fn parse_pylint_output(output: &str) -> Vec<ToolError> {
    let mut errors = Vec::new();

    // pylint format: "src/auth.py:42:0: E0001: ..."
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(5, ':').collect();
        if parts.len() >= 5 {
            let file = parts[0].trim().replace('\\', "/");
            let line_num: u32 = parts[1].trim().parse().unwrap_or(0);
            let col: u32 = parts[2].trim().parse().unwrap_or(0);
            let code = parts[3].trim();
            let message = parts[4].trim();

            let is_error = code.starts_with(" E") || code.starts_with(" F");

            errors.push(ToolError {
                tool: "pylint".into(),
                file,
                line: line_num,
                col,
                message: format!("{}: {}", code.trim(), message),
                severity: if is_error { ToolSeverity::Error } else { ToolSeverity::Warning },
            });
        }
    }

    errors
}

fn parse_cargo_output(output: &str) -> Vec<ToolError> {
    let mut errors = Vec::new();

    for line in output.lines() {
        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
            if msg["reason"].as_str() == Some("compiler-message") {
                if let Some(message) = msg.get("message") {
                    let level = message["level"].as_str().unwrap_or("");
                    if level != "error" && level != "warning" { continue; }

                    let text = message["message"].as_str().unwrap_or("").to_string();

                    let (file, line_num, col) = if let Some(spans) = message["spans"].as_array() {
                        if let Some(span) = spans.first() {
                            (
                                span["file_name"].as_str().unwrap_or("").replace('\\', "/"),
                                span["line_start"].as_u64().unwrap_or(0) as u32,
                                span["column_start"].as_u64().unwrap_or(0) as u32,
                            )
                        } else {
                            (String::new(), 0, 0)
                        }
                    } else {
                        (String::new(), 0, 0)
                    };

                    errors.push(ToolError {
                        tool: "cargo".into(),
                        file,
                        line: line_num,
                        col,
                        message: text,
                        severity: if level == "error" { ToolSeverity::Error } else { ToolSeverity::Warning },
                    });
                }
            }
        }
    }

    errors
}

fn extract_line_number(text: &str) -> u32 {
    // Try to find "line N" pattern.
    if let Some(pos) = text.find("line ") {
        let rest = &text[pos + 5..];
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        num.parse().unwrap_or(0)
    } else {
        0
    }
}
