use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod commands;

#[derive(Parser)]
#[command(name = "vibetap")]
#[command(author, version, about = "AI-powered test generation from code changes", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize VibeTap in the current repository
    Init(commands::init::InitArgs),

    /// Watch for staged changes and suggest tests
    Watch(commands::watch::WatchArgs),

    /// Generate tests immediately for current changes
    Now(commands::now::NowArgs),

    /// Apply a suggestion or the latest suggestion set
    Apply(commands::apply::ApplyArgs),

    /// Revert the last applied patch
    Revert(commands::revert::RevertArgs),

    /// Silence suggestions for a period
    Hush(commands::hush::HushArgs),

    /// Run the generated tests
    Run(commands::run::RunArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vibetap=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    if cli.verbose {
        tracing::info!("Verbose mode enabled");
    }

    match cli.command {
        Commands::Init(args) => commands::init::execute(args).await,
        Commands::Watch(args) => commands::watch::execute(args).await,
        Commands::Now(args) => commands::now::execute(args).await,
        Commands::Apply(args) => commands::apply::execute(args).await,
        Commands::Revert(args) => commands::revert::execute(args).await,
        Commands::Hush(args) => commands::hush::execute(args).await,
        Commands::Run(args) => commands::run::execute(args).await,
    }
}
