use clap::{Args, Subcommand};
use colored::Colorize;
use std::io::{self, Write};

use vibetap_core::{Config, GlobalConfig};

#[derive(Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Log in to VibeTap by providing your API key
    Login(LoginArgs),
    /// Log out and remove stored credentials
    Logout,
    /// Show current authentication status
    Status,
}

#[derive(Args)]
struct LoginArgs {
    /// API key (if not provided, will prompt interactively)
    #[arg(long)]
    key: Option<String>,

    /// API URL (defaults to https://vibetap.dev)
    #[arg(long)]
    api_url: Option<String>,
}

pub async fn execute(args: AuthArgs) -> anyhow::Result<()> {
    match args.command {
        AuthCommand::Login(login_args) => login(login_args).await,
        AuthCommand::Logout => logout().await,
        AuthCommand::Status => status().await,
    }
}

async fn login(args: LoginArgs) -> anyhow::Result<()> {
    println!("{}", "VibeTap Authentication".cyan().bold());
    println!();

    let api_key = if let Some(key) = args.key {
        key
    } else {
        println!("To get your API key:");
        println!("  1. Go to {} and sign in", "https://vibetap.dev".blue().underline());
        println!("  2. Navigate to Settings → API Keys");
        println!("  3. Create a new API key and copy it");
        println!();

        print!("{}", "Enter your API key: ".yellow());
        io::stdout().flush()?;

        let mut key = String::new();
        io::stdin().read_line(&mut key)?;
        key.trim().to_string()
    };

    if api_key.is_empty() {
        println!("{}", "Error: API key cannot be empty".red());
        return Ok(());
    }

    // Validate the API key format
    if !api_key.starts_with("vt_") {
        println!("{}", "Warning: API key should start with 'vt_'".yellow());
    }

    let api_url = args.api_url.unwrap_or_else(|| "https://vibetap.dev".to_string());

    // Verify the API key by calling the usage endpoint
    print!("{}", "Verifying API key... ".cyan());
    io::stdout().flush()?;

    match verify_api_key(&api_key, &api_url).await {
        Ok(_) => {
            println!("{}", "✓".green());

            // Save the configuration
            let config = GlobalConfig {
                api_key: Some(api_key),
                api_url: Some(api_url),
            };

            Config::save_global(&config)?;

            println!();
            println!("{}", "Successfully authenticated!".green().bold());
            println!(
                "Configuration saved to {}",
                Config::global_config_path().display().to_string().dimmed()
            );
        }
        Err(e) => {
            println!("{}", "✗".red());
            println!();
            println!("{} {}", "Authentication failed:".red(), e);
            println!();
            println!("Please check your API key and try again.");
        }
    }

    Ok(())
}

async fn logout() -> anyhow::Result<()> {
    let config_path = Config::global_config_path();

    if config_path.exists() {
        // Load existing config and clear the API key
        let config = GlobalConfig {
            api_key: None,
            api_url: None,
        };
        Config::save_global(&config)?;

        println!("{}", "Successfully logged out.".green());
        println!(
            "API key removed from {}",
            config_path.display().to_string().dimmed()
        );
    } else {
        println!("{}", "No credentials found. Already logged out.".yellow());
    }

    Ok(())
}

async fn status() -> anyhow::Result<()> {
    let config = Config::load()?;

    println!("{}", "VibeTap Authentication Status".cyan().bold());
    println!();

    if let Some(ref key) = config.global.api_key {
        let masked = if key.len() > 12 {
            format!("{}...{}", &key[..8], &key[key.len() - 4..])
        } else {
            "****".to_string()
        };

        println!("  {} {}", "Status:".bold(), "Authenticated".green());
        println!("  {} {}", "API Key:".bold(), masked.dimmed());
        println!("  {} {}", "API URL:".bold(), config.api_url().dimmed());

        // Try to fetch usage info
        print!("\n{}", "Fetching usage info... ".cyan());
        io::stdout().flush()?;

        match fetch_usage(key, config.api_url()).await {
            Ok(usage) => {
                println!("{}", "✓".green());
                println!();
                println!("  {} {}", "Requests this period:".bold(), usage.total_requests);
                println!("  {} {}", "Tokens used:".bold(), usage.total_tokens);
            }
            Err(_) => {
                println!("{}", "✗".red());
                println!("  Could not fetch usage information.");
            }
        }
    } else {
        println!("  {} {}", "Status:".bold(), "Not authenticated".red());
        println!();
        println!("Run {} to authenticate.", "vibetap auth login".cyan());
    }

    Ok(())
}

async fn verify_api_key(api_key: &str, api_url: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/usage", api_url);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Invalid API key")
    } else {
        anyhow::bail!("Server error: {}", response.status())
    }
}

struct UsageInfo {
    total_requests: u32,
    total_tokens: u32,
}

async fn fetch_usage(api_key: &str, api_url: &str) -> anyhow::Result<UsageInfo> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/usage", api_url);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch usage");
    }

    let body: serde_json::Value = response.json().await?;

    let usage = body
        .get("data")
        .and_then(|d| d.get("usage"))
        .ok_or_else(|| anyhow::anyhow!("Invalid response"))?;

    Ok(UsageInfo {
        total_requests: usage
            .get("totalRequests")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        total_tokens: usage
            .get("totalTokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    })
}
