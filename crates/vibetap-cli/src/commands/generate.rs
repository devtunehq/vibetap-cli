use clap::Args;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

use vibetap_core::{
    api::{
        DiffHunk, DiffPayload, FileContext, GenerateOptions, GenerateRequest, GenerateResponse,
        StreamEvent,
    },
    ApiClient, Config,
};
use vibetap_git::{get_staged_diff, get_uncommitted_diff, GitError};

/// Saved suggestions with source file state for change detection
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSuggestions {
    pub response: GenerateResponse,
    pub source_files: HashMap<String, String>, // path -> content hash
    pub generated_at: i64,
}

#[derive(Args)]
pub struct GenerateArgs {
    /// Specific file to generate tests for (optional, defaults to all staged changes)
    #[arg(value_name = "FILE")]
    file: Option<String>,

    /// Generate tests for staged changes only (default)
    #[arg(long, default_value = "true")]
    staged: bool,

    /// Generate tests for all uncommitted changes
    #[arg(long)]
    uncommitted: bool,

    /// Prioritize security guardrail tests
    #[arg(long)]
    security: bool,

    /// Maximum number of suggestions to generate
    #[arg(long, default_value = "3")]
    max_suggestions: u32,

    /// Test runner to use (vitest, jest, pytest, etc.)
    #[arg(long)]
    test_runner: Option<String>,

    /// Quiet mode - show condensed output (useful for git hooks)
    #[arg(short, long)]
    quiet: bool,
}

