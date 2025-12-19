use clap::Args;
use colored::Colorize;
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;

use super::hush::load_state;
use vibetap_core::{
    api::{DiffHunk, DiffPayload, FileContext, GenerateOptions, GenerateRequest},
    ApiClient, Config,
};
use vibetap_git::{get_staged_diff, GitError};

#[derive(Args)]
pub struct WatchArgs {
    /// Debounce time in milliseconds
    #[arg(short, long, default_value = "2000")]
    debounce: u64,

    /// Watch for all uncommitted changes, not just staged
    #[arg(long)]
    uncommitted: bool,

    /// Maximum suggestions per generation
    #[arg(long, default_value = "3")]
    max_suggestions: u32,

    /// Prioritize security tests
    #[arg(long)]
    security: bool,
}

pub async fn execute(args: WatchArgs) -> anyhow::Result<()> {
    // Check hush state
    let hush_state = load_state()?;
    if hush_state.is_hushed() {
        if let Some(remaining) = hush_state.remaining() {
            println!(
                "{}",
                format!("Suggestions are hushed for {}. Use 'vibetap hush --clear' to resume.", remaining)
                    .yellow()
            );
        } else {
            println!(
                "{}",
                "Suggestions are hushed. Use 'vibetap hush --clear' to resume.".yellow()
            );
        }
        return Ok(());
    }

    // Load config
    let mut config = Config::load()?;
    let access_token = config.get_valid_access_token().await?;
    let api_url = config.api_url().to_string();

    println!("{}", "Starting VibeTap watch mode...".cyan().bold());
    println!("  Debounce: {}ms", args.debounce);
    println!("  Mode: {}", if args.uncommitted { "all uncommitted" } else { "staged only" });
    println!();
    println!("{}", "Watching for changes. Press Ctrl+C to stop.".dimmed());
    println!();

    // Get initial diff hash
    let mut last_diff_hash = get_diff_hash(args.uncommitted);

    // Set up file watcher
    let (tx, rx) = channel();
    let debounce_duration = Duration::from_millis(args.debounce);

    let mut debouncer = new_debouncer(debounce_duration, tx)?;

    // Watch current directory recursively
    debouncer
        .watcher()
        .watch(Path::new("."), RecursiveMode::Recursive)?;

    // Main watch loop
    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                // Check hush state each iteration
                let hush_state = load_state()?;
                if hush_state.is_hushed() {
                    continue;
                }

                // Filter out irrelevant events
                let relevant = events.iter().any(|event| {
                    if event.kind == DebouncedEventKind::Any {
                        let path = &event.path;
                        // Ignore .git, .vibetap, node_modules, target, etc.
                        !is_ignored_path(path)
                    } else {
                        false
                    }
                });

                if !relevant {
                    continue;
                }

                // Check if diff has changed
                let new_hash = get_diff_hash(args.uncommitted);
                if new_hash == last_diff_hash {
                    continue;
                }
                last_diff_hash = new_hash;

                // Get the current diff
                let diff = if args.uncommitted {
                    vibetap_git::get_uncommitted_diff()
                } else {
                    get_staged_diff()
                };

                let diff = match diff {
                    Ok(d) => d,
                    Err(GitError::NoStagedChanges) => {
                        println!("{}", "No staged changes.".dimmed());
                        continue;
                    }
                    Err(GitError::NotARepo) => {
                        println!("{}", "Not a git repository.".red());
                        break;
                    }
                    Err(e) => {
                        println!("{} {}", "Git error:".red(), e);
                        continue;
                    }
                };

                if diff.hunks.is_empty() {
                    continue;
                }

                println!(
                    "\n{} {} in {} file(s)",
                    "Changes detected:".cyan(),
                    format!("{} hunk(s)", diff.hunks.len()).green(),
                    diff.files_changed.len()
                );

                // Build and send request
                let request = build_request(&diff, &args, &config);
                let client = ApiClient::new(&api_url, &access_token);

                println!("{}", "Generating suggestions...".dimmed());

                match client.generate(request).await {
                    Ok(response) => {
                        // Save for apply command
                        if let Err(e) = save_suggestions(&response) {
                            eprintln!("{} {}", "Warning:".yellow(), e);
                        }

                        // Display summary
                        println!();
                        if response.suggestions.is_empty() {
                            println!("{}", "No test suggestions for these changes.".dimmed());
                        } else {
                            println!(
                                "{} {}",
                                format!("{} suggestion(s) generated:", response.suggestions.len()).green().bold(),
                                response.model_used.dimmed()
                            );
                            for (i, suggestion) in response.suggestions.iter().enumerate() {
                                println!(
                                    "  {} {} - {}",
                                    format!("{}.", i + 1).bold(),
                                    suggestion.file_path.cyan(),
                                    suggestion.description.dimmed()
                                );
                            }
                            println!();
                            println!(
                                "Run {} to view and apply.",
                                "vibetap apply".cyan()
                            );
                        }
                    }
                    Err(e) => {
                        println!("{} {}", "API error:".red(), e);
                    }
                }

                println!();
                println!("{}", "Watching for changes...".dimmed());
            }
            Ok(Err(e)) => {
                println!("{} {}", "Watch error:".red(), e);
                // Continue watching despite the error
                continue;
            }
            Err(e) => {
                println!("{} {}", "Channel error:".red(), e);
                break;
            }
        }
    }

    Ok(())
}

