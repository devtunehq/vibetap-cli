use clap::Args;
use colored::Colorize;
use std::path::Path;
use std::process::Command;

use super::apply::ApplyHistory;
use vibetap_core::Config;

#[derive(Args)]
pub struct RunArgs {
    /// Run all tests, not just generated ones
    #[arg(long)]
    all: bool,

    /// Test runner to use (auto-detected if not specified)
    #[arg(long)]
    runner: Option<String>,

    /// Pass additional arguments to the test runner
    #[arg(last = true)]
    args: Vec<String>,
}

pub async fn execute(args: RunArgs) -> anyhow::Result<()> {
    // Determine test runner
    let runner = if let Some(r) = args.runner {
        r
    } else {
        detect_test_runner()?
    };

    println!(
        "{} {}",
        "Using test runner:".cyan(),
        runner.bold()
    );

    // Get files to test
    let test_files: Vec<String> = if args.all {
        Vec::new() // Empty = run all tests
    } else {
        // Get only applied test files
        let history = load_history()?;
        history
            .records
            .iter()
            .map(|r| r.file_path.clone())
            .filter(|p| Path::new(p).exists())
            .collect()
    };

    if !args.all && test_files.is_empty() {
        println!(
            "{}",
            "No applied test files found. Use --all to run all tests.".yellow()
        );
        return Ok(());
    }

    // Build command based on runner
    let (cmd, cmd_args) = build_command(&runner, &test_files, &args.args)?;

    println!(
        "{} {} {}",
        "Running:".dimmed(),
        cmd,
        cmd_args.join(" ")
    );
    println!();

    // Execute the test runner
    let status = Command::new(&cmd)
        .args(&cmd_args)
        .status()?;

    if status.success() {
        println!("\n{}", "All tests passed!".green().bold());
    } else {
        let code = status.code().unwrap_or(1);
        println!(
            "\n{} (exit code: {})",
            "Tests failed!".red().bold(),
            code
        );
        std::process::exit(code);
    }

    Ok(())
}

fn detect_test_runner() -> anyhow::Result<String> {
    // Try to load from config first
    if let Ok(config) = Config::load() {
        if let Some(project) = config.project {
            return Ok(project.test_runner);
        }
    }

    // Auto-detect from project files
    if Path::new("vitest.config.ts").exists()
        || Path::new("vitest.config.js").exists()
        || Path::new("vitest.config.mts").exists()
    {
        return Ok("vitest".to_string());
    }

    if Path::new("jest.config.ts").exists()
        || Path::new("jest.config.js").exists()
        || Path::new("jest.config.json").exists()
    {
        return Ok("jest".to_string());
    }

    if Path::new("pytest.ini").exists()
        || Path::new("pyproject.toml").exists()
        || Path::new("setup.py").exists()
    {
        // Check if pytest is in pyproject.toml
        if let Ok(content) = std::fs::read_to_string("pyproject.toml") {
            if content.contains("pytest") {
                return Ok("pytest".to_string());
            }
        }
    }

    if Path::new("Cargo.toml").exists() {
        return Ok("cargo-test".to_string());
    }

    if Path::new("go.mod").exists() {
        return Ok("go-test".to_string());
    }

    // Default to vitest for JS/TS projects
    if Path::new("package.json").exists() {
        return Ok("vitest".to_string());
    }

    anyhow::bail!(
        "Could not detect test runner. Use --runner to specify one.\n\
         Supported: vitest, jest, pytest, cargo-test, go-test"
    )
}

fn build_command(
    runner: &str,
    test_files: &[String],
    extra_args: &[String],
) -> anyhow::Result<(String, Vec<String>)> {
    match runner {
        "vitest" => {
            let mut args = vec!["run".to_string()];
            args.extend(test_files.iter().cloned());
            args.extend(extra_args.iter().cloned());
            Ok(("npx".to_string(), {
                let mut v = vec!["vitest".to_string()];
                v.extend(args);
                v
            }))
        }
        "jest" => {
            let mut args = test_files.to_vec();
            args.extend(extra_args.iter().cloned());
            Ok(("npx".to_string(), {
                let mut v = vec!["jest".to_string()];
                v.extend(args);
                v
            }))
        }
        "pytest" => {
            let mut args = test_files.to_vec();
            args.extend(extra_args.iter().cloned());
            Ok(("pytest".to_string(), args))
        }
        "cargo-test" => {
            let mut args = vec!["test".to_string()];
            // Cargo test doesn't take file paths directly, use --test for specific tests
            if !test_files.is_empty() {
                println!(
                    "{}",
                    "Note: Cargo test runs all tests. Use 'cargo test <name>' for specific tests."
                        .dimmed()
                );
            }
            args.extend(extra_args.iter().cloned());
            Ok(("cargo".to_string(), args))
        }
        "go-test" => {
            let mut args = vec!["test".to_string()];
            if test_files.is_empty() {
                args.push("./...".to_string());
            } else {
                args.extend(test_files.iter().cloned());
            }
            args.extend(extra_args.iter().cloned());
            Ok(("go".to_string(), args))
        }
        _ => {
            // Custom runner - just run it directly
            let mut args = test_files.to_vec();
            args.extend(extra_args.iter().cloned());
            Ok((runner.to_string(), args))
        }
    }
}

fn load_history() -> anyhow::Result<ApplyHistory> {
    let path = Path::new(".vibetap/history.json");
    if !path.exists() {
        return Ok(ApplyHistory::default());
    }

    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}
