use clap::Args;
use colored::Colorize;

#[derive(Args)]
pub struct RunArgs {
    /// Run all tests, not just generated ones
    #[arg(long)]
    all: bool,
}

pub async fn execute(args: RunArgs) -> anyhow::Result<()> {
    if args.all {
        println!("{}", "Running all tests...".cyan());
    } else {
        println!("{}", "Running generated tests...".cyan());
    }

    // TODO: Read last applied tests from .aitest/history.json
    // TODO: Execute test runner with specific files

    println!(
        "\n{}",
        "Test execution not yet implemented. Run your test command manually.".yellow()
    );

    Ok(())
}
