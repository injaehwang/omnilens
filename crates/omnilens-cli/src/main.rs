use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(
    name = "omnilens",
    about = "AI-native code verification engine",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, default_value = "text", global = true)]
    format: OutputFormat,

    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

#[derive(Subcommand)]
enum Command {
    // ─── AI-internal commands (AI calls these, not developers) ───

    /// Verify changes against a git ref.
    Verify {
        #[arg(long)]
        diff: Option<String>,
        #[arg(long)]
        files: Vec<String>,
    },
    /// Run an OmniQL query.
    Query { query: String },
    /// Analyze impact of a function change.
    Impact {
        target: String,
        #[arg(long)]
        r#fn: Option<String>,
        #[arg(long, default_value = "5")]
        depth: usize,
    },
    /// Generate tests and run them.
    Fix {
        files: Vec<String>,
        #[arg(long)]
        auto: bool,
        #[arg(long, default_value = "3")]
        max_retries: u32,
    },
    /// Show discovered invariants.
    Invariants,
    /// Build/update semantic index.
    Index,
    /// Install git hooks.
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
    /// Run in CI mode.
    Ci {
        #[arg(long)]
        platform: Option<String>,
        #[arg(long, default_value = "error")]
        fail_on: String,
    },
    /// Project health dashboard.
    Status,
    /// Scan for problems.
    Check { files: Vec<String> },
    /// Start LSP server.
    Serve,
    /// Export graph.
    Graph {
        #[arg(long, default_value = "json")]
        output: String,
    },
    /// Generate tests.
    Testgen {
        target: String,
        #[arg(long, default_value = "boundary")]
        strategy: String,
    },
    /// Runtime profiler.
    Trace {
        #[arg(long)]
        attach: Option<u32>,
    },
    /// Initialize omnilens.
    Init,
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

    // No subcommand = the main experience: analyze and output snapshot.
    let command = match cli.command {
        Some(cmd) => cmd,
        None => return commands::analyze::run(),
    };

    match command {
        Command::Check { files } => commands::check::run(files, &cli.format),
        Command::Fix { files, auto, max_retries } => commands::fix::run(files, auto, max_retries),
        Command::Status => commands::status::run(),
        Command::Hook { action } => commands::hook::run(action),
        Command::Ci { platform, fail_on } => commands::ci::run(platform.as_deref(), &fail_on, &cli.format),
        Command::Init => commands::init::run(),
        Command::Index => commands::index::run(),
        Command::Impact { target, r#fn, depth } => commands::impact::run(&target, r#fn.as_deref(), depth),
        Command::Verify { diff, files } => commands::verify::run(diff, files, &cli.format),
        Command::Query { query } => commands::query::run(&query),
        Command::Testgen { target, strategy } => commands::testgen::run(&target, &strategy),
        Command::Trace { attach } => commands::trace::run(attach),
        Command::Graph { output } => commands::graph::run(&output),
        Command::Serve => commands::serve::run(),
        Command::Invariants => commands::invariants::run(),
    }
}
