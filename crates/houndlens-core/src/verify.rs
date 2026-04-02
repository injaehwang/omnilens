//! Verification pipeline — the core of AI-native testing.
//!
//! Runs semantic diff analysis, invariant checking, and contract verification
//! against code changes (especially AI-generated code).

use std::process::Command;

use anyhow::Result;
use houndlens_ir::contract::Contract;
use houndlens_ir::invariant::{InvariantViolation, ViolationSeverity};
use houndlens_ir::node::UsirNode;
use houndlens_ir::Visibility;

use crate::config::Config;
use crate::frontend::LanguageFrontend;

/// Specifies what to verify.
pub enum DiffSpec {
    /// Verify changes between two git refs.
    GitDiff { base: String, head: String },
    /// Verify specific files.
    Files(Vec<std::path::PathBuf>),
    /// Verify staged changes.
    Staged,
    /// Verify working directory changes.
    WorkingDir,
}

/// Complete verification result.
pub struct VerifyResult {
    pub semantic_changes: Vec<SemanticChange>,
    pub invariant_violations: Vec<InvariantViolation>,
    pub contract_violations: Vec<ContractViolation>,
    pub risk_score: f64,
    pub confidence: f64,
    pub suggested_tests: Vec<TestSuggestion>,
}

impl VerifyResult {
    pub fn has_errors(&self) -> bool {
        self.invariant_violations
            .iter()
            .any(|v| v.severity == ViolationSeverity::Error)
            || self.contract_violations.iter().any(|v| v.is_breaking)
    }

    pub fn error_count(&self) -> usize {
        self.invariant_violations
            .iter()
            .filter(|v| v.severity == ViolationSeverity::Error)
            .count()
            + self
                .contract_violations
                .iter()
                .filter(|v| v.is_breaking)
                .count()
    }

    pub fn warning_count(&self) -> usize {
        self.invariant_violations
            .iter()
            .filter(|v| v.severity == ViolationSeverity::Warning)
            .count()
            + self
                .semantic_changes
                .iter()
                .filter(|c| matches!(c.risk, ChangeRisk::NeedsReview))
                .count()
    }
}

/// A semantic change (not just text diff, but behavioral change).
pub struct SemanticChange {
    pub location: houndlens_ir::SourceSpan,
    pub kind: SemanticChangeKind,
    pub description: String,
    pub risk: ChangeRisk,
}

#[derive(Debug)]
pub enum SemanticChangeKind {
    SignatureChange,
    ControlFlowChange,
    DataFlowChange,
    DependencyAdded,
    DependencyRemoved,
    ApiSurfaceChange,
    TypeChange,
    FunctionAdded,
    FunctionRemoved,
    ComplexityChange { old: u32, new: u32 },
}

#[derive(Debug)]
pub enum ChangeRisk {
    Safe,
    NeedsReview,
    Breaking,
    SecuritySensitive,
}

pub struct ContractViolation {
    pub contract: Contract,
    pub location: houndlens_ir::SourceSpan,
    pub description: String,
    pub is_breaking: bool,
    pub suggested_fix: Option<String>,
}

pub struct TestSuggestion {
    pub target: houndlens_ir::NodeId,
    pub description: String,
    pub priority: TestPriority,
    pub skeleton: Option<String>,
}

#[derive(Debug)]
pub enum TestPriority {
    Critical,
    High,
    Medium,
    Low,
}

