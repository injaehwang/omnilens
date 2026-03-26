use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(
    name = "omnilens",
    about = "AI-native code verification engine",
    long_about = "omnilens — Your code, verified.\n\nRun `omnilens check` to scan your changes.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Output format
    #[arg(long, default_value = "text", global = true)]
    format: OutputFormat,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

#[derive(Subcommand)]
enum Command {
    /// Scan your code for problems. This is the main command.
    Check {
        /// Only check specific files.
        files: Vec<String>,
    },

    /// Auto-fix problems found by check.
    Fix {
        /// Only fix specific files.
        files: Vec<String>,
    },

    /// Show project health dashboard.
    Status,

    /// Install git hooks for automatic checking.
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },

    /// Run in CI mode (auto-detects platform).
    Ci {
        #[arg(long)]
        platform: Option<String>,
        #[arg(long, default_value = "error")]
        fail_on: String,
    },

    // ─── Advanced (AI agents & power users) ─────────────────
    /// [advanced] Verify changes against a git ref.
    Verify {
        #[arg(long)]
        diff: Option<String>,
        #[arg(long)]
        files: Vec<String>,
    },

    /// [advanced] Run an OmniQL query.
    Query {
        query: String,
    },

    /// [advanced] Analyze impact of a function change.
    Impact {
        target: String,
        #[arg(long)]
        r#fn: Option<String>,
        #[arg(long, default_value = "5")]
        depth: usize,
    },

    /// [advanced] Show discovered invariants.
    Invariants,

    /// [advanced] Build/update semantic index.
    Index,

    /// [advanced] Initialize omnilens in current project.
    Init,

    /// [advanced] Start LSP server.
    Serve,

    /// [advanced] Export semantic graph.
    Graph {
        #[arg(long, default_value = "json")]
        output: String,
    },

    /// [advanced] Generate tests.
    Testgen {
        target: String,
        #[arg(long, default_value = "boundary")]
        strategy: String,
    },

    /// [advanced] Runtime profiler.
    Trace {
        #[arg(long)]
        attach: Option<u32>,
    },
}

#[derive(Subcommand)]
pub enum HookAction {
    Install,
    Uninstall,
    Status,
}

#[derive(Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    Sarif,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    // No subcommand = `omnilens check`
    let command = cli.command.unwrap_or(Command::Check { files: vec![] });

    match command {
        // ─── Developer commands ──────────────────────────────
        Command::Check { files } => commands::check::run(files, &cli.format),
        Command::Fix { files } => commands::fix::run(files),
        Command::Status => commands::status::run(),
        Command::Hook { action } => commands::hook::run(action),
        Command::Ci { platform, fail_on } => {
            commands::ci::run(platform.as_deref(), &fail_on, &cli.format)
        }

        // ─── Advanced commands ───────────────────────────────
        Command::Init => commands::init::run(),
        Command::Index => commands::index::run(),
        Command::Impact { target, r#fn, depth } => {
            commands::impact::run(&target, r#fn.as_deref(), depth)
        }
        Command::Verify { diff, files } => commands::verify::run(diff, files, &cli.format),
        Command::Query { query } => commands::query::run(&query),
        Command::Testgen { target, strategy } => commands::testgen::run(&target, &strategy),
        Command::Trace { attach } => commands::trace::run(attach),
        Command::Graph { output } => commands::graph::run(&output),
        Command::Serve => commands::serve::run(),
        Command::Invariants => commands::invariants::run(),
    }
}