pub async fn execute(args: GenerateArgs) -> anyhow::Result<()> {
    // Load configuration
    let mut config = Config::load()?;
    let access_token = config.get_valid_access_token().await?;
    let api_url = config.api_url().to_string();

    let quiet = args.quiet;

    // Get the diff based on scope
    let diff = if args.uncommitted {
        if !quiet {
            println!("{}", "Analyzing uncommitted changes...".cyan());
        }
        get_uncommitted_diff()
    } else {
        if !quiet {
            println!("{}", "Analyzing staged changes...".cyan());
        }
        get_staged_diff()
    };

    let mut diff = match diff {
        Ok(d) => d,
        Err(GitError::NoStagedChanges) => {
            if !quiet {
                println!(
                    "\n{}",
                    "No changes found. Stage some changes first with 'git add'.".yellow()
                );
            }
            return Ok(());
        }
        Err(GitError::NotARepo) => {
            if !quiet {
                println!(
                    "\n{}",
                    "Not a git repository. Run this command from within a git repo.".red()
                );
            }
            return Ok(());
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    // Filter by specific file if provided
    if let Some(ref file_filter) = args.file {
        let normalized_filter = file_filter.trim_start_matches("./");
        diff.hunks.retain(|h| {
            let normalized_path = h.file_path.trim_start_matches("./");
            normalized_path == normalized_filter || normalized_path.ends_with(normalized_filter)
        });
        diff.files_changed.retain(|f| {
            let normalized_path = f.trim_start_matches("./");
            normalized_path == normalized_filter || normalized_path.ends_with(normalized_filter)
        });

        if diff.hunks.is_empty() {
            if !quiet {
                println!(
                    "\n{}",
                    format!("No changes found for file: {}", file_filter).yellow()
                );
            }
            return Ok(());
        }
    }

    if !quiet {
        println!(
            "  Found {} in {} file(s)",
            format!("{} hunk(s)", diff.hunks.len()).green(),
            diff.files_changed.len()
        );
    }

    // Build the API request
    let request = build_request(&diff, &args, &config);

    // Calculate payload size for progress display
    let payload_size = serde_json::to_string(&request)
        .map(|s| s.len())
        .unwrap_or(0);

    // Show upload progress bar (only in non-quiet mode)
    if !quiet {
        print_upload_progress(payload_size);
    }

    // Call the streaming API
    let client = ApiClient::new(api_url, access_token);

    // Create progress bar for generation phase
    let progress_bar = if !quiet {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    // Track suggestions as they stream in
    let mut streamed_suggestions: Vec<vibetap_core::api::TestSuggestion> = Vec::new();

    let response = match client
        .generate_streaming(request, |event| {
            match event {
                StreamEvent::Progress { phase, message, .. } => {
                    if let Some(ref pb) = progress_bar {
                        let phase_icon = match phase.as_str() {
                            "authenticating" => "🔐",
                            "analyzing" => "🔍",
                            "context" => "📚",
                            "generating" => "⚡",
                            _ => "•",
                        };
                        pb.set_message(format!("{} {}", phase_icon, message));
                    }
                }
                StreamEvent::Suggestion {
                    index,
                    total,
                    suggestion,
                } => {
                    if let Some(ref pb) = progress_bar {
                        pb.set_message(format!(
                            "📝 Generated suggestion {}/{}: {}",
                            index,
                            total,
                            suggestion.file_path.cyan()
                        ));
                    }
                    streamed_suggestions.push(suggestion);
                }
                StreamEvent::Complete { .. } => {
                    if let Some(ref pb) = progress_bar {
                        pb.finish_and_clear();
                    }
                }
                StreamEvent::Error { code, message } => {
                    if let Some(ref pb) = progress_bar {
                        pb.finish_and_clear();
                    }
                    if !quiet {
                        eprintln!("\n{} {} - {}", "Error:".red(), code, message);
                    }
                }
            }
        })
        .await
    {
        Ok(r) => r,
        Err(e) => {
            if let Some(pb) = progress_bar {
                pb.finish_and_clear();
            }
            if !quiet {
                println!("\n{} {}", "Error:".red(), e);
            }
            return Ok(());
        }
    };

    // Save suggestions for later use by apply command (with source file hashes)
    if let Err(e) = save_suggestions(&response, &diff.files_changed) {
        if !quiet {
            eprintln!("{} {}", "Warning: Could not save suggestions:".yellow(), e);
        }
    }

    // Quiet mode: show condensed output
    if quiet {
        let count = response.suggestions.len();
        if count > 0 {
            let security_count = response
                .suggestions
                .iter()
                .filter(|s| s.category == "security")
                .count();

            if security_count > 0 {
                println!(
                    "VibeTap: {} test suggestion(s) available ({} security). Run 'vibetap generate' for details.",
                    count, security_count
                );
            } else {
                println!(
                    "VibeTap: {} test suggestion(s) available. Run 'vibetap generate' for details or 'vibetap apply' to add.",
                    count
                );
            }
        }
        return Ok(());
    }

    // Full output mode
    println!("\n{}", "=== Test Suggestions ===".bold());
    println!();

    if response.used_byok {
        println!(
            "{}",
            "ℹ Using your own API key (BYOK mode)".dimmed()
        );
        println!();
    }

    if let Some(ref warning) = response.warning {
        println!("{} {}", "⚠".yellow(), warning.yellow());
        println!();
    }

    if response.suggestions.is_empty() {
        println!("{}", "No test suggestions generated.".yellow());
        return Ok(());
    }

    for (i, suggestion) in response.suggestions.iter().enumerate() {
        println!(
            "{} {}",
            format!("{}.", i + 1).bold(),
            suggestion.file_path.cyan()
        );
        println!(
            "   {} {} | {} {:.0}%",
            "Type:".dimmed(),
            format_category(&suggestion.category),
            "Confidence:".dimmed(),
            suggestion.confidence * 100.0
        );
        println!("   {}", suggestion.description.dimmed());
        println!();

        // Display the test code with a border
        print_code_block(&suggestion.code, &suggestion.file_path);

        if !suggestion.risks_addressed.is_empty() {
            println!(
                "   {} {}",
                "Risks:".dimmed(),
                suggestion.risks_addressed.join(", ").dimmed()
            );
        }
        println!();
    }

    println!("{}", response.summary.dimmed());
    println!();
    println!(
        "Run {} to apply a suggestion.",
        "vibetap apply <number>".cyan()
    );
    println!(
        "Tokens used: {} | Model: {}",
        response.tokens_used.to_string().dimmed(),
        response.model_used.dimmed()
    );

    Ok(())
}

fn build_request(
    diff: &vibetap_git::StagedDiff,
    args: &GenerateArgs,
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

    // Load context files (the changed files themselves)
    let context: Vec<FileContext> = diff
        .files_changed
        .iter()
        .filter_map(|path| {
            std::fs::read_to_string(path).ok().map(|content| FileContext {
                path: path.clone(),
                content: content.chars().take(50000).collect(), // Limit to 50KB
                language: Some(detect_language(path)),
            })
        })
        .take(10) // Limit context files
        .collect();

    // Determine test runner
    let test_runner = args.test_runner.clone().unwrap_or_else(|| {
        config
            .project
            .as_ref()
            .map(|p| p.test_runner.clone())
            .unwrap_or_else(|| "vitest".to_string())
    });

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

fn detect_language(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "ts" | "tsx" => "typescript".to_string(),
        "js" | "jsx" => "javascript".to_string(),
        "py" => "python".to_string(),
        "rs" => "rust".to_string(),
        "go" => "go".to_string(),
        "java" => "java".to_string(),
        "rb" => "ruby".to_string(),
        "php" => "php".to_string(),
        "cs" => "csharp".to_string(),
        "cpp" | "cc" | "cxx" => "cpp".to_string(),
        "c" | "h" => "c".to_string(),
        "json" => "json".to_string(),
        "yaml" | "yml" => "yaml".to_string(),
        "toml" => "toml".to_string(),
        "md" => "markdown".to_string(),
        "sql" => "sql".to_string(),
        "sh" | "bash" => "shell".to_string(),
        "css" => "css".to_string(),
        "scss" | "sass" => "scss".to_string(),
        "html" | "htm" => "html".to_string(),
        _ => "text".to_string(),
    }
}

fn format_category(category: &str) -> String {
    match category {
        "unit" => "Unit test".to_string(),
        "integration" => "Integration test".to_string(),
        "security" => "Security test".to_string(),
        "edge_case" => "Edge case test".to_string(),
        "regression" => "Regression test".to_string(),
        _ => category.to_string(),
    }
}

fn print_code_block(code: &str, file_path: &str) {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];

    // Detect syntax from file extension
    let extension = file_path.rsplit('.').next().unwrap_or("js");

    // Find syntax with fallback chain: exact match -> JS for TS/JSX files -> plain text
    let syntax = ps
        .find_syntax_by_extension(extension)
        .or_else(|| {
            // TypeScript/JSX aren't in syntect's defaults, fall back to JavaScript
            if matches!(extension, "ts" | "tsx" | "jsx") {
                ps.find_syntax_by_extension("js")
            } else {
                None
            }
        })
        .unwrap_or_else(|| ps.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, theme);

    // Print top border
    println!("   {}", "┌─".dimmed());

    // Print highlighted code with proper color resets
    for line in LinesWithEndings::from(code) {
        let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &ps).unwrap();
        let escaped = as_24_bit_terminal_escaped(&ranges[..], true); // Reset colors at end
        // Remove trailing newline for cleaner output
        let escaped = escaped.trim_end_matches('\n');
        println!("   {}  {}", "│".dimmed(), escaped);
    }

    // Print bottom border
    println!("   {}", "└─".dimmed());
}

/// Save suggestions to .vibetap/last-suggestions.json for apply command
fn save_suggestions(response: &GenerateResponse, source_files: &[String]) -> anyhow::Result<()> {
    let vibetap_dir = Path::new(".vibetap");
    if !vibetap_dir.exists() {
        std::fs::create_dir_all(vibetap_dir)?;
    }

    // Compute hashes of source files
    let mut file_hashes = HashMap::new();
    for path in source_files {
        if let Ok(content) = std::fs::read_to_string(path) {
            file_hashes.insert(path.clone(), compute_hash(&content));
        }
    }

    let saved = SavedSuggestions {
        response: response.clone(),
        source_files: file_hashes,
        generated_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    };

    let suggestions_path = vibetap_dir.join("last-suggestions.json");
    let json = serde_json::to_string_pretty(&saved)?;
    std::fs::write(suggestions_path, json)?;

    Ok(())
}

/// Compute a simple hash of content for change detection
pub fn compute_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Load the last saved suggestions
pub fn load_suggestions() -> anyhow::Result<SavedSuggestions> {
    let suggestions_path = Path::new(".vibetap/last-suggestions.json");
    if !suggestions_path.exists() {
        anyhow::bail!("No suggestions found. Run 'vibetap generate' first.");
    }

    let content = std::fs::read_to_string(suggestions_path)?;

    // Try to load new format first, fall back to old format for backwards compatibility
    if let Ok(saved) = serde_json::from_str::<SavedSuggestions>(&content) {
        return Ok(saved);
    }

    // Fall back to old format (just GenerateResponse)
    let response: GenerateResponse = serde_json::from_str(&content)?;
    Ok(SavedSuggestions {
        response,
        source_files: HashMap::new(), // No hashes in old format
        generated_at: 0,
    })
}

/// Print a nice ASCII art upload progress bar
fn print_upload_progress(payload_size: usize) {
    let size_kb = payload_size as f64 / 1024.0;
    let bar_width = 30;
    let filled = bar_width; // Instant upload visualization

    // Build the progress bar
    let bar: String = "█".repeat(filled);
    let empty: String = "░".repeat(bar_width - filled);

    // Print upload progress
    print!(
        "\r  {} [{}{}] {:.1} KB ",
        "Uploading".cyan(),
        bar.green(),
        empty.dimmed(),
        size_kb
    );
    io::stdout().flush().ok();

    // Simulate brief upload animation for visual feedback
    for i in 0..=bar_width {
        let filled_part: String = "█".repeat(i);
        let empty_part: String = "░".repeat(bar_width - i);
        print!(
            "\r  {} [{}{}] {:.1} KB ",
            "Uploading".cyan(),
            filled_part.green(),
            empty_part.dimmed(),
            size_kb * (i as f64 / bar_width as f64)
        );
        io::stdout().flush().ok();
        std::thread::sleep(Duration::from_millis(10));
    }

    println!(
        "\r  {} [{}] {:.1} KB {}",
        "Uploaded".green(),
        "█".repeat(bar_width).green(),
        size_kb,
        "✓".green()
    );
}
