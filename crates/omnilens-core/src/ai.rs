//! AI communication protocol — structured JSON interface for any AI agent.
//!
//! omnilens sends a FixRequest, AI returns a FixResponse.
//! This works with Claude CLI, OpenAI API, local LLMs, or any custom command.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ─── Protocol ───────────────────────────────────────────────────

/// Request sent from omnilens to AI.
#[derive(Debug, Serialize)]
pub struct FixRequest {
    /// What omnilens wants the AI to do.
    pub task: String,
    /// Test failures with file, line, error message.
    pub failures: Vec<TestFailure>,
    /// Test files the AI should modify.
    pub test_files: Vec<FileContent>,
    /// Source files for reference (read-only).
    pub source_files: Vec<FileContent>,
    /// Rules the AI must follow.
    pub rules: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestFailure {
    pub test_name: String,
    pub file: String,
    pub line: Option<u32>,
    pub error_type: String,
    pub error_message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    /// If true, AI must not modify this file.
    pub readonly: bool,
}

/// Response from AI to omnilens.
#[derive(Debug, Deserialize)]
pub struct FixResponse {
    /// Files the AI has modified.
    pub edits: Vec<FileEdit>,
    /// Explanation of what was changed.
    pub explanation: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileEdit {
    pub path: String,
    pub content: String,
}

// ─── Adapter trait ──────────────────────────────────────────────

pub trait AiAdapter {
    fn name(&self) -> &str;
    fn fix(&self, request: &FixRequest, cwd: &Path) -> Result<FixResponse>;
}

// ─── Adapter detection ──────────────────────────────────────────

pub fn detect_adapter() -> Option<Box<dyn AiAdapter>> {
    // 1. OMNILENS_AI_CMD env var.
    if let Ok(cmd) = std::env::var("OMNILENS_AI_CMD") {
        if !cmd.is_empty() {
            return Some(Box::new(CustomAdapter { command: cmd }));
        }
    }

    // 2. Claude CLI.
    if command_exists("claude") {
        return Some(Box::new(ClaudeCliAdapter));
    }

    // 3. OpenAI API key.
    if std::env::var("OPENAI_API_KEY").is_ok() {
        return Some(Box::new(OpenAiAdapter));
    }

    None
}

fn command_exists(cmd: &str) -> bool {
    #[cfg(windows)]
    {
        Command::new("where")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

// ─── Claude CLI adapter ─────────────────────────────────────────

struct ClaudeCliAdapter;

impl AiAdapter for ClaudeCliAdapter {
    fn name(&self) -> &str {
        "claude"
    }

    fn fix(&self, request: &FixRequest, cwd: &Path) -> Result<FixResponse> {
        let prompt = build_prompt(request);

        // Write request to file for claude to read.
        let req_path = cwd.join(".omnilens-ai-request.json");
        let req_json = serde_json::to_string_pretty(request)?;
        std::fs::write(&req_path, &req_json)?;

        // Call claude with structured prompt.
        let output = Command::new("claude")
            .args([
                "-p",
                &prompt,
                "--output-format", "json",
                "--allowedTools", "Edit,Write,Read",
            ])
            .current_dir(cwd)
            .output()
            .context("Failed to run claude")?;

        let _ = std::fs::remove_file(&req_path);

        if !output.status.success() {
            // Claude may have edited files directly — check for changes.
            return Ok(collect_file_changes(cwd, request));
        }

        // Try to parse JSON response.
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(resp) = serde_json::from_str::<FixResponse>(&stdout) {
            return Ok(resp);
        }

        // Claude likely edited files directly instead of returning JSON.
        Ok(collect_file_changes(cwd, request))
    }
}

// ─── OpenAI API adapter ─────────────────────────────────────────

struct OpenAiAdapter;

impl AiAdapter for OpenAiAdapter {
    fn name(&self) -> &str {
        "openai"
    }

    fn fix(&self, request: &FixRequest, _cwd: &Path) -> Result<FixResponse> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY not set")?;

        let prompt = build_prompt(request);
        let system = "You are a test fixer. Given failing tests and source code, return a JSON object with an 'edits' array. Each edit has 'path' (file path) and 'content' (full new file content). Only modify test files.";

        let body = serde_json::json!({
            "model": std::env::var("OMNILENS_AI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string()),
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": prompt}
            ],
            "response_format": {"type": "json_object"},
            "temperature": 0.2
        });

        let output = Command::new("curl")
            .args([
                "-s",
                "-X", "POST",
                "https://api.openai.com/v1/chat/completions",
                "-H", &format!("Authorization: Bearer {}", api_key),
                "-H", "Content-Type: application/json",
                "-d", &body.to_string(),
            ])
            .output()
            .context("Failed to call OpenAI API")?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse OpenAI response.
        let api_resp: serde_json::Value = serde_json::from_str(&stdout)
            .context("Failed to parse OpenAI response")?;

        let content = api_resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("{}");

        let fix_resp: FixResponse = serde_json::from_str(content)
            .unwrap_or(FixResponse { edits: vec![], explanation: None });

        Ok(fix_resp)
    }
}

// ─── Custom command adapter ─────────────────────────────────────

struct CustomAdapter {
    command: String,
}

impl AiAdapter for CustomAdapter {
    fn name(&self) -> &str {
        &self.command
    }

    fn fix(&self, request: &FixRequest, cwd: &Path) -> Result<FixResponse> {
        // Write request JSON to stdin, read response JSON from stdout.
        let req_json = serde_json::to_string(request)?;

        let output = Command::new(&self.command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .current_dir(cwd)
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(ref mut stdin) = child.stdin {
                    stdin.write_all(req_json.as_bytes())?;
                }
                child.wait_with_output()
            })
            .context("Failed to run custom AI command")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let resp: FixResponse = serde_json::from_str(&stdout)
            .unwrap_or(FixResponse { edits: vec![], explanation: None });

        Ok(resp)
    }
}

// ─── Helpers ────────────────────────────────────────────────────

fn build_prompt(request: &FixRequest) -> String {
    let mut prompt = String::new();

    prompt.push_str(&request.task);
    prompt.push_str("\n\n");

    prompt.push_str("RULES:\n");
    for rule in &request.rules {
        prompt.push_str(&format!("- {}\n", rule));
    }
    prompt.push('\n');

    prompt.push_str("FAILURES:\n");
    for f in &request.failures {
        prompt.push_str(&format!(
            "  {} ({}:{}) — {}: {}\n",
            f.test_name,
            f.file,
            f.line.unwrap_or(0),
            f.error_type,
            f.error_message
        ));
    }
    prompt.push('\n');

    prompt.push_str("TEST FILES (modify these):\n");
    for f in &request.test_files {
        prompt.push_str(&format!("--- {} ---\n{}\n\n", f.path, f.content));
    }

    prompt.push_str("SOURCE FILES (read-only, for reference):\n");
    for f in &request.source_files {
        prompt.push_str(&format!("--- {} ---\n{}\n\n", f.path, f.content));
    }

    prompt
}

/// After Claude edits files directly, read the modified test files.
fn collect_file_changes(cwd: &Path, request: &FixRequest) -> FixResponse {
    let mut edits = Vec::new();

    for test_file in &request.test_files {
        let path = cwd.join(&test_file.path);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content != test_file.content {
                edits.push(FileEdit {
                    path: test_file.path.clone(),
                    content,
                });
            }
        }
    }

