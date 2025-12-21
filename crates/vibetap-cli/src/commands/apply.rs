use clap::Args;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

use super::generate::{compute_hash, load_suggestions, SavedSuggestions};

#[derive(Args)]
pub struct ApplyArgs {
    /// Suggestion(s) to apply: numbers (1 2 3), ranges (1-3), or "all"
    #[arg()]
    selections: Vec<String>,

    /// Skip confirmation prompt
    #[arg(short, long)]
    yes: bool,

    /// Force apply even if source files have changed
    #[arg(short, long)]
    force: bool,
}

/// Record of an applied suggestion for revert tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedRecord {
    pub suggestion_id: String,
    pub file_path: String,
    pub created_file: bool,
    pub original_content: Option<String>,
    pub applied_at: i64,
}

/// History of applied suggestions
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ApplyHistory {
    pub records: Vec<AppliedRecord>,
}

pub async fn execute(args: ApplyArgs) -> anyhow::Result<()> {
    // Load the last suggestions
    let saved = load_suggestions()?;
    let response = &saved.response;

    if response.suggestions.is_empty() {
        println!("{}", "No suggestions to apply.".yellow());
        return Ok(());
    }

    // Check if source files have changed since suggestions were generated
    if !args.force && !saved.source_files.is_empty() {
        let changed_files = check_file_changes(&saved);
        if !changed_files.is_empty() {
            println!("\n{}", "⚠ Source files have changed since suggestions were generated:".yellow().bold());
            for file in &changed_files {
                println!("  {} {}", "•".yellow(), file);
            }
            println!();
            println!("{}", "The suggestions may be outdated or cause conflicts.".dimmed());
            println!("Options:");
            println!("  {} - Re-generate with current changes", "vibetap generate".cyan());
            println!("  {} - Apply anyway", "vibetap apply --force".cyan());

            if !args.yes {
                print!("\n{} ", "Apply anyway? [y/N]:".yellow());
                io::stdout().flush()?;

                let mut confirm = String::new();
                io::stdin().read_line(&mut confirm)?;

                if !confirm.trim().eq_ignore_ascii_case("y") {
                    println!("{}", "Cancelled. Run 'vibetap generate' to regenerate.".dimmed());
                    return Ok(());
                }
            } else {
                println!("{}", "Use --force to bypass this check.".dimmed());
                return Ok(());
            }
        }
    }

    let max = response.suggestions.len();

    // Determine which suggestions to apply
    let to_apply: Vec<usize> = if args.selections.is_empty() {
        // Interactive mode - show list and prompt
        println!("\n{}", "Available suggestions:".bold());
        for (i, suggestion) in response.suggestions.iter().enumerate() {
            println!(
                "  {} {} ({})",
                format!("{}.", i + 1).bold(),
                suggestion.file_path.cyan(),
                suggestion.category.dimmed()
            );
        }
        println!();

        print!("Enter suggestion number(s) to apply (e.g., 1 or 1,2,3 or all): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        parse_selections(&[input.trim().to_string()], max)?
    } else {
        parse_selections(&args.selections, max)?
    };

    if to_apply.is_empty() {
        println!("{}", "No suggestions selected.".yellow());
        return Ok(());
    }

    // Show preview and confirm
    for &idx in &to_apply {
        let suggestion = &response.suggestions[idx];
        println!("\n{}", format!("─── {} ───", suggestion.file_path).bold());
        println!("{}", suggestion.description.dimmed());
        println!();
        print_code_block(&suggestion.code, &suggestion.file_path);
    }

    if !args.yes {
        print!(
            "\n{} ",
            format!("Apply {} suggestion(s)? [y/N]:", to_apply.len()).yellow()
        );
        io::stdout().flush()?;

        let mut confirm = String::new();
        io::stdin().read_line(&mut confirm)?;

        if !confirm.trim().eq_ignore_ascii_case("y") {
            println!("{}", "Cancelled.".dimmed());
            return Ok(());
        }
    }

    // Apply the suggestions
    let mut history = load_history()?;
    let mut applied_count = 0;

    for &idx in &to_apply {
        let suggestion = &response.suggestions[idx];
        let file_path = Path::new(&suggestion.file_path);

        // Track if file existed before
        let (created_file, original_content) = if file_path.exists() {
            (false, Some(std::fs::read_to_string(file_path)?))
        } else {
            // Create parent directories if needed
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            (true, None)
        };

        // Write the test file
        std::fs::write(file_path, &suggestion.code)?;

        // Record in history
        history.records.push(AppliedRecord {
            suggestion_id: suggestion.id.clone(),
            file_path: suggestion.file_path.clone(),
            created_file,
            original_content,
            applied_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
        });

        println!(
            "  {} {}",
            "✓".green(),
            suggestion.file_path
        );
        applied_count += 1;
    }

    // Save history
    save_history(&history)?;

    println!(
        "\n{}",
        format!("Applied {} suggestion(s)!", applied_count).green().bold()
    );
    println!("\nRun {} to execute the generated tests.", "vibetap run".cyan());
    println!(
        "Run {} to undo if needed.",
        "vibetap revert".cyan()
    );

    Ok(())
}

/// Check which source files have changed since suggestions were generated
fn check_file_changes(saved: &SavedSuggestions) -> Vec<String> {
    let mut changed = Vec::new();

    for (path, old_hash) in &saved.source_files {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let current_hash = compute_hash(&content);
                if &current_hash != old_hash {
                    changed.push(path.clone());
                }
            }
            Err(_) => {
                // File no longer exists or can't be read
                changed.push(format!("{} (deleted or unreadable)", path));
            }
        }
    }

    changed
}

