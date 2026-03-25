mod commands;

use clap::{Parser, Subcommand, ValueEnum};
use lore_core::types::OutputFormat;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "lore", version, about = "Semantic git intelligence for LLM agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format
    #[arg(long, global = true, default_value = "text", value_enum)]
    format: CliFormat,

    /// Maximum response tokens
    #[arg(long, global = true)]
    max_tokens: Option<usize>,

    /// Suppress all non-result output (for hooks)
    #[arg(long, global = true)]
    silent: bool,
}

#[derive(Clone, ValueEnum)]
enum CliFormat {
    Text,
    Xml,
    Json,
}

impl From<CliFormat> for OutputFormat {
    fn from(f: CliFormat) -> OutputFormat {
        match f {
            CliFormat::Text => OutputFormat::Text,
            CliFormat::Xml => OutputFormat::Xml,
            CliFormat::Json => OutputFormat::Json,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize lore for this repository
    Init(commands::init::InitArgs),

    /// Search git history semantically
    Why(commands::why::WhyArgs),

    /// Run health checks and optionally fix issues
    Doctor {
        /// Auto-fix all detected issues
        #[arg(long)]
        fix_all: bool,
    },

    /// Start the MCP server (stdio JSON-RPC)
    Serve,
}

#[tokio::main]
async fn main() {
    // Initialize tracing from LORE_LOG env var
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("LORE_LOG")
                .unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let format: OutputFormat = cli.format.into();

    let result = match cli.command {
        Commands::Init(args) => commands::init::run(args).await,
        Commands::Why(args) => commands::why::run(args, format, cli.max_tokens).await,
        Commands::Doctor { fix_all } => commands::doctor::run(fix_all).await,
        Commands::Serve => commands::serve::run().await,
    };

    if let Err(e) = result {
        if !cli.silent {
            eprintln!("Error: {}", e);
        }
        std::process::exit(1);
    }
}
