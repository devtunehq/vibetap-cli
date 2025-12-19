use clap::Args;
use colored::Colorize;

#[derive(Args)]
pub struct InitArgs {
    /// Force re-initialization even if already configured
    #[arg(short, long)]
    force: bool,
}

pub async fn execute(args: InitArgs) -> anyhow::Result<()> {
    println!("{}", "Initializing VibeTap...".cyan());

    // TODO: Detect framework and test runner
    // TODO: Read AGENTS.md / CLAUDE.md for project guidance
    // TODO: Create .vibetap/config.json

    if args.force {
        println!("Force mode: overwriting existing configuration");
    }

    // Check for existing config
    let config_path = std::path::Path::new(".vibetap/config.json");
    if config_path.exists() && !args.force {
        println!(
            "{}",
            "VibeTap is already initialized. Use --force to re-initialize.".yellow()
        );
        return Ok(());
    }

    // Create config directory
    std::fs::create_dir_all(".vibetap")?;

    // Detect project type
    let project_type = detect_project_type();
    println!("Detected project type: {}", project_type.green());

    // Create default config
    let config = serde_json::json!({
        "version": "1.0",
        "projectType": project_type,
        "testRunner": detect_test_runner(),
        "watchMode": {
            "enabled": true,
            "debounceMs": 2000
        },
        "generation": {
            "maxSuggestions": 3,
            "includeSecurity": true,
            "includeNegativePaths": true
        }
    });

    std::fs::write(
        ".vibetap/config.json",
        serde_json::to_string_pretty(&config)?,
    )?;

    println!("{}", "VibeTap initialized successfully!".green());
    println!("Configuration saved to .vibetap/config.json");
    println!("\nNext steps:");
    println!("  1. Add your API key: vibetap auth login");
    println!("  2. Start watching: vibetap watch");
    println!("  3. Or generate now: vibetap now --staged");

    Ok(())
}

fn detect_project_type() -> &'static str {
    if std::path::Path::new("next.config.js").exists()
        || std::path::Path::new("next.config.ts").exists()
        || std::path::Path::new("next.config.mjs").exists()
    {
        return "nextjs";
    }
    if std::path::Path::new("package.json").exists() {
        return "node";
    }
    if std::path::Path::new("Cargo.toml").exists() {
        return "rust";
    }
    "unknown"
}

fn detect_test_runner() -> &'static str {
    // Check for Vitest
    if std::path::Path::new("vitest.config.ts").exists()
        || std::path::Path::new("vitest.config.js").exists()
    {
        return "vitest";
    }
    // Check for Jest
    if std::path::Path::new("jest.config.ts").exists()
        || std::path::Path::new("jest.config.js").exists()
    {
        return "jest";
    }
    // Check package.json for test scripts
    if let Ok(content) = std::fs::read_to_string("package.json") {
        if content.contains("vitest") {
            return "vitest";
        }
        if content.contains("jest") {
            return "jest";
        }
    }
    "vitest" // Default
}