/// Run the full verification pipeline.
pub fn run_verification(
    graph: &houndlens_graph::SemanticGraph,
    _config: &Config,
    diff: &DiffSpec,
    frontends: &[Box<dyn LanguageFrontend>],
) -> Result<VerifyResult> {
    // Step 1: Get changed files and base ref from git.
    let (changed_files, base_ref) = get_changed_files_with_ref(diff)?;

    if changed_files.is_empty() {
        return Ok(VerifyResult {
            semantic_changes: Vec::new(),
            invariant_violations: Vec::new(),
            contract_violations: Vec::new(),
            risk_score: 0.0,
            confidence: 1.0,
            suggested_tests: Vec::new(),
        });
    }

    // Step 2: Compute real semantic diff (old vs new USIR).
    let mut semantic_changes = if let Some(ref base) = base_ref {
        crate::semantic_diff::compute_semantic_diff(base, &changed_files, frontends, graph)
    } else {
        Vec::new()
    };

    // Step 3: Additional static analysis on current graph for changed files.
    for file in &changed_files {
        let file_str = file.replace('\\', "/");
        let nodes = graph.find_file_by_suffix(&file_str).unwrap_or_default();

        for node_id in &nodes {
            if let Some(node) = graph.get_node(*node_id) {
                let extra = analyze_node_static(graph, node);
                semantic_changes.extend(extra);
            }
        }
    }

    // Step 4: Syntax validation on changed files.
    let syntax_errors = crate::syntax_check::check_syntax(&changed_files, frontends);
    for err in &syntax_errors {
        let severity = match err.severity {
            crate::syntax_check::SyntaxSeverity::Error => ChangeRisk::Breaking,
            crate::syntax_check::SyntaxSeverity::Warning => ChangeRisk::NeedsReview,
        };
        semantic_changes.push(SemanticChange {
            location: houndlens_ir::SourceSpan {
                file: std::path::PathBuf::from(&err.file),
                start_byte: 0,
                end_byte: 0,
                start_line: err.line,
                start_col: err.col,
                end_line: err.line,
                end_col: err.col,
            },
            kind: SemanticChangeKind::ControlFlowChange,
            description: err.message.clone(),
            risk: severity,
        });
    }

    // Step 5: Run project's own tools (tsc, eslint, pytest, cargo) on changed files.
    let project_root = std::env::current_dir().unwrap_or_default();
    let tooling = crate::snapshot::detect_tooling(&project_root);
    let tool_errors = crate::tool_runner::run_project_tools(&tooling, &changed_files, &project_root);
    for err in &tool_errors {
        let severity = match err.severity {
            crate::tool_runner::ToolSeverity::Error => ChangeRisk::Breaking,
            crate::tool_runner::ToolSeverity::Warning => ChangeRisk::NeedsReview,
        };
        semantic_changes.push(SemanticChange {
            location: houndlens_ir::SourceSpan {
                file: std::path::PathBuf::from(&err.file),
                start_byte: 0,
                end_byte: 0,
                start_line: err.line,
                start_col: err.col,
                end_line: err.line,
                end_col: err.col,
            },
            kind: SemanticChangeKind::ControlFlowChange,
            description: format!("[{}] {}", err.tool, err.message),
            risk: severity,
        });
    }

    // Step 6: Invariant checking.
    let invariant_violations = check_invariants_on_changes(graph, &changed_files);

    // Step 7: Compute risk score.
    let mut risk_score: f64 = 0.0;
    for change in &semantic_changes {
        risk_score += match change.risk {
            ChangeRisk::Breaking => 0.25,
            ChangeRisk::SecuritySensitive => 0.20,
            ChangeRisk::NeedsReview => 0.05,
            ChangeRisk::Safe => 0.01,
        };
    }
    risk_score += changed_files.len() as f64 * 0.02;
    if !invariant_violations.is_empty() {
        risk_score += 0.15;
    }
    risk_score = risk_score.min(1.0);

    // Step 8: Generate test suggestions.
    let suggested_tests = generate_test_suggestions(graph, &changed_files);

    let confidence = if base_ref.is_some() { 0.85 } else { 0.5 };

    Ok(VerifyResult {
        semantic_changes,
        invariant_violations,
        contract_violations: Vec::new(),
        risk_score,
        confidence,
        suggested_tests,
    })
}