    FixResponse {
        edits,
        explanation: Some("Claude edited files directly".to_string()),
    }
}

/// Parse pytest/jest output into structured failures.
pub fn parse_test_failures(output: &str) -> Vec<TestFailure> {
    let mut failures = Vec::new();

    for line in output.lines() {
        let line = line.trim();

        // pytest: "FAILED app/services/test_user.py::TestUser::test_get - Error: ..."
        if line.starts_with("FAILED ") {
            let rest = &line[7..];
            let parts: Vec<&str> = rest.splitn(2, " - ").collect();
            let test_path = parts[0].trim();
            let error = parts.get(1).unwrap_or(&"").trim();

            let (file, test_name) = if test_path.contains("::") {
                let p: Vec<&str> = test_path.splitn(2, "::").collect();
                (p[0].to_string(), p[1].to_string())
            } else {
                (test_path.to_string(), String::new())
            };

            failures.push(TestFailure {
                test_name,
                file,
                line: None,
                error_type: "TestFailure".to_string(),
                error_message: error.to_string(),
            });
            continue;
        }

        // Detailed error lines: "E   TypeError: ..."
        if line.starts_with("E   ") && !failures.is_empty() {
            let last = failures.last_mut().unwrap();
            let err = &line[4..];
            if let Some((etype, msg)) = err.split_once(": ") {
                last.error_type = etype.trim().to_string();
                last.error_message = msg.trim().to_string();
            } else if last.error_message.is_empty() {
                last.error_message = err.to_string();
            }
        }

        // Line number: "file.py:42: in test_foo"
        if line.contains(": in test_") {
            let parts: Vec<&str> = line.splitn(3, ':').collect();
            if parts.len() >= 2 {
                if let Ok(num) = parts[1].trim().parse::<u32>() {
                    if let Some(last) = failures.last_mut() {
                        last.line = Some(num);
                    }
                }
            }
        }
    }

    failures
}
