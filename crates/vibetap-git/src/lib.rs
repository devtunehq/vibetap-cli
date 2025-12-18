//! VibeTap Git Integration
//!
//! Git operations for VibeTap including:
//! - Staged diff detection
//! - Commit history analysis
//! - File status tracking

use git2::{Diff, DiffFormat, DiffOptions, Repository, StatusOptions};
use std::cell::RefCell;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("Not a git repository")]
    NotARepo,

    #[error("No staged changes")]
    NoStagedChanges,
}

/// Represents a parsed diff hunk
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub file_path: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub content: String,
}

/// Represents the staged diff
#[derive(Debug)]
pub struct StagedDiff {
    pub hunks: Vec<DiffHunk>,
    pub files_changed: Vec<String>,
}

/// Parse a git2 Diff into our StagedDiff structure
fn parse_diff(diff: &Diff) -> Result<StagedDiff, GitError> {
    let hunks = RefCell::new(Vec::new());
    let files_changed = RefCell::new(Vec::new());
    let current_file = RefCell::new(String::new());

    diff.print(DiffFormat::Patch, |delta, hunk, line| {
        // Track file changes
        if let Some(path) = delta.new_file().path() {
            let path_str = path.to_string_lossy().to_string();
            let mut files = files_changed.borrow_mut();
            if !files.contains(&path_str) {
                files.push(path_str.clone());
            }
            *current_file.borrow_mut() = path_str;
        }

        // When we see a hunk header, create a new hunk
        if let Some(h) = hunk {
            let file_path = current_file.borrow().clone();
            hunks.borrow_mut().push(DiffHunk {
                file_path,
                old_start: h.old_start(),
                old_lines: h.old_lines(),
                new_start: h.new_start(),
                new_lines: h.new_lines(),
                content: String::new(),
            });
        }

        // Append line content to the current hunk
        let origin = line.origin();
        if matches!(origin, '+' | '-' | ' ') {
            if let Ok(content) = std::str::from_utf8(line.content()) {
                if let Some(last_hunk) = hunks.borrow_mut().last_mut() {
                    last_hunk.content.push(origin);
                    last_hunk.content.push_str(content);
                }
            }
        }

        true
    })?;

    let hunks = hunks.into_inner();
    let files_changed = files_changed.into_inner();

    if hunks.is_empty() {
        return Err(GitError::NoStagedChanges);
    }

    Ok(StagedDiff {
        hunks,
        files_changed,
    })
}

/// Get the staged diff from the current repository
pub fn get_staged_diff() -> Result<StagedDiff, GitError> {
    let repo = Repository::open_from_env().map_err(|_| GitError::NotARepo)?;

    let head = repo.head()?.peel_to_tree()?;
    let index = repo.index()?;

    let mut opts = DiffOptions::new();
    opts.include_untracked(false);

    let diff = repo.diff_tree_to_index(Some(&head), Some(&index), Some(&mut opts))?;

    parse_diff(&diff)
}

/// Get uncommitted changes (staged + unstaged)
pub fn get_uncommitted_diff() -> Result<StagedDiff, GitError> {
    let repo = Repository::open_from_env().map_err(|_| GitError::NotARepo)?;

    let head = repo.head()?.peel_to_tree()?;

    let mut opts = DiffOptions::new();
    opts.include_untracked(true);

    let diff = repo.diff_tree_to_workdir_with_index(Some(&head), Some(&mut opts))?;

    parse_diff(&diff)
}

/// Check if there are any staged changes
pub fn has_staged_changes() -> Result<bool, GitError> {
    let repo = Repository::open_from_env().map_err(|_| GitError::NotARepo)?;

    let mut opts = StatusOptions::new();
    opts.include_untracked(false);

    let statuses = repo.statuses(Some(&mut opts))?;

    Ok(statuses.iter().any(|s| {
        s.status().intersects(
            git2::Status::INDEX_NEW
                | git2::Status::INDEX_MODIFIED
                | git2::Status::INDEX_DELETED
                | git2::Status::INDEX_RENAMED
                | git2::Status::INDEX_TYPECHANGE,
        )
    }))
}
