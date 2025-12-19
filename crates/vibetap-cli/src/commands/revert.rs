use clap::Args;
use colored::Colorize;
use std::io::{self, Write};
use std::path::Path;

use super::apply::{ApplyHistory, AppliedRecord};

#[derive(Args)]
pub struct RevertArgs {
    /// Skip confirmation prompt
    #[arg(short, long)]
    yes: bool,

    /// Revert all applied changes (not just the last batch)
    #[arg(long)]
    all: bool,

    /// Number of applied files to revert (default: last batch)
    #[arg(short, long)]
    count: Option<usize>,
}

pub async fn execute(args: RevertArgs) -> anyhow::Result<()> {
    let mut history = load_history()?;

    if history.records.is_empty() {
        println!("{}", "No applied changes to revert.".yellow());
        return Ok(());
    }

    // Determine what to revert
    let to_revert: Vec<AppliedRecord> = if args.all {
        history.records.drain(..).collect()
    } else if let Some(count) = args.count {
        let count = count.min(history.records.len());
        history
            .records
            .drain(history.records.len() - count..)
            .collect()
    } else {
        // Revert the last batch (same applied_at timestamp)
        if let Some(last) = history.records.last() {
            let last_timestamp = last.applied_at;
            let drain_start = history
                .records
                .iter()
                .position(|r| r.applied_at == last_timestamp)
                .unwrap_or(history.records.len() - 1);
            history.records.drain(drain_start..).collect()
        } else {
            Vec::new()
        }
    };

    if to_revert.is_empty() {
        println!("{}", "No changes to revert.".yellow());
        return Ok(());
    }

    // Show what will be reverted
    println!("\n{}", "Files to revert:".bold());
    for record in &to_revert {
        let action = if record.created_file {
            "delete".red()
        } else {
            "restore".yellow()
        };
        println!("  {} {} ({})", "•".dimmed(), record.file_path, action);
    }

    if !args.yes {
        print!(
            "\n{} ",
            format!("Revert {} file(s)? [y/N]:", to_revert.len()).yellow()
        );
        io::stdout().flush()?;

        let mut confirm = String::new();
        io::stdin().read_line(&mut confirm)?;

        if !confirm.trim().eq_ignore_ascii_case("y") {
            // Put the records back
            let mut new_history = load_history()?;
            new_history.records.extend(to_revert);
            save_history(&new_history)?;
            println!("{}", "Cancelled.".dimmed());
            return Ok(());
        }
    }

    // Perform the revert
    let mut reverted_count = 0;
    let mut errors = Vec::new();

    for record in &to_revert {
        let file_path = Path::new(&record.file_path);

        let result = if record.created_file {
            // Delete the created file
            if file_path.exists() {
                std::fs::remove_file(file_path)
            } else {
                Ok(()) // Already gone
            }
        } else {
            // Restore original content
            match &record.original_content {
                Some(content) => std::fs::write(file_path, content),
                None => {
                    // No original content recorded - can't restore
                    errors.push(format!(
                        "{}: no original content recorded",
                        record.file_path
                    ));
                    continue;
                }
            }
        };

        match result {
            Ok(()) => {
                let action = if record.created_file {
                    "deleted"
                } else {
                    "restored"
                };
                println!("  {} {} ({})", "✓".green(), record.file_path, action);
                reverted_count += 1;
            }
            Err(e) => {
                errors.push(format!("{}: {}", record.file_path, e));
            }
        }
    }

    // Save updated history
    save_history(&history)?;

    if !errors.is_empty() {
        println!("\n{}", "Errors:".red().bold());
        for error in &errors {
            println!("  {} {}", "✗".red(), error);
        }
    }

    println!(
        "\n{}",
        format!("Reverted {} file(s).", reverted_count).green().bold()
    );

    if !history.records.is_empty() {
        println!(
            "{} applied change(s) remaining.",
            history.records.len().to_string().dimmed()
        );
    }

    Ok(())
}

fn load_history() -> anyhow::Result<ApplyHistory> {
    let path = Path::new(".vibetap/history.json");
    if !path.exists() {
        return Ok(ApplyHistory::default());
    }

    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn save_history(history: &ApplyHistory) -> anyhow::Result<()> {
    let vibetap_dir = Path::new(".vibetap");
    if !vibetap_dir.exists() {
        std::fs::create_dir_all(vibetap_dir)?;
    }

    let path = vibetap_dir.join("history.json");
    let json = serde_json::to_string_pretty(history)?;
    std::fs::write(path, json)?;

    Ok(())
}
