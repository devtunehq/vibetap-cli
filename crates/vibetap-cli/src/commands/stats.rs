use clap::Args;
use colored::Colorize;

use vibetap_core::{ApiClient, Config};

#[derive(Args)]
pub struct StatsArgs {
    /// Show raw JSON output
    #[arg(long)]
    json: bool,
}

pub async fn execute(args: StatsArgs) -> anyhow::Result<()> {
    // Load configuration
    let mut config = Config::load()?;
    let access_token = config.get_valid_access_token().await?;
    let api_url = config.api_url().to_string();

    // Fetch stats from API
    let client = ApiClient::new(api_url, access_token);
    let stats = match client.get_stats().await {
        Ok(s) => s,
        Err(e) => {
            println!("{} {}", "Error:".red(), e);
            return Ok(());
        }
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "thisMonth": {
                    "generations": stats.this_month.generations,
                    "remaining": stats.this_month.remaining,
                    "limit": stats.this_month.limit,
                    "securityIssuesCaught": stats.this_month.security_issues_caught,
                    "testsApplied": stats.this_month.tests_applied,
                    "acceptanceRate": stats.this_month.acceptance_rate,
                },
                "allTime": {
                    "totalGenerations": stats.all_time.total_generations,
                    "totalSecurityIssues": stats.all_time.total_security_issues,
                    "totalTestsApplied": stats.all_time.total_tests_applied,
                    "topFramework": stats.all_time.top_framework,
                },
                "plan": {
                    "name": stats.plan.name,
                    "generationsPerMonth": stats.plan.generations_per_month,
                    "creditsBalance": stats.plan.credits_balance,
                },
                "byok": stats.byok.as_ref().map(|b| serde_json::json!({
                    "enabled": b.enabled,
                    "totalRequests": b.total_requests,
                }))
            }))?
        );
        return Ok(());
    }

    // Display formatted stats
    println!();
    println!("{}", "═══ VibeTap Stats ═══".bold().cyan());
    println!();

    // This month section
    println!("{}", "This Month".bold());
    let usage_pct = if stats.this_month.limit > 0 {
        (stats.this_month.generations as f64 / stats.this_month.limit as f64 * 100.0) as u32
    } else {
        0
    };

    // Progress bar
    let bar_width = 30;
    let filled = (bar_width * usage_pct / 100).min(bar_width);
    let empty = bar_width - filled;
    let bar = format!(
        "[{}{}]",
        "█".repeat(filled as usize).green(),
        "░".repeat(empty as usize).dimmed()
    );

    println!(
        "  {} generations used {} {}%",
        stats.this_month.generations.to_string().green(),
        bar,
        usage_pct
    );
    println!(
        "  {} remaining of {} limit",
        stats.this_month.remaining.to_string().yellow(),
        stats.this_month.limit
    );

    if stats.this_month.tests_applied > 0 {
        println!(
            "  Tests applied: {} ({}% acceptance rate)",
            stats.this_month.tests_applied.to_string().green(),
            (stats.this_month.acceptance_rate * 100.0) as u32
        );
    }

    if stats.this_month.security_issues_caught > 0 {
        println!(
            "  Security issues caught: {}",
            stats.this_month.security_issues_caught.to_string().red()
        );
    }

    println!();

    // All time section
    if stats.all_time.total_generations > 0 {
        println!("{}", "All Time".bold());
        println!(
            "  Total generations: {}",
            stats.all_time.total_generations.to_string().cyan()
        );
        println!(
            "  Total tests applied: {}",
            stats.all_time.total_tests_applied.to_string().green()
        );
        if stats.all_time.total_security_issues > 0 {
            println!(
                "  Security issues caught: {}",
                stats.all_time.total_security_issues.to_string().red()
            );
        }
        if let Some(ref framework) = stats.all_time.top_framework {
            println!("  Top framework: {}", framework.cyan());
        }
        println!();
    }

    // Plan info
    println!("{}", "Plan".bold());
    println!("  {}", stats.plan.name.cyan());
    println!(
        "  {} generations/month",
        stats.plan.generations_per_month
    );
    if stats.plan.credits_balance > 0 {
        println!(
            "  Credits balance: {}",
            stats.plan.credits_balance.to_string().green()
        );
    }

    // BYOK info
    if let Some(ref byok) = stats.byok {
        println!();
        println!("{}", "BYOK (Bring Your Own Key)".bold());
        if byok.enabled {
            println!("  Status: {}", "Enabled".green());
            println!(
                "  Total BYOK requests: {}",
                byok.total_requests.to_string().cyan()
            );
        } else {
            println!("  Status: {}", "Not configured".dimmed());
        }
    }

    println!();

    Ok(())
}
