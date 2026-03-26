//! `omnilens fix` — auto-generate fixes for problems found by check.
//!
//! Currently generates:
//! - Test files for untested public functions
//! - Error handling wrappers for unsafe calls

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use colored::Colorize;
use omnilens_ir::node::UsirNode;
use omnilens_ir::Visibility;

pub fn run(files: Vec<String>) -> Result<()> {
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
    println!(
        "  {} {} tests generated in {} files",
        "Done.".green().bold(),
        tests_generated,
        files_written,
    );
    println!(
        "  Run your test runner to verify: {}, {}, or {}",
        "vitest".cyan(),
        "pytest".cyan(),
        "cargo test".cyan(),
    );
    println!();

    Ok(())
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
                format!("undefined /* {} */", p.name)
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
        content.push_str(&format!("class Test{}:\n", cls));
        for f in methods {
            if f.name == "__init__" { continue; }
            let async_kw = if f.is_async { "async " } else { "" };
            let await_kw = if f.is_async { "await " } else { "" };
            let decorator = if f.is_async { "    @pytest.mark.asyncio\n" } else { "" };
            let param_stubs = generate_py_param_stubs(&f.params);

            content.push_str(&format!(
                "{}    {}def test_{}(self):\n        instance = {}(TODO)  # provide constructor args\n        result = {}instance.{}({})\n        assert result is not None\n\n",
                decorator,
                async_kw,
                f.name,
                cls,
                await_kw,
                f.name,
                param_stubs,
            ));
        }
        content.push('\n');
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
                format!("None  # {}", p.name)
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
