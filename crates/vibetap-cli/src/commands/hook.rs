use clap::{Args, Subcommand};
use colored::Colorize;
use std::fs;
use std::os::unix::fs::PermissionsExt;

const PRE_COMMIT_HOOK_MARKER: &str = "# VibeTap pre-commit hook";

#[derive(Args)]
pub struct HookArgs {
    #[command(subcommand)]
    command: HookCommand,
}

#[derive(Subcommand)]
enum HookCommand {
    /// Install the VibeTap pre-commit hook
    Install(InstallArgs),

    /// Remove the VibeTap pre-commit hook
    Uninstall,

    /// Check if VibeTap pre-commit hook is installed
    Status,
}

#[derive(Args)]
struct InstallArgs {
    /// Block commits when test suggestions are available
    #[arg(long)]
    block: bool,

    /// Only show warnings for security-related suggestions
    #[arg(long)]
    security_only: bool,
}

pub async fn execute(args: HookArgs) -> anyhow::Result<()> {
    match args.command {
        HookCommand::Install(install_args) => install(install_args),
        HookCommand::Uninstall => uninstall(),
        HookCommand::Status => status(),
    }
}

fn get_git_hooks_dir() -> anyhow::Result<std::path::PathBuf> {
    // Find .git directory
    let mut current = std::env::current_dir()?;

    loop {
        let git_dir = current.join(".git");
        if git_dir.exists() {
            return Ok(git_dir.join("hooks"));
        }
        if !current.pop() {
            anyhow::bail!("Not a git repository. Run this command from within a git repo.");
        }
    }
}

fn install(args: InstallArgs) -> anyhow::Result<()> {
    let hooks_dir = get_git_hooks_dir()?;

    // Create hooks directory if it doesn't exist
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir)?;
    }

    let pre_commit_path = hooks_dir.join("pre-commit");

    // Check if a pre-commit hook already exists
    let existing_hook = if pre_commit_path.exists() {
        Some(fs::read_to_string(&pre_commit_path)?)
    } else {
        None
    };

    // Check if VibeTap hook is already installed
    if let Some(ref content) = existing_hook {
        if content.contains(PRE_COMMIT_HOOK_MARKER) {
            println!("{}", "VibeTap hook is already installed.".yellow());
            println!(
                "Run {} to reinstall with different options.",
                "vibetap hook uninstall && vibetap hook install".cyan()
            );
            return Ok(());
        }
    }

    // Build the vibetap command
    let mut vibetap_cmd = "vibetap now --staged --quiet".to_string();
    if args.security_only {
        vibetap_cmd.push_str(" --security");
    }

    // Generate the hook script
    let hook_script = if args.block {
        generate_blocking_hook(&vibetap_cmd)
    } else {
        generate_non_blocking_hook(&vibetap_cmd)
    };

    // If there's an existing hook, append to it
    let final_script = if let Some(existing) = existing_hook {
        if existing.starts_with("#!/") {
            // Append our hook to the existing one
            format!("{}\n\n{}", existing.trim_end(), hook_script)
        } else {
            // Existing hook doesn't have a shebang, prepend one
            format!("#!/bin/sh\n{}\n\n{}", existing.trim_end(), hook_script)
        }
    } else {
        format!("#!/bin/sh\n{}", hook_script)
    };

    // Write the hook
    fs::write(&pre_commit_path, final_script)?;

    // Make it executable
    let mut perms = fs::metadata(&pre_commit_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&pre_commit_path, perms)?;

    println!("{}", "✓ VibeTap pre-commit hook installed!".green());
    println!();

    if args.block {
        println!(
            "{}",
            "Mode: Blocking - commits will be prevented when test suggestions are available."
                .dimmed()
        );
        println!(
            "{}",
            "Use --no-verify to bypass the hook when needed.".dimmed()
        );
    } else {
        println!(
            "{}",
            "Mode: Advisory - you'll see suggestions but commits won't be blocked.".dimmed()
        );
    }

    if args.security_only {
        println!(
            "{}",
            "Filter: Security-only - only security-related suggestions will trigger warnings."
                .dimmed()
        );
    }

    println!();
    println!(
        "The hook will run {} before each commit.",
        "vibetap now".cyan()
    );
    println!(
        "Run {} to remove the hook.",
        "vibetap hook uninstall".cyan()
    );

    Ok(())
}