fn get_diff_hash(uncommitted: bool) -> String {
    let diff = if uncommitted {
        vibetap_git::get_uncommitted_diff()
    } else {
        get_staged_diff()
    };

    match diff {
        Ok(d) => {
            // Create a simple hash from the diff content
            let mut hash = 0u64;
            for hunk in &d.hunks {
                for byte in hunk.content.bytes() {
                    hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
                }
            }
            format!("{:x}", hash)
        }
        Err(_) => String::new(),
    }
}

fn is_ignored_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    path_str.contains(".git/")
        || path_str.contains(".vibetap/")
        || path_str.contains("node_modules/")
        || path_str.contains("target/")
        || path_str.contains("__pycache__/")
        || path_str.contains(".pytest_cache/")
        || path_str.contains("dist/")
        || path_str.contains("build/")
        || path_str.contains(".next/")
        || path_str.contains(".turbo/")
        || path_str.ends_with(".lock")
        || path_str.ends_with(".log")
}

fn build_request(
    diff: &vibetap_git::StagedDiff,
    args: &WatchArgs,
    config: &Config,
) -> GenerateRequest {
    let hunks: Vec<DiffHunk> = diff
        .hunks
        .iter()
        .map(|h| DiffHunk {
            file_path: h.file_path.clone(),
            old_start: h.old_start,
            old_lines: h.old_lines,
            new_start: h.new_start,
            new_lines: h.new_lines,
            content: h.content.clone(),
        })
        .collect();

    let context: Vec<FileContext> = diff
        .files_changed
        .iter()
        .filter_map(|path| {
            std::fs::read_to_string(path).ok().map(|content| FileContext {
                path: path.clone(),
                content: content.chars().take(50000).collect(),
                language: detect_language(path),
            })
        })
        .take(10)
        .collect();

    let test_runner = config
        .project
        .as_ref()
        .map(|p| p.test_runner.clone())
        .unwrap_or_else(|| "vitest".to_string());

    GenerateRequest {
        diff: DiffPayload {
            hunks,
            base_branch: None,
            head_commit: None,
        },
        context,
        options: GenerateOptions {
            test_runner,
            max_suggestions: args.max_suggestions,
            include_security: args.security,
            include_negative_paths: true,
            model_tier: "default".to_string(),
        },
        policy_pack_id: None,
        repo_identifier: None,
    }
}

fn detect_language(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "ts" | "tsx" => Some("typescript".to_string()),
        "js" | "jsx" => Some("javascript".to_string()),
        "py" => Some("python".to_string()),
        "rs" => Some("rust".to_string()),
        "go" => Some("go".to_string()),
        "java" => Some("java".to_string()),
        "rb" => Some("ruby".to_string()),
        "php" => Some("php".to_string()),
        "cs" => Some("csharp".to_string()),
        "cpp" | "cc" | "cxx" => Some("cpp".to_string()),
        "c" | "h" => Some("c".to_string()),
        _ => None,
    }
}

fn save_suggestions(response: &vibetap_core::api::GenerateResponse) -> anyhow::Result<()> {
    let vibetap_dir = Path::new(".vibetap");
    if !vibetap_dir.exists() {
        std::fs::create_dir_all(vibetap_dir)?;
    }

    let suggestions_path = vibetap_dir.join("last-suggestions.json");
    let json = serde_json::to_string_pretty(response)?;
    std::fs::write(suggestions_path, json)?;

    Ok(())
}
