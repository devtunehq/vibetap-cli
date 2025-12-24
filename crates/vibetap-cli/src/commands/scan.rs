use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clap::Args;
use colored::Colorize;
use walkdir::WalkDir;

#[derive(Args)]
pub struct ScanArgs {
    /// Directory to scan (defaults to current directory)
    #[arg(default_value = ".")]
    path: String,

    /// Show all files, not just high-risk ones
    #[arg(long)]
    all: bool,

    /// Maximum number of results to show
    #[arg(long, default_value = "10")]
    limit: usize,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Debug)]
struct ScanResult {
    path: String,
    file_type: String,
    risk_level: RiskLevel,
    has_tests: bool,
    test_file: Option<String>,
    reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RiskLevel {
    High,
    Medium,
    Low,
}

impl RiskLevel {
    fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::High => "HIGH",
            RiskLevel::Medium => "MED",
            RiskLevel::Low => "LOW",
        }
    }
}

pub async fn execute(args: ScanArgs) -> anyhow::Result<()> {
    let scan_path = Path::new(&args.path);

    if !scan_path.exists() {
        println!("{} Path does not exist: {}", "Error:".red(), args.path);
        return Ok(());
    }

    println!("{}", "Scanning repository for coverage gaps...".cyan());
    println!();

    // Find all source files and their corresponding test files
    let source_files = find_source_files(scan_path);
    let test_files = find_test_files(scan_path);

    // Analyze coverage
    let results = analyze_coverage(&source_files, &test_files);

    if args.json {
        let json_results: Vec<_> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "path": r.path,
                    "fileType": r.file_type,
                    "riskLevel": r.risk_level.as_str(),
                    "hasTests": r.has_tests,
                    "testFile": r.test_file,
                    "reason": r.reason,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_results)?);
        return Ok(());
    }

    // Filter and sort results
    let mut results: Vec<_> = results
        .into_iter()
        .filter(|r| !r.has_tests)
        .collect();
    results.sort_by(|a, b| a.risk_level.cmp(&b.risk_level));

    let total_files = source_files.len();
    let files_without_tests = results.len();
    let files_with_tests = total_files - files_without_tests;

    // Summary
    println!(
        "Found {} source files ({} with tests, {} without)",
        total_files.to_string().bold(),
        files_with_tests.to_string().green(),
        files_without_tests.to_string().yellow()
    );
    println!();

    if files_without_tests == 0 {
        println!("{}", "All source files have corresponding tests!".green());
        return Ok(());
    }

    // Show high-risk files
    let display_results: Vec<_> = if args.all {
        results.iter().take(args.limit).collect()
    } else {
        results
            .iter()
            .filter(|r| r.risk_level == RiskLevel::High || r.risk_level == RiskLevel::Medium)
            .take(args.limit)
            .collect()
    };

    if display_results.is_empty() {
        println!("{}", "No high-risk files found without tests.".green());
        println!(
            "Use {} to see all files without tests.",
            "--all".cyan()
        );
        return Ok(());
    }

    println!("{}", "Files needing tests:".bold());
    println!();

    for (i, result) in display_results.iter().enumerate() {
        let risk_badge = match result.risk_level {
            RiskLevel::High => format!("[{}]", "HIGH".red()),
            RiskLevel::Medium => format!("[{}]", "MED".yellow()),
            RiskLevel::Low => format!("[{}]", "LOW".dimmed()),
        };

        println!(
            "  {}. {} {}",
            (i + 1).to_string().dimmed(),
            result.path.cyan(),
            risk_badge
        );
        println!("     {}", result.reason.dimmed());
    }

    if results.len() > args.limit {
        println!();
        println!(
            "{} more files without tests. Use {} to see all.",
            (results.len() - args.limit).to_string().yellow(),
            "--all --limit 50".cyan()
        );
    }

    println!();
    println!(
        "Run {} to generate tests for a specific file.",
        "vibetap generate <file>".cyan()
    );

    Ok(())
}

