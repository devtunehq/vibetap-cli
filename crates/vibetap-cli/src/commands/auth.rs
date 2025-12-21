use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use clap::{Args, Subcommand};
use colored::Colorize;
use rand::Rng;

use vibetap_core::{AuthTokens, Config};

#[derive(Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Log in to VibeTap via browser authentication
    Login(LoginArgs),
    /// Log out and remove stored credentials
    Logout,
    /// Show current authentication status
    Status,
}

#[derive(Args)]
struct LoginArgs {
    /// API URL (defaults to https://vibetap.dev)
    #[arg(long)]
    api_url: Option<String>,

    /// Use API key instead of OAuth (for CI/CD)
    #[arg(long)]
    key: Option<String>,
}

pub async fn execute(args: AuthArgs) -> anyhow::Result<()> {
    match args.command {
        AuthCommand::Login(login_args) => login(login_args).await,
        AuthCommand::Logout => logout().await,
        AuthCommand::Status => status().await,
    }
}

async fn login(args: LoginArgs) -> anyhow::Result<()> {
    let api_url = args
        .api_url
        .unwrap_or_else(|| "https://vibetap.dev".to_string());

    // If API key provided, use simple key-based auth (for CI/CD)
    if let Some(key) = args.key {
        return login_with_key(&key, &api_url).await;
    }

    // OAuth flow
    login_with_oauth(&api_url).await
}

async fn login_with_key(key: &str, api_url: &str) -> anyhow::Result<()> {
    println!("{}", "Authenticating with API key...".cyan());

    // Validate the key
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/v1/usage", api_url))
        .header("Authorization", format!("Bearer {}", key))
        .send()
        .await?;

    if !response.status().is_success() {
        println!("{}", "Invalid API key.".red());
        return Ok(());
    }

    // Save as API key auth
    let tokens = AuthTokens {
        access_token: key.to_string(),
        refresh_token: None,
        expires_at: None,
        auth_type: "api_key".to_string(),
    };

    Config::save_tokens(&tokens, api_url)?;

    println!("{}", "Successfully authenticated with API key!".green());
    println!(
        "Configuration saved to {}",
        Config::global_config_path().display().to_string().dimmed()
    );

    Ok(())
}

