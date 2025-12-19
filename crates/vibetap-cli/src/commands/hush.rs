use clap::Args;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Args)]
pub struct HushArgs {
    /// Duration to silence (e.g., "30m", "1h", "2h", "forever")
    #[arg(default_value = "30m")]
    duration: String,

    /// Show current hush status
    #[arg(long)]
    status: bool,

    /// Clear hush state (resume suggestions)
    #[arg(long)]
    clear: bool,
}

/// Persisted hush state
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HushState {
    /// Unix timestamp when hush expires (None = forever, Some(0) = not hushed)
    pub hush_until: Option<i64>,
}

impl HushState {
    /// Check if currently hushed
    pub fn is_hushed(&self) -> bool {
        match self.hush_until {
            None => true, // Forever
            Some(0) => false,
            Some(until) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                until > now
            }
        }
    }

    /// Get remaining hush time as human-readable string
    pub fn remaining(&self) -> Option<String> {
        match self.hush_until {
            None => Some("forever".to_string()),
            Some(0) => None,
            Some(until) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let remaining = until - now;
                if remaining <= 0 {
                    None
                } else if remaining < 60 {
                    Some(format!("{}s", remaining))
                } else if remaining < 3600 {
                    Some(format!("{}m", remaining / 60))
                } else {
                    Some(format!("{}h {}m", remaining / 3600, (remaining % 3600) / 60))
                }
            }
        }
    }
}

pub async fn execute(args: HushArgs) -> anyhow::Result<()> {
    if args.status {
        return show_status();
    }

    if args.clear {
        return clear_hush();
    }

    // Parse duration and set hush
    let hush_until = if args.duration.to_lowercase() == "forever" {
        None // None = forever
    } else {
        let duration = parse_duration(&args.duration)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Some(now + duration.as_secs() as i64)
    };

    let state = HushState { hush_until };
    save_state(&state)?;

    if args.duration.to_lowercase() == "forever" {
        println!(
            "{}",
            "Suggestions silenced indefinitely.".cyan()
        );
        println!(
            "Run {} to resume.",
            "vibetap hush --clear".cyan()
        );
    } else {
        println!(
            "{}",
            format!("Suggestions silenced for {}.", args.duration).cyan()
        );
        if let Some(remaining) = state.remaining() {
            println!("Will resume in {}.", remaining.green());
        }
    }

    Ok(())
}

fn show_status() -> anyhow::Result<()> {
    let state = load_state()?;

    if state.is_hushed() {
        match state.remaining() {
            Some(remaining) => {
                println!("{} ({})", "Hushed".yellow(), remaining);
            }
            None => {
                println!("{}", "Not hushed".green());
            }
        }
    } else {
        println!("{}", "Not hushed".green());
    }

    Ok(())
}

fn clear_hush() -> anyhow::Result<()> {
    let state = HushState {
        hush_until: Some(0), // 0 = not hushed
    };
    save_state(&state)?;

    println!("{}", "Hush cleared. Suggestions resumed.".green());

    Ok(())
}

fn parse_duration(s: &str) -> anyhow::Result<std::time::Duration> {
    let s = s.trim().to_lowercase();

    // Handle combined format like "1h30m"
    let mut total_secs = 0u64;
    let mut current_num = String::new();

    for c in s.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else {
            if current_num.is_empty() {
                continue;
            }
            let num: u64 = current_num.parse()?;
            current_num.clear();

            match c {
                's' => total_secs += num,
                'm' => total_secs += num * 60,
                'h' => total_secs += num * 3600,
                'd' => total_secs += num * 86400,
                _ => anyhow::bail!("Invalid duration unit: {}. Use s, m, h, or d.", c),
            }
        }
    }

    // If we have leftover digits with no unit, assume minutes
    if !current_num.is_empty() {
        let num: u64 = current_num.parse()?;
        total_secs += num * 60;
    }

    if total_secs == 0 {
        anyhow::bail!("Invalid duration format. Examples: '30m', '1h', '2h30m', '1d'");
    }

    Ok(std::time::Duration::from_secs(total_secs))
}

pub fn load_state() -> anyhow::Result<HushState> {
    let path = Path::new(".vibetap/state.json");
    if !path.exists() {
        return Ok(HushState::default());
    }

    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn save_state(state: &HushState) -> anyhow::Result<()> {
    let vibetap_dir = Path::new(".vibetap");
    if !vibetap_dir.exists() {
        std::fs::create_dir_all(vibetap_dir)?;
    }

    let path = vibetap_dir.join("state.json");
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(path, json)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30m").unwrap().as_secs(), 30 * 60);
        assert_eq!(parse_duration("1h").unwrap().as_secs(), 3600);
        assert_eq!(parse_duration("2h30m").unwrap().as_secs(), 2 * 3600 + 30 * 60);
        assert_eq!(parse_duration("1d").unwrap().as_secs(), 86400);
        assert_eq!(parse_duration("30s").unwrap().as_secs(), 30);
    }
}