fn uninstall() -> anyhow::Result<()> {
    let hooks_dir = get_git_hooks_dir()?;
    let pre_commit_path = hooks_dir.join("pre-commit");

    if !pre_commit_path.exists() {
        println!("{}", "No pre-commit hook found.".yellow());
        return Ok(());
    }

    let content = fs::read_to_string(&pre_commit_path)?;

    if !content.contains(PRE_COMMIT_HOOK_MARKER) {
        println!("{}", "VibeTap hook is not installed.".yellow());
        return Ok(());
    }

    // Remove VibeTap section from the hook
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines: Vec<&str> = Vec::new();
    let mut in_vibetap_section = false;

    for line in lines {
        if line.contains(PRE_COMMIT_HOOK_MARKER) {
            in_vibetap_section = true;
            continue;
        }
        if in_vibetap_section && line.contains("# End VibeTap hook") {
            in_vibetap_section = false;
            continue;
        }
        if !in_vibetap_section {
            new_lines.push(line);
        }
    }

    // Clean up empty lines at the end
    while new_lines.last() == Some(&"") {
        new_lines.pop();
    }

    let remaining = new_lines.join("\n");

    // If only shebang remains (or empty), remove the file entirely
    if remaining.trim().is_empty() || remaining.trim() == "#!/bin/sh" {
        fs::remove_file(&pre_commit_path)?;
        println!("{}", "✓ VibeTap pre-commit hook removed.".green());
    } else {
        fs::write(&pre_commit_path, format!("{}\n", remaining))?;
        println!(
            "{}",
            "✓ VibeTap section removed from pre-commit hook.".green()
        );
        println!(
            "{}",
            "Other pre-commit hooks remain installed.".dimmed()
        );
    }

    Ok(())
}

fn status() -> anyhow::Result<()> {
    let hooks_dir = match get_git_hooks_dir() {
        Ok(dir) => dir,
        Err(_) => {
            println!("{}", "Not a git repository.".yellow());
            return Ok(());
        }
    };

    let pre_commit_path = hooks_dir.join("pre-commit");

    if !pre_commit_path.exists() {
        println!("{}", "VibeTap pre-commit hook: Not installed".yellow());
        println!(
            "Run {} to install.",
            "vibetap hook install".cyan()
        );
        return Ok(());
    }

    let content = fs::read_to_string(&pre_commit_path)?;

    if content.contains(PRE_COMMIT_HOOK_MARKER) {
        println!("{}", "VibeTap pre-commit hook: Installed ✓".green());

        // Detect mode
        if content.contains("exit $result") {
            println!("  Mode: Blocking (prevents commits when suggestions available)");
        } else {
            println!("  Mode: Advisory (shows suggestions but allows commits)");
        }

        if content.contains("--security") {
            println!("  Filter: Security-only");
        }

        println!();
        println!(
            "Run {} to remove.",
            "vibetap hook uninstall".cyan()
        );
    } else {
        println!("{}", "VibeTap pre-commit hook: Not installed".yellow());
        println!(
            "{}",
            "A pre-commit hook exists but doesn't include VibeTap.".dimmed()
        );
        println!(
            "Run {} to add VibeTap to it.",
            "vibetap hook install".cyan()
        );
    }

    Ok(())
}

fn generate_non_blocking_hook(vibetap_cmd: &str) -> String {
    format!(
        r#"
{marker}
# Shows test suggestions before commit (advisory only)
if command -v vibetap >/dev/null 2>&1; then
    {cmd} || true
fi
# End VibeTap hook
"#,
        marker = PRE_COMMIT_HOOK_MARKER,
        cmd = vibetap_cmd
    )
}

fn generate_blocking_hook(vibetap_cmd: &str) -> String {
    format!(
        r#"
{marker}
# Shows test suggestions and blocks commit if suggestions are available
if command -v vibetap >/dev/null 2>&1; then
    output=$({cmd} 2>&1)
    result=$?
    if [ -n "$output" ]; then
        echo "$output"
        echo ""
        echo "Commit blocked: Test suggestions available."
        echo "Run 'vibetap apply' to add tests, or commit with --no-verify to skip."
        exit 1
    fi
fi
# End VibeTap hook
"#,
        marker = PRE_COMMIT_HOOK_MARKER,
        cmd = vibetap_cmd
    )
}