async fn login_with_oauth(api_url: &str) -> anyhow::Result<()> {
    println!("{}", "VibeTap Authentication".cyan().bold());
    println!();

    // Find an available port
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();

    // Generate random state for CSRF protection
    let state: String = rand::rng()
        .sample_iter(&rand::distr::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    // Build auth URL
    let auth_url = format!(
        "{}/cli/auth?port={}&state={}",
        api_url, port, state
    );

    println!("Opening browser to authenticate...");
    println!();
    println!("If your browser doesn't open, visit:");
    println!("  {}", auth_url.blue().underline());
    println!();

    // Open browser (webbrowser crate has better fallback logic than open)
    if let Err(e) = webbrowser::open(&auth_url) {
        println!(
            "{} {}",
            "Could not open browser:".yellow(),
            e
        );
        println!("Please open the URL above manually.");
    }

    println!("Waiting for authentication...");

    // Wait for callback (with timeout)
    listener.set_nonblocking(false)?;

    // Set a timeout using a separate thread approach
    let (tx, rx) = std::sync::mpsc::channel();
    let listener_clone = listener.try_clone()?;

    std::thread::spawn(move || {
        // Handle multiple connections (CORS preflight OPTIONS + actual POST)
        loop {
            if let Ok((mut stream, _)) = listener_clone.accept() {
                let mut buffer = [0; 8192];
                if let Ok(n) = stream.read(&mut buffer) {
                    let request = String::from_utf8_lossy(&buffer[..n]);

                    // Check if this is a CORS preflight OPTIONS request
                    if request.starts_with("OPTIONS") {
                        // Respond to preflight with CORS headers
                        let response = "HTTP/1.1 204 No Content\r\n\
                            Access-Control-Allow-Origin: *\r\n\
                            Access-Control-Allow-Methods: POST, OPTIONS\r\n\
                            Access-Control-Allow-Headers: Content-Type\r\n\
                            Access-Control-Max-Age: 86400\r\n\r\n";
                        let _ = stream.write_all(response.as_bytes());
                        // Continue listening for the actual POST
                        continue;
                    }

                    // This is the actual POST with tokens
                    let _ = tx.send(request.to_string());

                    // Send success response with CORS headers
                    let response = "HTTP/1.1 200 OK\r\n\
                        Content-Type: application/json\r\n\
                        Access-Control-Allow-Origin: *\r\n\r\n\
                        {\"success\":true}";
                    let _ = stream.write_all(response.as_bytes());
                    break; // Done, exit the loop
                }
            }
        }
    });

    // Wait for callback with 2 minute timeout
    let request = rx
        .recv_timeout(Duration::from_secs(120))
        .map_err(|_| anyhow::anyhow!("Authentication timed out"))?;

    // Parse the callback
    let tokens = parse_callback(&request, &state)?;

    // Save tokens
    Config::save_tokens(&tokens, api_url)?;

    println!();
    println!("{}", "Successfully authenticated!".green().bold());
    println!(
        "Configuration saved to {}",
        Config::global_config_path().display().to_string().dimmed()
    );

    Ok(())
}

fn parse_callback(request: &str, expected_state: &str) -> anyhow::Result<AuthTokens> {
    // Parse HTTP POST request with JSON body
    // Request looks like: POST /callback HTTP/1.1\r\n...headers...\r\n\r\n{"access_token":...}

    // Find the empty line that separates headers from body
    let body = request
        .split("\r\n\r\n")
        .nth(1)
        .or_else(|| request.split("\n\n").nth(1))
        .ok_or_else(|| anyhow::anyhow!("Invalid callback request: no body"))?;

    // Parse JSON body
    let parsed: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("Failed to parse callback body: {}", e))?;

    // Verify state
    let state = parsed
        .get("state")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing state parameter"))?;

    if state != expected_state {
        return Err(anyhow::anyhow!("State mismatch - possible CSRF attack"));
    }

    // Extract tokens
    let access_token = parsed
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing access token"))?
        .to_string();

    let refresh_token = parsed
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let expires_at = parsed
        .get("expires_at")
        .and_then(|v| v.as_i64());

    Ok(AuthTokens {
        access_token,
        refresh_token,
        expires_at,
        auth_type: "oauth".to_string(),
    })
}

async fn logout() -> anyhow::Result<()> {
    let config_path = Config::global_config_path();

    if config_path.exists() {
        Config::clear_tokens()?;
        println!("{}", "Successfully logged out.".green());
        println!(
            "Credentials removed from {}",
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

    if let Some(ref tokens) = config.tokens {
        let auth_type = match tokens.auth_type.as_str() {
            "oauth" => "OAuth (browser login)",
            "api_key" => "API Key",
            _ => "Unknown",
        };

        println!("  {} {}", "Status:".bold(), "Authenticated".green());
        println!("  {} {}", "Method:".bold(), auth_type.dimmed());
        println!("  {} {}", "API URL:".bold(), config.api_url().dimmed());

        if tokens.auth_type == "oauth" {
            // Note: Access tokens expire hourly but auto-refresh keeps you logged in
            println!(
                "  {} {}",
                "Session:".bold(),
                "Active (auto-refreshing)".green()
            );
        }

        // Try to fetch usage info
        print!("\n{}", "Fetching account info... ".cyan());
        std::io::stdout().flush()?;

        match fetch_user_info(&config).await {
            Ok(email) => {
                println!("{}", "✓".green());
                println!("  {} {}", "Account:".bold(), email);
            }
            Err(_) => {
                println!("{}", "✗".red());
            }
        }
    } else {
        println!("  {} {}", "Status:".bold(), "Not authenticated".red());
        println!();
        println!("Run {} to authenticate.", "vibetap auth login".cyan());
    }

    Ok(())
}

async fn fetch_user_info(config: &Config) -> anyhow::Result<String> {
    let tokens = config.tokens.as_ref().ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/v1/usage", config.api_url()))
        .header("Authorization", format!("Bearer {}", tokens.access_token))
        .send()
        .await?;

    if response.status().is_success() {
        // For now just return a placeholder - we'd need a /me endpoint to get user info
        Ok("Authenticated".to_string())
    } else {
        Err(anyhow::anyhow!("Failed to fetch user info"))
    }
}