fn parse_selections(inputs: &[String], max: usize) -> anyhow::Result<Vec<usize>> {
    let mut result = Vec::new();

    for input in inputs {
        // Check for "all" keyword
        if input.eq_ignore_ascii_case("all") {
            return Ok((0..max).collect());
        }

        // Handle comma-separated or space-separated within a single arg
        for part in input.split([',', ' ']) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            // Check for "all" within the string
            if part.eq_ignore_ascii_case("all") {
                return Ok((0..max).collect());
            }

            if let Some((start, end)) = part.split_once('-') {
                // Range like "1-3"
                let start: usize = start.trim().parse().map_err(|_| {
                    anyhow::anyhow!("Invalid number in range: {}", part)
                })?;
                let end: usize = end.trim().parse().map_err(|_| {
                    anyhow::anyhow!("Invalid number in range: {}", part)
                })?;
                if start == 0 || end == 0 || start > max || end > max {
                    anyhow::bail!("Invalid range: {}. Choose 1-{}.", part, max);
                }
                for i in start..=end {
                    result.push(i - 1);
                }
            } else {
                // Single number
                let num: usize = part.parse().map_err(|_| {
                    anyhow::anyhow!("Invalid selection: '{}'. Use numbers, ranges (1-3), or 'all'.", part)
                })?;
                if num == 0 || num > max {
                    anyhow::bail!("Invalid number: {}. Choose 1-{}.", num, max);
                }
                result.push(num - 1);
            }
        }
    }

    // Remove duplicates
    result.sort();
    result.dedup();

    Ok(result)
}

fn print_code_block(code: &str, file_path: &str) {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];

    let extension = file_path.rsplit('.').next().unwrap_or("ts");
    let syntax = ps
        .find_syntax_by_extension(extension)
        .or_else(|| ps.find_syntax_by_extension("ts"))
        .unwrap_or_else(|| ps.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, theme);

    println!("   {}", "┌─".dimmed());
    for line in LinesWithEndings::from(code) {
        let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &ps).unwrap();
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        let escaped = escaped.trim_end_matches('\n');
        println!("   {}  {}", "│".dimmed(), escaped);
    }
    println!("   {}\x1b[0m", "└─".dimmed());
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
