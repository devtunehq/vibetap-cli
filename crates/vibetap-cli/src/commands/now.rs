use clap::Args;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use vibetap_core::{
    api::{DiffHunk, DiffPayload, FileContext, GenerateOptions, GenerateRequest},
    ApiClient, Config,
};
use vibetap_git::{get_staged_diff, get_uncommitted_diff, GitError};

#[derive(Args)]
pub struct NowArgs {
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
}

pub async fn execute(args: NowArgs) -> anyhow::Result<()> {
    // Load configuration
    let config = Config::load()?;
    let api_key = config.api_key()?;
    let api_url = config.api_url();

    // Get the diff based on scope
    let diff = if args.uncommitted {
        println!("{}", "Analyzing uncommitted changes...".cyan());
        get_uncommitted_diff()
    } else {
        println!("{}", "Analyzing staged changes...".cyan());
        get_staged_diff()
    };

    let diff = match diff {
        Ok(d) => d,
        Err(GitError::NoStagedChanges) => {
            println!(
                "\n{}",
                "No changes found. Stage some changes first with 'git add'.".yellow()
            );
            return Ok(());
        }
        Err(GitError::NotARepo) => {
            println!(
                "\n{}",
                "Not a git repository. Run this command from within a git repo.".red()
            );
            return Ok(());
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    println!(
        "  Found {} in {} file(s)",
        format!("{} hunk(s)", diff.hunks.len()).green(),
        diff.files_changed.len()
    );

    // Show progress spinner
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Generating test suggestions...");
    spinner.enable_steady_tick(Duration::from_millis(100));

    // Build the API request
    let request = build_request(&diff, &args, &config);

    // Call the API
    let client = ApiClient::new(api_url, api_key);
    let response = match client.generate(request).await {
        Ok(r) => r,
        Err(e) => {
            spinner.finish_and_clear();
            println!("\n{} {}", "Error:".red(), e);
            return Ok(());
        }
    };

    spinner.finish_and_clear();

    // Display results
    println!("\n{}", "=== Test Suggestions ===".bold());
    println!();

    if response.escalated {
        println!(
            "{}",
            "ℹ Used enhanced model for complex/security-sensitive code".dimmed()
        );
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
        println!("   {} {}", "Type:".dimmed(), format_category(&suggestion.category));
        println!(
            "   {} {:.0}%",
            "Confidence:".dimmed(),
            suggestion.confidence * 100.0
        );
        println!("   {} {}", "Description:".dimmed(), suggestion.description);

        if !suggestion.risks_addressed.is_empty() {
            println!(
                "   {} {}",
                "Risks covered:".dimmed(),
                suggestion.risks_addressed.join(", ")
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
    args: &NowArgs,
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
                language: detect_language(path),
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
