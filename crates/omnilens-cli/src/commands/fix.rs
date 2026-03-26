//! `omnilens fix` — auto-generate tests, run them, optionally let AI fix failures.
//!
//! Flow:
//!   1. Generate test files for untested public functions
//!   2. Run tests
//!   3. If --auto: send failures to AI → AI fixes → rerun → repeat until pass

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use colored::Colorize;
use omnilens_ir::node::UsirNode;
use omnilens_ir::Visibility;

pub fn run(files: Vec<String>, auto: bool, max_retries: u32) -> Result<()> {
    let mut engine = super::create_engine()?;
    engine.index()?;

    let graph = &engine.graph;
    let all_ids = graph.all_node_ids();

    // Collect public functions that need tests, grouped by file.
    let mut file_functions: BTreeMap<String, Vec<FnInfo>> = BTreeMap::new();

    for id in &all_ids {
        let Some(node) = graph.get_node(*id) else { continue };
        if graph.is_placeholder(*id) { continue; }

        let UsirNode::Function(f) = node else { continue };
        if f.complexity.is_none() { continue; } // placeholder
        if f.visibility != Visibility::Public { continue; }

        let file_path = f.span.file.to_string_lossy().replace('\\', "/");

        // Skip test files — don't generate tests for tests.
        let file_name = file_path.rsplit('/').next().unwrap_or(&file_path);
        if file_name.starts_with("test_") || file_name.contains(".test.") || file_name.contains(".spec.") || file_name.starts_with("tests_") {
            continue;
        }

        // Filter by specified files if any.
        if !files.is_empty() {
            let matches = files.iter().any(|filter| file_path.contains(filter));
            if !matches { continue; }
        }

        let file_short = file_path.rsplit('/').next().unwrap_or(&file_path).to_string();
        let ext = Path::new(&file_short)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let fn_name = f.name.segments.last().cloned().unwrap_or_default();
        let class_name = if f.name.segments.len() > 1 {
            Some(f.name.segments[f.name.segments.len() - 2].clone())
        } else {
            None
        };

        let params: Vec<ParamInfo> = f.params.iter().map(|p| {
            ParamInfo {
                name: p.name.clone(),
                type_str: p.type_ref.as_ref().map(|t| format!("{:?}", t)).unwrap_or_default(),
            }
        }).collect();

        file_functions
            .entry(file_path.clone())
            .or_default()
            .push(FnInfo {
                name: fn_name,
                class_name,
                params,
                is_async: f.is_async,
                ext: ext.to_string(),
                file_path: file_path.clone(),
            });
    }

    if file_functions.is_empty() {
        println!("\n  {} Nothing to fix.\n", "✓".green().bold());
        return Ok(());
    }

    println!();
    let mut files_written = 0;
    let mut tests_generated = 0;

    for (source_file, functions) in &file_functions {
        let ext = Path::new(source_file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let (test_path, test_content) = match ext {
            "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs" => {
                generate_ts_tests(source_file, functions)
            }
            "py" | "pyi" => {
                generate_py_tests(source_file, functions)
            }
            "rs" => {
                generate_rs_tests(source_file, functions)
            }
            _ => continue,
        };

        // Write test file.
        let path = PathBuf::from(&test_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let exists = path.exists();
        if exists {
            // Append to existing test file.
            let existing = std::fs::read_to_string(&path)?;
            if !existing.contains(&functions[0].name) {
                let appended = format!("{}\n\n{}", existing.trim_end(), test_content);
                std::fs::write(&path, appended)?;
                tests_generated += functions.len();
                let short = test_path.rsplit('/').next().unwrap_or(&test_path);
                println!(
                    "  {} {} — appended {} tests",
                    "✓".green(),
                    short,
                    functions.len()
                );
            } else {
                let short = test_path.rsplit('/').next().unwrap_or(&test_path);
                println!(
                    "  {} {} — tests already exist, skipped",
                    "·".dimmed(),
                    short,
                );
                continue;
            }
        } else {
            std::fs::write(&path, &test_content)?;
            tests_generated += functions.len();
            let short = test_path.rsplit('/').next().unwrap_or(&test_path);
            println!(
                "  {} {} — {} tests",
                "✓".green(),
                short,
                functions.len()
            );
        }
        files_written += 1;
    }

    println!();
    if tests_generated == 0 {
        return Ok(());
    }

    println!(
        "  {} {} tests generated in {} files",
        "Done.".green().bold(),
        tests_generated,
        files_written,
    );

    // Auto-detect and run test runners.
    println!();
    println!("  {}", "Running tests...".cyan());
    println!();

    let cwd = std::env::current_dir()?;
    let mut any_ran = false;
    let mut any_failed = false;

    // TypeScript/JavaScript: vitest > jest > npx vitest
    if file_functions.keys().any(|f| {
        let ext = Path::new(f).extension().and_then(|e| e.to_str()).unwrap_or("");
        matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs")
    }) {
        let runner = detect_ts_runner(&cwd);
        if let Some((cmd, args)) = runner {
            println!("  {} {}", "▶".cyan(), format!("{} {}", cmd, args.join(" ")).dimmed());
            let status = std::process::Command::new(&cmd)
                .args(&args)
                .current_dir(&cwd)
                .status();
            match status {
                Ok(s) => {
                    any_ran = true;
                    if !s.success() {
                        any_failed = true;
                        println!("  {} TypeScript tests failed", "✗".red());
                    } else {
                        println!("  {} TypeScript tests passed", "✓".green());
                    }
                }
                Err(e) => {
                    println!("  {} Could not run {}: {}", "!".yellow(), cmd, e);
                }
            }
            println!();
        }
    }

    // Python: pytest
    if file_functions.keys().any(|f| {
        let ext = Path::new(f).extension().and_then(|e| e.to_str()).unwrap_or("");
        matches!(ext, "py" | "pyi")
    }) {
        let runner = detect_py_runner(&cwd);
        if let Some((cmd, args)) = runner {
            println!("  {} {}", "▶".cyan(), format!("{} {}", cmd, args.join(" ")).dimmed());
            let status = std::process::Command::new(&cmd)
                .args(&args)
                .current_dir(&cwd)
                .env("PYTHONPATH", &cwd)
                .status();
            match status {
                Ok(s) => {
                    any_ran = true;
                    if !s.success() {
                        any_failed = true;
                        println!("  {} Python tests failed", "✗".red());
                    } else {
                        println!("  {} Python tests passed", "✓".green());
                    }
                }
                Err(e) => {
                    println!("  {} Could not run {}: {}", "!".yellow(), cmd, e);
                }
            }
            println!();
        }
    }

    // Rust: cargo test
    if file_functions.keys().any(|f| f.ends_with(".rs")) {
        println!("  {} cargo test", "▶".cyan());
        let status = std::process::Command::new("cargo")
            .args(["test"])
            .current_dir(&cwd)
            .status();
        match status {
            Ok(s) => {
                any_ran = true;
                if !s.success() {
                    any_failed = true;
                    println!("  {} Rust tests failed", "✗".red());
                } else {
                    println!("  {} Rust tests passed", "✓".green());
                }
            }
            Err(e) => {
                println!("  {} Could not run cargo test: {}", "!".yellow(), e);
            }
        }
        println!();
    }

    if !any_ran {
        println!(
            "  {} No test runner found. Install {}, {}, or {}",
            "!".yellow(),
            "vitest".cyan(),
            "pytest".cyan(),
            "cargo".cyan(),
        );
        return Ok(());
    }

    if !any_failed {
        println!("  {} All tests passed", "✓".green().bold());
        println!();
        return Ok(());
    }

    // Tests failed.
    if !auto {
        println!("  {} Some tests failed — run {} to let AI fix them", "!".yellow(), "omnilens fix --auto".cyan());
        println!();
        std::process::exit(1);
    }

    // ─── AI-assisted fix loop ───────────────────────────────
    println!();
    println!("  {}", "Starting AI-assisted fix loop...".cyan().bold());

    let adapter = omnilens_core::ai::detect_adapter();
    if adapter.is_none() {
        println!("  {} No AI agent found.", "!".yellow());
        println!("    Options:");
        println!("      {} in PATH (Claude Code CLI)", "claude".cyan());
        println!("      {} env var (OpenAI API)", "OPENAI_API_KEY".cyan());
        println!("      {} env var (any command)", "OMNILENS_AI_CMD".cyan());
        println!();
        std::process::exit(1);
    }
    let adapter = adapter.unwrap();
    println!("  AI: {}", adapter.name().cyan());

    let cwd = std::env::current_dir()?;

    for attempt in 1..=max_retries {
        println!();
        println!("  {} Attempt {}/{}", "●".cyan(), attempt, max_retries);

        // Run tests and collect failures.
        let raw_output = run_tests_and_capture(&cwd, &file_functions);
        let failures = omnilens_core::ai::parse_test_failures(&raw_output);

        if failures.is_empty() {
            println!("  {} All tests passed!", "✓".green().bold());
            println!();
            return Ok(());
        }

        let failure_count = failures.len();
        println!("  {} {} failures found", "→".yellow(), failure_count);

        // Build structured request.
        let request = build_fix_request(&cwd, failures);

        // Call AI via protocol.
        println!("  {} Sending to {}...", "→".cyan(), adapter.name());
        let response = adapter.fix(&request, &cwd);

        match response {
            Ok(resp) => {
                // Apply edits.
                let mut applied = 0;
                for edit in &resp.edits {
                    let path = cwd.join(&edit.path);
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if std::fs::write(&path, &edit.content).is_ok() {
                        applied += 1;
                    }
                }

                if let Some(ref explanation) = resp.explanation {
                    println!("  {} {}", "AI:".dimmed(), explanation.dimmed());
                }

                if applied == 0 {
                    println!("  {} AI returned no edits", "·".dimmed());
                    // For Claude CLI which edits files directly, rerun anyway.
                } else {
                    println!("  {} {} files modified", "✓".green(), applied);
                }
            }
            Err(e) => {
                println!("  {} AI error: {}", "✗".red(), e);
                continue;
            }
        }

        // Rerun tests.
        println!("  {} Rerunning tests...", "→".cyan());
        let rerun_output = run_tests_and_capture(&cwd, &file_functions);
        let rerun_failures = omnilens_core::ai::parse_test_failures(&rerun_output);

        if rerun_failures.is_empty() {
            println!();
            println!("  {} All tests passed after AI fix!", "✓".green().bold());
            println!();
            return Ok(());
        }

        let prev = failure_count;
        let curr = rerun_failures.len();
        if curr < prev {
            println!("  {} Progress: {} → {} failures", "↓".yellow(), prev, curr);
        } else {
            println!("  {} Still {} failures", "·".dimmed(), curr);
        }
    }

    println!();
    println!(
        "  {} Could not fix all tests after {} attempts",
        "!".yellow(),
        max_retries
    );
    println!("  Review the failing tests manually.");
    println!();
    std::process::exit(1);
}

// ─── Test execution ─────────────────────────────────────────────

fn run_tests_and_capture(cwd: &Path, file_functions: &BTreeMap<String, Vec<FnInfo>>) -> String {
    let mut output = String::new();

    let has_py = file_functions.keys().any(|f| f.ends_with(".py"));
    if has_py {
        if let Some((cmd, args)) = detect_py_runner(cwd) {
            if let Ok(r) = std::process::Command::new(&cmd)
                .args(&args)
                .current_dir(cwd)
                .env("PYTHONPATH", cwd)
                .output()
            {
                output.push_str(&String::from_utf8_lossy(&r.stdout));
                output.push_str(&String::from_utf8_lossy(&r.stderr));
            }
        }
    }

    let has_ts = file_functions.keys().any(|f| {
        let ext = Path::new(f).extension().and_then(|e| e.to_str()).unwrap_or("");
        matches!(ext, "ts" | "tsx" | "js" | "jsx")
    });
    if has_ts {
        if let Some((cmd, args)) = detect_ts_runner(cwd) {
            if let Ok(r) = std::process::Command::new(&cmd)
                .args(&args)
                .current_dir(cwd)
                .output()
            {
                output.push_str(&String::from_utf8_lossy(&r.stdout));
                output.push_str(&String::from_utf8_lossy(&r.stderr));
            }
        }
    }

    output
}

fn build_fix_request(cwd: &Path, failures: Vec<omnilens_core::ai::TestFailure>) -> omnilens_core::ai::FixRequest {
    use omnilens_core::ai::FileContent;

    let mut test_files = Vec::new();
    let mut source_files = Vec::new();

    let walker = ignore::WalkBuilder::new(cwd)
        .hidden(true)
        .git_ignore(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() { continue; }
        let Ok(content) = std::fs::read_to_string(path) else { continue };
        let relative = path.strip_prefix(cwd).unwrap_or(path)
            .to_string_lossy().replace('\\', "/");

        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let is_test = name.starts_with("test_") || name.contains(".test.") || name.contains(".spec.");
        let is_source = matches!(ext, "py" | "ts" | "js" | "tsx" | "jsx" | "rs");

        if is_test {
            test_files.push(FileContent { path: relative, content, readonly: false });
        } else if is_source {
            source_files.push(FileContent { path: relative, content, readonly: true });
        }
    }

    omnilens_core::ai::FixRequest {
        task: "Fix the failing tests so they pass. The tests were auto-generated and have stub values that need to be replaced with real data based on the source code.".to_string(),
        failures,
        test_files,
        source_files,
        rules: vec![
            "Only modify test files (test_*.py, *.test.ts). NEVER modify source files.".to_string(),
            "Replace None/undefined stubs with realistic mock data.".to_string(),
            "Set up MagicMock/AsyncMock return values to match what the source code expects.".to_string(),
            "Use valid enum values (e.g., 'admin'/'user' for roles, not 'test_role').".to_string(),
            "For functions that return None when given mock data, change assertion to allow None or set up mocks to return valid data.".to_string(),
        ],
    }
}

struct FnInfo {
    name: String,
    class_name: Option<String>,
    params: Vec<ParamInfo>,
    is_async: bool,
    ext: String,
    file_path: String,
}

struct ParamInfo {
    name: String,
    type_str: String,
}

// ─── TypeScript test generation ─────────────────────────────────

fn generate_ts_tests(source_file: &str, functions: &[FnInfo]) -> (String, String) {
    let source_stem = Path::new(source_file)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    // Determine relative import path.
    let test_dir = Path::new(source_file).parent().unwrap_or(Path::new("."));
    let test_path = format!(
        "{}/__tests__/{}.test.ts",
        test_dir.to_string_lossy().replace('\\', "/"),
        source_stem
    );

    let import_names: Vec<&str> = functions
        .iter()
        .filter(|f| f.class_name.is_none())
        .map(|f| f.name.as_str())
        .collect();

    let class_names: Vec<&str> = functions
        .iter()
        .filter_map(|f| f.class_name.as_deref())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut imports = Vec::new();
    if !import_names.is_empty() {
        imports.push(format!(
            "import {{ {} }} from \"../{}\";\n",
            import_names.join(", "),
            source_stem
        ));
    }
    for class in &class_names {
        if !import_names.contains(class) {
            imports.push(format!(
                "import {{ {} }} from \"../{}\";\n",
                class, source_stem
            ));
        }
    }

    let mut tests = imports.join("");
    tests.push('\n');

    for f in functions {
        let async_kw = if f.is_async { "async " } else { "" };
        let await_kw = if f.is_async { "await " } else { "" };

        let display_name = if let Some(ref cls) = f.class_name {
            format!("{}.{}", cls, f.name)
        } else {
            f.name.clone()
        };

        let param_stubs = generate_ts_param_stubs(&f.params);

        tests.push_str(&format!(
            "describe(\"{}\", () => {{\n  it(\"should work\", {}() => {{\n    const result = {}{}({});\n    expect(result).toBeDefined();\n  }});\n\n  it(\"should handle edge cases\", {}() => {{\n    // TODO: add edge case tests\n  }});\n}});\n\n",
            display_name,
            async_kw,
            await_kw,
            if f.class_name.is_some() {
                format!("new {}().{}", f.class_name.as_deref().unwrap_or(""), f.name)
            } else {
                f.name.clone()
            },
            param_stubs,
            async_kw,
        ));
    }

    (test_path, tests)
}

fn generate_ts_param_stubs(params: &[ParamInfo]) -> String {
    params
        .iter()
        .filter(|p| p.name != "self" && p.name != "this")
        .map(|p| {
            if p.type_str.contains("String") || p.type_str.contains("string") {
                format!("\"test_{}\"", p.name)
            } else if p.type_str.contains("Int") || p.type_str.contains("number") || p.type_str.contains("Float") {
                "0".to_string()
            } else if p.type_str.contains("Bool") || p.type_str.contains("boolean") {
                "false".to_string()
            } else {
                "undefined".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ─── Python test generation ─────────────────────────────────────

fn generate_py_tests(source_file: &str, functions: &[FnInfo]) -> (String, String) {
    let source_stem = Path::new(source_file)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    let test_dir = Path::new(source_file).parent().unwrap_or(Path::new("."));
    let test_path = format!(
        "{}/test_{}.py",
        test_dir.to_string_lossy().replace('\\', "/"),
        source_stem
    );

    // Build import: convert file path to Python module path.
    // "/abs/path/app/services/user_service.py" → "app.services.user_service"
    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .replace('\\', "/");
    let relative = source_file
        .replace('\\', "/")
        .strip_prefix(&format!("{}/", cwd))
        .unwrap_or(source_file)
        .to_string();
    let module_path = relative
        .trim_start_matches("./")
        .trim_end_matches(".py")
        .trim_end_matches(".pyi")
        .replace('/', ".");

    let mut content = String::new();
    content.push_str("import pytest\n");

    let standalone: Vec<&FnInfo> = functions.iter().filter(|f| f.class_name.is_none()).collect();
    let classes: std::collections::BTreeMap<&str, Vec<&FnInfo>> = functions
        .iter()
        .filter_map(|f| f.class_name.as_deref().map(|c| (c, f)))
        .fold(std::collections::BTreeMap::new(), |mut acc, (c, f)| {
            acc.entry(c).or_default().push(f);
            acc
        });

    if !standalone.is_empty() {
        let names: Vec<&str> = standalone.iter().map(|f| f.name.as_str()).collect();
        content.push_str(&format!("from {} import {}\n", module_path, names.join(", ")));
    }
    for cls in classes.keys() {
        content.push_str(&format!("from {} import {}\n", module_path, cls));
    }

    content.push('\n');

    // Standalone function tests.
    for f in &standalone {
        let async_kw = if f.is_async { "async " } else { "" };
        let await_kw = if f.is_async { "await " } else { "" };
        let decorator = if f.is_async { "@pytest.mark.asyncio\n" } else { "" };
        let param_stubs = generate_py_param_stubs(&f.params);

        let call_args = if param_stubs.is_empty() {
            "()".to_string()
        } else {
            format!("({})", param_stubs)
        };
        content.push_str(&format!(
            "{}{}def test_{}():\n    result = {}{}{}\n    assert result is not None\n\n\n",
            decorator,
            async_kw,
            f.name,
            await_kw,
            f.name,
            call_args,
        ));
    }

    // Class method tests.
    for (cls, methods) in &classes {
        // Find __init__ params for constructor stub.
        let init_params: Vec<&ParamInfo> = methods
            .iter()
            .find(|f| f.name == "__init__")
            .map(|f| f.params.iter().filter(|p| p.name != "self" && p.name != "cls").collect())
            .unwrap_or_default();

        let ctor_args = if init_params.is_empty() {
            String::new()
        } else {
            init_params
                .iter()
                .map(|p| {
                    // Create a MagicMock for object-like params, simple values for primitives.
                    if p.type_str.contains("str") || p.type_str.contains("String") {
                        format!("\"test_{}\"", p.name)
                    } else if p.type_str.contains("int") || p.type_str.contains("Int") {
                        "0".to_string()
                    } else if p.type_str.contains("bool") || p.type_str.contains("Bool") {
                        "False".to_string()
                    } else {
                        format!("mock_{}", p.name)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        // Check if we need MagicMock.
        let needs_mock = ctor_args.contains("mock_");

        content.push_str(&format!("class Test{}:\n", cls));

        // Setup method if mocks needed.
        if needs_mock {
            content.push_str("    def setup_method(self):\n");
            for p in &init_params {
                if !p.type_str.contains("str") && !p.type_str.contains("int") && !p.type_str.contains("bool")
                    && !p.type_str.contains("String") && !p.type_str.contains("Int") && !p.type_str.contains("Bool") {
                    content.push_str(&format!("        self.mock_{} = MagicMock()\n", p.name));
                    // Add async return values for common db methods.
                    content.push_str(&format!("        self.mock_{name}.fetch_one = AsyncMock(return_value=None)\n", name=p.name));
                    content.push_str(&format!("        self.mock_{name}.fetch_all = AsyncMock(return_value=[])\n", name=p.name));
                    content.push_str(&format!("        self.mock_{name}.execute = AsyncMock(return_value=MagicMock(rowcount=0))\n", name=p.name));
                }
            }
            let self_ctor_args = init_params
                .iter()
                .map(|p| {
                    if p.type_str.contains("str") || p.type_str.contains("String") {
                        format!("\"test_{}\"", p.name)
                    } else if p.type_str.contains("int") || p.type_str.contains("Int") {
                        "0".to_string()
                    } else if p.type_str.contains("bool") || p.type_str.contains("Bool") {
                        "False".to_string()
                    } else {
                        format!("self.mock_{}", p.name)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            content.push_str(&format!("        self.instance = {}({})\n\n", cls, self_ctor_args));
        }

        for f in methods {
            if f.name == "__init__" { continue; }
            let async_kw = if f.is_async { "async " } else { "" };
            let await_kw = if f.is_async { "await " } else { "" };
            let decorator = if f.is_async { "    @pytest.mark.asyncio\n" } else { "" };
            let param_stubs = generate_py_param_stubs(&f.params);

            if needs_mock {
                content.push_str(&format!(
                    "{}    {}def test_{}(self):\n        result = {}self.instance.{}({})\n        assert result is not None\n\n",
                    decorator, async_kw, f.name, await_kw, f.name, param_stubs,
                ));
            } else {
                content.push_str(&format!(
                    "{}    {}def test_{}(self):\n        instance = {}({})\n        result = {}instance.{}({})\n        assert result is not None\n\n",
                    decorator, async_kw, f.name, cls, ctor_args, await_kw, f.name, param_stubs,
                ));
            }
        }
        content.push('\n');
    }

    // Add mock imports at top if needed.
    if content.contains("MagicMock") {
        content = format!("from unittest.mock import MagicMock, AsyncMock\n{}", content);
    }

    (test_path, content)
}

fn generate_py_param_stubs(params: &[ParamInfo]) -> String {
    params
        .iter()
        .filter(|p| p.name != "self" && p.name != "cls")
        .map(|p| {
            if p.type_str.contains("String") || p.type_str.contains("str") {
                format!("\"test_{}\"", p.name)
            } else if p.type_str.contains("Int") || p.type_str.contains("int") {
                "0".to_string()
            } else if p.type_str.contains("Float") || p.type_str.contains("float") {
                "0.0".to_string()
            } else if p.type_str.contains("Bool") || p.type_str.contains("bool") {
                "False".to_string()
            } else {
                "None".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ─── Rust test generation ───────────────────────────────────────

fn generate_rs_tests(source_file: &str, functions: &[FnInfo]) -> (String, String) {
    // Rust tests go in the same file as a #[cfg(test)] module,
    // but we'll create a separate test file to avoid modifying source.
    let source_stem = Path::new(source_file)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    let test_dir = Path::new(source_file).parent().unwrap_or(Path::new("."));
    let test_path = format!(
        "{}/tests_{}.rs",
        test_dir.to_string_lossy().replace('\\', "/"),
        source_stem
    );

    let mut content = String::from("// Auto-generated by omnilens fix\n\n");

    for f in functions {
        let async_kw = if f.is_async { "#[tokio::test]\nasync " } else { "#[test]\n" };
        let fn_call = if f.is_async {
            format!("{}({}).await", f.name, generate_rs_param_stubs(&f.params))
        } else {
            format!("{}({})", f.name, generate_rs_param_stubs(&f.params))
        };

        content.push_str(&format!(
            "{}fn test_{}() {{\n    let result = {};\n    // TODO: add assertions\n}}\n\n",
            async_kw,
            f.name,
            fn_call,
        ));
    }

    (test_path, content)
}

fn generate_rs_param_stubs(params: &[ParamInfo]) -> String {
    params
        .iter()
        .filter(|p| p.name != "self")
        .map(|p| {
            if p.type_str.contains("String") || p.type_str.contains("str") {
                format!("\"test_{}\"", p.name)
            } else if p.type_str.contains("Int") || p.type_str.contains("u32") || p.type_str.contains("i32") || p.type_str.contains("usize") {
                "0".to_string()
            } else if p.type_str.contains("Bool") || p.type_str.contains("bool") {
                "false".to_string()
            } else {
                format!("todo!(/* {} */)", p.name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ─── Test runner detection ──────────────────────────────────────

fn detect_ts_runner(cwd: &Path) -> Option<(String, Vec<String>)> {
    let pkg_json = cwd.join("package.json");
    if pkg_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&pkg_json) {
            if content.contains("vitest") {
                return Some(("npx".into(), vec!["vitest".into(), "run".into()]));
            }
            if content.contains("jest") {
                return Some(("npx".into(), vec!["jest".into(), "--passWithNoTests".into()]));
            }
        }
    }

    if cwd.join("node_modules/.bin/vitest").exists()
        || cwd.join("node_modules/.bin/vitest.cmd").exists()
    {
        return Some(("npx".into(), vec!["vitest".into(), "run".into()]));
    }
    if cwd.join("node_modules/.bin/jest").exists()
        || cwd.join("node_modules/.bin/jest.cmd").exists()
    {
        return Some(("npx".into(), vec!["jest".into(), "--passWithNoTests".into()]));
    }

    // Fallback: try npx vitest.
    Some(("npx".into(), vec!["vitest".into(), "run".into()]))
}

fn detect_py_runner(cwd: &Path) -> Option<(String, Vec<String>)> {
    let pytest_exists = std::process::Command::new("pytest")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if pytest_exists {
        return Some(("pytest".into(), vec!["-v".into(), "--tb=short".into(), "--rootdir=.".into()]));
    }

    let python_cmd = if cfg!(windows) { "python" } else { "python3" };
    let python_pytest = std::process::Command::new(python_cmd)
        .args(["-m", "pytest", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if python_pytest {
        return Some((python_cmd.into(), vec!["-m".into(), "pytest".into(), "-v".into(), "--tb=short".into(), "--rootdir=.".into()]));
    }

    None
}