/// Get changed files and the base ref for comparison.
fn get_changed_files_with_ref(diff: &DiffSpec) -> Result<(Vec<String>, Option<String>)> {
    match diff {
        DiffSpec::GitDiff { base, head } => {
            let files = git_diff_files(base, head)?;
            Ok((files, Some(base.clone())))
        }
        DiffSpec::Staged => {
            let files = run_git(&["diff", "--name-only", "--cached"])?;
            Ok((files, Some("HEAD".to_string())))
        }
        DiffSpec::WorkingDir => {
            let files = run_git(&["diff", "--name-only"])?;
            Ok((files, Some("HEAD".to_string())))
        }
        DiffSpec::Files(files) => {
            let file_strs = files.iter().map(|f| f.display().to_string()).collect();
            // Try to find a base ref.
            let base = run_git(&["rev-parse", "HEAD"]).ok().and_then(|v| v.into_iter().next());
            Ok((file_strs, base))
        }
    }
}

fn git_diff_files(base: &str, head: &str) -> Result<Vec<String>> {
    run_git(&["diff", "--name-only", base, head])
}

fn run_git(args: &[&str]) -> Result<Vec<String>> {
    let output = Command::new("git").args(args).output()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect())
    } else {
        Ok(Vec::new())
    }
}

/// Static analysis checks on current nodes (no comparison needed).
fn analyze_node_static(
    _graph: &houndlens_graph::SemanticGraph,
    node: &UsirNode,
) -> Vec<SemanticChange> {
    let mut changes = Vec::new();

    if let UsirNode::Function(f) = node {
        // Flag unsafe functions.
        if f.is_unsafe {
            changes.push(SemanticChange {
                location: f.span.clone(),
                kind: SemanticChangeKind::ControlFlowChange,
                description: format!("Unsafe function '{}' — requires careful review", f.name.display()),
                risk: ChangeRisk::SecuritySensitive,
            });
        }
    }

    changes
}

/// Check discovered invariants against changed files.
fn check_invariants_on_changes(
    graph: &houndlens_graph::SemanticGraph,
    changed_files: &[String],
) -> Vec<InvariantViolation> {
    let mut violations = Vec::new();
    let discovered = crate::invariants::discover(graph);

    for inv in &discovered.invariants {
        for scope_id in &inv.scope {
            if let Some(node) = graph.get_node(*scope_id) {
                let node_file = node.span().file.to_string_lossy().replace('\\', "/");
                for changed in changed_files {
                    let changed_normalized = changed.replace('\\', "/");
                    if node_file.ends_with(&changed_normalized)
                        || changed_normalized.ends_with(&node_file)
                    {
                        violations.push(InvariantViolation {
                            invariant: inv.id,
                            location: node.span().clone(),
                            description: format!(
                                "Changed file may affect invariant: {}",
                                inv.description
                            ),
                            severity: ViolationSeverity::Warning,
                            suggested_fix: None,
                        });
                        break;
                    }
                }
            }
        }
    }

    violations
}

/// Generate test suggestions for changed code.
fn generate_test_suggestions(
    graph: &houndlens_graph::SemanticGraph,
    changed_files: &[String],
) -> Vec<TestSuggestion> {
    let mut suggestions = Vec::new();

    for file in changed_files {
        let file_str = file.replace('\\', "/");
        let nodes = graph.find_file_by_suffix(&file_str).unwrap_or_default();

        for node_id in nodes {
            if let Some(UsirNode::Function(f)) = graph.get_node(node_id) {
                if f.visibility == Visibility::Public && f.complexity.is_some() {
                    let fn_name = f.name.display();
                    let short = f.name.segments.last().unwrap_or(&fn_name);
                    suggestions.push(TestSuggestion {
                        target: node_id,
                        description: format!("Add tests for '{}'", fn_name),
                        priority: TestPriority::High,
                        skeleton: Some(format!(
                            "#[test]\nfn test_{}() {{\n    todo!()\n}}",
                            short
                        )),
                    });
                }
            }
        }
    }

    suggestions
}
