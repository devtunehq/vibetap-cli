use clap::Args;
use colored::Colorize;

#[derive(Args)]
pub struct HushArgs {
    /// Duration to silence (e.g., "30m", "1h", "2h")
    #[arg(default_value = "30m")]
    duration: String,
}

pub async fn execute(args: HushArgs) -> anyhow::Result<()> {
    let duration = parse_duration(&args.duration)?;
    println!(
        "{}",
        format!("Silencing suggestions for {}...", args.duration).cyan()
    );

    // TODO: Write hush state to .aitest/state.json
    let hush_until = std::time::SystemTime::now() + duration;

    println!(
        "{}",
        format!(
            "Suggestions silenced until {:?}",
            hush_until
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        )
        .green()
    );
    println!("Run 'vibetap watch' to resume watching when ready.");

    Ok(())
}

fn parse_duration(s: &str) -> anyhow::Result<std::time::Duration> {
    let s = s.trim().to_lowercase();

    if let Some(mins) = s.strip_suffix('m') {
        let mins: u64 = mins.parse()?;
        return Ok(std::time::Duration::from_secs(mins * 60));
    }

    if let Some(hours) = s.strip_suffix('h') {
        let hours: u64 = hours.parse()?;
        return Ok(std::time::Duration::from_secs(hours * 3600));
    }

    anyhow::bail!("Invalid duration format. Use '30m' or '1h'.");
}