fn find_source_files(base_path: &Path) -> Vec<PathBuf> {
    let source_extensions = ["ts", "tsx", "js", "jsx", "py", "rs", "go", "rb", "java"];
    let ignore_patterns = [
        "node_modules",
        "target",
        "dist",
        "build",
        ".git",
        "__pycache__",
        ".next",
        "coverage",
        ".turbo",
    ];

    WalkDir::new(base_path)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !ignore_patterns.iter().any(|p| name.contains(p))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let path = e.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Must have a source extension
            source_extensions.contains(&ext)
                // Exclude test files
                && !name.contains(".test.")
                && !name.contains(".spec.")
                && !name.contains("_test.")
                && !name.ends_with("_test.go")
                && !name.ends_with("_test.py")
                // Exclude type definition files
                && !name.ends_with(".d.ts")
        })
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn find_test_files(base_path: &Path) -> HashMap<String, PathBuf> {
    let ignore_patterns = [
        "node_modules",
        "target",
        "dist",
        "build",
        ".git",
        "__pycache__",
    ];

    WalkDir::new(base_path)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !ignore_patterns.iter().any(|p| name.contains(p))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let name = e.file_name().to_string_lossy();
            name.contains(".test.")
                || name.contains(".spec.")
                || name.contains("_test.")
                || name.ends_with("_test.go")
                || name.ends_with("_test.py")
        })
        .map(|e| {
            // Extract the base name that's being tested
            let name = e.file_name().to_string_lossy().to_string();
            let base = name
                .replace(".test.", ".")
                .replace(".spec.", ".")
                .replace("_test.", ".")
                .replace("_test.go", ".go")
                .replace("_test.py", ".py");
            (base, e.path().to_path_buf())
        })
        .collect()
}

fn analyze_coverage(source_files: &[PathBuf], test_files: &HashMap<String, PathBuf>) -> Vec<ScanResult> {
    source_files
        .iter()
        .map(|source| {
            let file_name = source
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let ext = source
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            // Check if there's a corresponding test file
            let test_file = test_files.get(&file_name).cloned();
            let has_tests = test_file.is_some();

            // Determine risk level based on file path and name
            let path_str = source.to_string_lossy().to_lowercase();
            let (risk_level, reason) = determine_risk(&path_str, &file_name);

            ScanResult {
                path: source.to_string_lossy().to_string(),
                file_type: ext.to_string(),
                risk_level,
                has_tests,
                test_file: test_file.map(|p| p.to_string_lossy().to_string()),
                reason,
            }
        })
        .collect()
}

fn determine_risk(path: &str, _file_name: &str) -> (RiskLevel, String) {
    // High-risk patterns (security, auth, payments)
    if path.contains("auth")
        || path.contains("login")
        || path.contains("password")
        || path.contains("token")
        || path.contains("session")
    {
        return (RiskLevel::High, "Authentication/authorization code".to_string());
    }

    if path.contains("payment")
        || path.contains("billing")
        || path.contains("checkout")
        || path.contains("stripe")
        || path.contains("subscription")
    {
        return (RiskLevel::High, "Payment/billing logic".to_string());
    }

    if path.contains("api/")
        || path.contains("routes/")
        || path.contains("endpoints")
        || path.contains("handlers")
    {
        return (RiskLevel::High, "API endpoint".to_string());
    }

    if path.contains("crypto")
        || path.contains("encrypt")
        || path.contains("decrypt")
        || path.contains("hash")
    {
        return (RiskLevel::High, "Cryptographic operations".to_string());
    }

    // Medium-risk patterns
    if path.contains("service")
        || path.contains("controller")
        || path.contains("repository")
    {
        return (RiskLevel::Medium, "Core business logic".to_string());
    }

    if path.contains("database")
        || path.contains("db/")
        || path.contains("model")
        || path.contains("schema")
    {
        return (RiskLevel::Medium, "Database operations".to_string());
    }

    if path.contains("middleware") || path.contains("interceptor") {
        return (RiskLevel::Medium, "Middleware/interceptor".to_string());
    }

    if path.contains("validation")
        || path.contains("validator")
        || path.contains("schema")
    {
        return (RiskLevel::Medium, "Input validation".to_string());
    }

    // Low-risk patterns
    if path.contains("util")
        || path.contains("helper")
        || path.contains("lib/")
    {
        return (RiskLevel::Low, "Utility/helper code".to_string());
    }

    if path.contains("component")
        || path.contains("ui/")
        || path.contains("view")
    {
        return (RiskLevel::Low, "UI component".to_string());
    }

    if path.contains("config") || path.contains("constant") {
        return (RiskLevel::Low, "Configuration".to_string());
    }

    (RiskLevel::Low, "General source file".to_string())
}
