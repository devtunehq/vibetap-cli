use clap::Args;
use colored::Colorize;

#[derive(Args)]
pub struct RevertArgs {
    /// Skip confirmation prompt
    #[arg(short, long)]
    yes: bool,
}

pub async fn execute(args: RevertArgs) -> anyhow::Result<()> {
    println!("{}", "Reverting last applied patch...".cyan());

    if !args.yes {
        println!(
            "\n{}",
            "This will remove the last applied test files. Continue? [y/N]".yellow()
        );
        // TODO: Read confirmation
        return Ok(());
    }

    // TODO: Read last applied patch from .aitest/history.json
    // TODO: Delete created files
    // TODO: Restore modified files

    println!("{}", "Last patch reverted successfully!".green());

    Ok(())
}
