use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(
    name = "omnilens",
    about = "AI-native code verification engine",
    long_about = "omnilens — Understand, verify, and test code at the speed AI generates it.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Output format
    #[arg(long, default_value = "text", global = true)]
    format: OutputFormat,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize omnilens in the current project.
    Init,

    /// Build or update the semantic index.
    Index,

    /// Analyze the impact of a code change.
    Impact {
        /// File or function to analyze.
        target: String,
        /// Function name within the file.
        #[arg(long)]
        r#fn: Option<String>,
        /// Maximum traversal depth.
        #[arg(long, default_value = "5")]
        depth: usize,
    },

    /// Verify changes against invariants and contracts.
    Verify {
        /// Git diff spec (e.g., "HEAD~1", "main..feature").
        #[arg(long)]
        diff: Option<String>,
        /// Verify specific files.
        #[arg(long)]
        files: Vec<String>,
    },

    /// Run an OmniQL query.
    Query {
        /// The OmniQL query string.
        query: String,
    },

    /// Generate tests for uncovered critical paths.
    Testgen {
        /// Target file or function.
        target: String,
        /// Test generation strategy.
        #[arg(long, default_value = "boundary")]
        strategy: String,
    },

    /// Attach runtime profiler to a running process.
    Trace {
        /// Process ID to attach to.
        #[arg(long)]
        attach: Option<u32>,
    },

    /// Export the semantic graph.
    Graph {
        /// Output format: dot, json, html.
        #[arg(long, default_value = "json")]
        output: String,
    },

    /// Start the LSP server for IDE integration.
    Serve,

    /// Show discovered invariants and contracts.
    Invariants,
}

#[derive(Clone, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Sarif,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing based on verbosity.
    let filter = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    match cli.command {
        Command::Init => commands::init::run(),
        Command::Index => commands::index::run(),
        Command::Impact { target, r#fn, depth } => {
            commands::impact::run(&target, r#fn.as_deref(), depth)
        }
        Command::Verify { diff, files } => commands::verify::run(diff, files),
        Command::Query { query } => commands::query::run(&query),
        Command::Testgen { target, strategy } => commands::testgen::run(&target, &strategy),
        Command::Trace { attach } => commands::trace::run(attach),
        Command::Graph { output } => commands::graph::run(&output),
        Command::Serve => commands::serve::run(),
        Command::Invariants => commands::invariants::run(),
    }
}
