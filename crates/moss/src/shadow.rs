//! Shadow Git - automatic edit history tracking.
//!
//! Maintains a hidden git repository (`.moss/shadow/`) that automatically
//! commits after each `moss edit` operation, preserving full edit history.

use rhizome_moss_derive::Merge;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A single entry in shadow git history.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub id: usize,
    pub hash: String,
    pub subject: String,
    pub operation: String,
    pub target: String,
    pub files: Vec<String>,
    pub message: Option<String>,
    pub workflow: Option<String>,
    pub git_head: String,
    pub timestamp: String,
}

/// Shadow git configuration.
#[derive(Debug, Clone, Deserialize, Default, Merge)]
#[serde(default)]
pub struct ShadowConfig {
    /// Whether shadow git is enabled. Default: true
    pub enabled: Option<bool>,
    /// Confirm before deleting symbols. Default: true
    pub warn_on_delete: Option<bool>,
}

impl ShadowConfig {
    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn warn_on_delete(&self) -> bool {
        self.warn_on_delete.unwrap_or(true)
    }
}

/// Information about an edit operation for shadow commit.
pub struct EditInfo {
    pub operation: String,
    pub target: String,
    pub files: Vec<PathBuf>,
    pub message: Option<String>,
    pub workflow: Option<String>,
}

/// Result of running a validation command in shadow worktree.
pub struct ValidationResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Shadow git repository manager.
pub struct Shadow {
    /// Root of the project (where .moss/ lives)
    root: PathBuf,
    /// Path to shadow git directory (.moss/shadow/)
    shadow_dir: PathBuf,
    /// Path to shadow worktree (.moss/shadow/worktree/)
    worktree: PathBuf,
}

impl Shadow {
    /// Create a new Shadow instance for a project root.
    pub fn new(root: &Path) -> Self {
        let shadow_dir = root.join(".moss").join("shadow");
        let worktree = shadow_dir.join("worktree");
        Self {
            root: root.to_path_buf(),
            shadow_dir,
            worktree,
        }
    }

    /// Check if shadow git exists for this project.
    pub fn exists(&self) -> bool {
        self.shadow_dir.join(".git").exists()
    }

    /// Initialize shadow git repository if it doesn't exist.
    /// Called on first edit, not on `moss init`.
    fn init(&self) -> Result<(), ShadowError> {
        if self.exists() {
            return Ok(());
        }

        // Create worktree directory (git init will create .git inside shadow_dir)
        std::fs::create_dir_all(&self.worktree)
            .map_err(|e| ShadowError::Init(format!("Failed to create shadow directory: {}", e)))?;

        // Initialize git repo with worktree in subdirectory
        // Use --separate-git-dir to put .git in shadow_dir while worktree is in worktree/
        let status = Command::new("git")
            .args([
                "init",
                "--quiet",
                &format!(
                    "--separate-git-dir={}",
                    self.shadow_dir.join(".git").display()
                ),
            ])
            .current_dir(&self.worktree)
            .status()
            .map_err(|e| ShadowError::Init(format!("Failed to run git init: {}", e)))?;

        if !status.success() {
            return Err(ShadowError::Init("git init failed".to_string()));
        }

        // Configure git user for commits (shadow-specific, doesn't affect user's git)
        let _ = Command::new("git")
            .args(["config", "user.email", "shadow@moss.local"])
            .current_dir(&self.worktree)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.name", "Moss Shadow"])
            .current_dir(&self.worktree)
            .status();

        Ok(())
    }

    /// Get the current git HEAD of the real repository.
    fn get_real_git_head(&self) -> Option<String> {
        let output = Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(&self.root)
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    /// Copy a file to the shadow worktree, preserving relative path.
    fn copy_to_worktree(&self, file: &Path) -> Result<PathBuf, ShadowError> {
        let rel_path = file
            .strip_prefix(&self.root)
            .map_err(|_| ShadowError::Commit("File not under project root".to_string()))?;

        let dest = self.worktree.join(rel_path);

        // Create parent directories
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ShadowError::Commit(format!("Failed to create directories: {}", e)))?;
        }

        // Copy file
        std::fs::copy(file, &dest)
            .map_err(|e| ShadowError::Commit(format!("Failed to copy file: {}", e)))?;

        Ok(rel_path.to_path_buf())
    }

    /// Record file state before an edit.
    /// Call this before applying the edit to capture "before" state.
    pub fn before_edit(&self, files: &[&Path]) -> Result<(), ShadowError> {
        self.init()?;

        for file in files {
            if file.exists() {
                self.copy_to_worktree(file)?;
            }
        }

        Ok(())
    }

    /// Record file state after an edit and commit.
    /// Call this after applying the edit to capture "after" state.
    pub fn after_edit(&self, info: &EditInfo) -> Result<(), ShadowError> {
        // Copy updated files to worktree
        for file in &info.files {
            if file.exists() {
                self.copy_to_worktree(file)?;
            }
        }

        // Stage all changes (run in worktree directory)
        let status = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.worktree)
            .status()
            .map_err(|e| ShadowError::Commit(format!("Failed to stage changes: {}", e)))?;

        if !status.success() {
            return Err(ShadowError::Commit("git add failed".to_string()));
        }

        // Check if there are changes to commit
        let status = Command::new("git")
            .args(["diff", "--cached", "--quiet"])
            .current_dir(&self.worktree)
            .status()
            .map_err(|e| ShadowError::Commit(format!("Failed to check diff: {}", e)))?;

        if status.success() {
            // No changes to commit
            return Ok(());
        }

        // Build commit message
        let git_head = self
            .get_real_git_head()
            .unwrap_or_else(|| "none".to_string());
        let files_str: Vec<String> = info
            .files
            .iter()
            .filter_map(|f| f.strip_prefix(&self.root).ok())
            .map(|p| p.display().to_string())
            .collect();

        let mut commit_msg = format!("moss edit: {} {}\n\n", info.operation, info.target);

        if let Some(ref msg) = info.message {
            commit_msg.push_str(&format!("Message: {}\n", msg));
        }
        if let Some(ref wf) = info.workflow {
            commit_msg.push_str(&format!("Workflow: {}\n", wf));
        }
        commit_msg.push_str(&format!("Operation: {}\n", info.operation));
        commit_msg.push_str(&format!("Target: {}\n", info.target));
        commit_msg.push_str(&format!("Files: {}\n", files_str.join(", ")));
        commit_msg.push_str(&format!("Git-HEAD: {}\n", git_head));

        // Commit
        let status = Command::new("git")
            .args(["commit", "-m", &commit_msg])
            .current_dir(&self.worktree)
            .status()
            .map_err(|e| ShadowError::Commit(format!("Failed to commit: {}", e)))?;

        if !status.success() {
            return Err(ShadowError::Commit("git commit failed".to_string()));
        }

        Ok(())
    }

    /// Get history of shadow edits.
    /// Returns list of edits in reverse chronological order (newest first).
    pub fn history(&self, file_filter: Option<&str>, limit: usize) -> Vec<HistoryEntry> {
        if !self.exists() {
            return Vec::new();
        }

        // Get git log with custom format
        // Use %x1e (record separator) between commits and %x1f (unit separator) between fields
        let mut args = vec![
            "log".to_string(),
            "--format=%H%x1f%s%x1f%b%x1f%aI%x1e".to_string(),
            format!("-{}", limit),
        ];

        // Filter by file if specified
        if let Some(file) = file_filter {
            args.push("--".to_string());
            args.push(file.to_string());
        }

        let output = Command::new("git")
            .args(&args)
            .current_dir(&self.worktree)
            .output();

        let output = match output {
            Ok(out) if out.status.success() => out,
            _ => return Vec::new(),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();

        // Split by record separator (0x1e)
        let blocks: Vec<&str> = stdout
            .split('\x1e')
            .filter(|b| !b.trim().is_empty())
            .collect();
        let total = blocks.len();

        for (idx, block) in blocks.into_iter().enumerate() {
            // Parse the commit format: hash\x1fsubject\x1fbody\x1ftimestamp
            let parts: Vec<&str> = block.split('\x1f').collect();
            if parts.len() < 4 {
                continue;
            }

            let hash = parts[0].trim();
            let subject = parts[1].trim();
            let body = parts[2].trim();
            let timestamp = parts[3].trim();

            // Parse body for structured fields
            let mut operation = String::new();
            let mut target = String::new();
            let mut files = Vec::new();
            let mut message = None;
            let mut workflow = None;
            let mut git_head = String::new();

            for line in body.lines() {
                if let Some(val) = line.strip_prefix("Operation: ") {
                    operation = val.to_string();
                } else if let Some(val) = line.strip_prefix("Target: ") {
                    target = val.to_string();
                } else if let Some(val) = line.strip_prefix("Files: ") {
                    files = val.split(", ").map(String::from).collect();
                } else if let Some(val) = line.strip_prefix("Message: ") {
                    message = Some(val.to_string());
                } else if let Some(val) = line.strip_prefix("Workflow: ") {
                    workflow = Some(val.to_string());
                } else if let Some(val) = line.strip_prefix("Git-HEAD: ") {
                    git_head = val.to_string();
                }
            }

            entries.push(HistoryEntry {
                id: total - idx, // newest first, so first entry gets highest ID
                hash: hash.to_string(),
                subject: subject.to_string(),
                operation,
                target,
                files,
                message,
                workflow,
                git_head,
                timestamp: timestamp.to_string(),
            });
        }

        entries
    }

    /// Get diff for a specific commit.
    pub fn diff(&self, commit_ref: &str) -> Option<String> {
        if !self.exists() {
            return None;
        }

        let output = Command::new("git")
            .args(["show", "--format=", commit_ref])
            .current_dir(&self.worktree)
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            None
        }
    }

    /// Get tree view of shadow history (shows all branches with graph).
    pub fn tree(&self, limit: usize) -> Option<String> {
        if !self.exists() {
            return None;
        }

        let output = Command::new("git")
            .args([
                "log",
                "--graph",
                "--all",
                "--oneline",
                "--decorate",
                &format!("-{}", limit),
            ])
            .current_dir(&self.worktree)
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            None
        }
    }

    /// Get current checkpoint (last git commit in real repo when shadow was updated).
    pub fn checkpoint(&self) -> Option<String> {
        self.history(None, 1)
            .first()
            .map(|e| e.git_head.clone())
            .filter(|h| h != "none")
    }

    /// Run a validation command in the shadow worktree.
    /// Returns (success, stdout, stderr).
    /// Used by agents to test changes before applying to real files.
    pub fn validate(&self, cmd: &str, args: &[&str]) -> Result<ValidationResult, ShadowError> {
        if !self.exists() {
            return Err(ShadowError::Init("No shadow worktree exists".to_string()));
        }

        let output = Command::new(cmd)
            .args(args)
            .current_dir(&self.worktree)
            .output()
            .map_err(|e| ShadowError::Validation(format!("Failed to run {}: {}", cmd, e)))?;

        Ok(ValidationResult {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Apply pending shadow changes to the real worktree.
    /// Only call this after validation passes.
    /// Returns list of files that were updated.
    pub fn apply_to_real(&self) -> Result<Vec<PathBuf>, ShadowError> {
        if !self.exists() {
            return Err(ShadowError::Init("No shadow worktree exists".to_string()));
        }

        // Get list of changed files in shadow
        let output = Command::new("git")
            .args(["diff", "--name-only", "HEAD~1", "HEAD"])
            .current_dir(&self.worktree)
            .output()
            .map_err(|e| ShadowError::Validation(format!("git diff failed: {}", e)))?;

        if !output.status.success() {
            return Err(ShadowError::Validation(
                "Failed to get changed files".to_string(),
            ));
        }

        let files: Vec<PathBuf> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| self.root.join(l))
            .collect();

        // Copy each file from shadow to real
        for file in &files {
            let rel = file.strip_prefix(&self.root).unwrap_or(file.as_path());
            let shadow_file = self.worktree.join(rel);
            if shadow_file.exists() {
                if let Some(parent) = file.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| ShadowError::Validation(format!("mkdir failed: {}", e)))?;
                }
                std::fs::copy(&shadow_file, file)
                    .map_err(|e| ShadowError::Validation(format!("copy failed: {}", e)))?;
            }
        }

        Ok(files)
    }

    /// Get the number of shadow commits (edits tracked).
    pub fn edit_count(&self) -> usize {
        if !self.exists() {
            return 0;
        }

        let output = Command::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .current_dir(&self.worktree)
            .output();

        match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse()
                .unwrap_or(0),
            _ => 0,
        }
    }

    /// Prune shadow history, keeping only the last N commits.
    /// Returns the number of commits pruned.
    pub fn prune(&self, keep: usize) -> Result<usize, ShadowError> {
        if !self.exists() {
            return Err(ShadowError::Init("No shadow history exists".to_string()));
        }

        let total = self.edit_count();
        if total <= keep {
            return Ok(0);
        }

        let to_prune = total - keep;

        // Find the commit that will become the new root (the `keep`th commit from HEAD)
        let new_root_output = Command::new("git")
            .args(["rev-parse", &format!("HEAD~{}", keep - 1)])
            .current_dir(&self.worktree)
            .output()
            .map_err(|e| ShadowError::Init(format!("Failed to find root commit: {}", e)))?;

        if !new_root_output.status.success() {
            return Err(ShadowError::Init(
                "Failed to find commit to keep".to_string(),
            ));
        }

        let new_root = String::from_utf8_lossy(&new_root_output.stdout)
            .trim()
            .to_string();

        // Create a graft to make the new root appear as an initial commit
        let _ = Command::new("git")
            .args(["replace", "--graft", &new_root])
            .current_dir(&self.worktree)
            .output();

        // Use filter-branch to bake in the graft (rewrite history)
        let filter_result = Command::new("git")
            .args(["filter-branch", "--force", "--", "--all"])
            .current_dir(&self.worktree)
            .output();

        if let Err(e) = filter_result {
            return Err(ShadowError::Init(format!("Filter-branch failed: {}", e)));
        }

        // Clean up refs created by filter-branch
        let _ = Command::new("git")
            .args(["for-each-ref", "--format=%(refname)", "refs/original/"])
            .current_dir(&self.worktree)
            .output()
            .map(|out| {
                for refname in String::from_utf8_lossy(&out.stdout).lines() {
                    let _ = Command::new("git")
                        .args(["update-ref", "-d", refname])
                        .current_dir(&self.worktree)
                        .output();
                }
            });

        // Remove the replacement ref
        let _ = Command::new("git")
            .args(["replace", "-d", &new_root])
            .current_dir(&self.worktree)
            .output();

        // Run gc to actually free space
        let _ = Command::new("git")
            .args(["gc", "--prune=now", "--aggressive"])
            .current_dir(&self.worktree)
            .output();

        Ok(to_prune)
    }

    /// Undo the most recent edit (or specified number of edits).
    /// Returns information about what was undone.
    ///
    /// If `file_filter` is Some, only undo changes to files matching that path.
    /// If `force` is false, checks for external modifications first and fails
    /// if any files have been modified outside of moss.
    ///
    /// If `cross_checkpoint` is false, refuses to undo past a git commit boundary.
    pub fn undo(
        &self,
        count: usize,
        file_filter: Option<&str>,
        cross_checkpoint: bool,
        dry_run: bool,
        force: bool,
    ) -> Result<Vec<UndoResult>, ShadowError> {
        if !self.exists() {
            return Err(ShadowError::Undo("No shadow history exists".to_string()));
        }

        let entries = self.history(None, count);
        if entries.is_empty() {
            return Err(ShadowError::Undo("No edits to undo".to_string()));
        }

        // Filter entries to only those affecting the specified file
        let entries: Vec<_> = if let Some(filter) = file_filter {
            entries
                .into_iter()
                .filter(|e| e.files.iter().any(|f| f.contains(filter) || f == filter))
                .collect()
        } else {
            entries
        };

        if entries.is_empty() {
            return Err(ShadowError::Undo(
                "No edits found matching the file filter".to_string(),
            ));
        }

        // Check for checkpoint boundaries (git commit changes) unless cross_checkpoint is set
        if !cross_checkpoint && entries.len() > 1 {
            let first_git_head = &entries[0].git_head;
            for entry in entries.iter().skip(1) {
                if entry.git_head != *first_git_head && entry.git_head != "none" {
                    return Err(ShadowError::Undo(format!(
                        "Cannot undo past checkpoint (git commit {}). Use --cross-checkpoint to override.",
                        entry.git_head
                    )));
                }
            }
        }

        // Check for external modifications unless force is set
        if !force && !dry_run {
            let conflicts = self.detect_conflicts(&entries);
            if !conflicts.is_empty() {
                let files_str = conflicts.join(", ");
                return Err(ShadowError::Undo(format!(
                    "Files modified externally since last edit: {}. Use --force to override.",
                    files_str
                )));
            }
        }

        let mut results = Vec::new();

        for entry in entries.iter().take(count) {
            // Filter files to only those matching the filter
            let files_to_undo: Vec<_> = if let Some(filter) = file_filter {
                entry
                    .files
                    .iter()
                    .filter(|f| f.contains(filter) || *f == filter)
                    .cloned()
                    .collect()
            } else {
                entry.files.clone()
            };

            if dry_run {
                // Also report conflicts in dry-run mode
                let conflicts = self.detect_conflicts(&[entry.clone()]);
                results.push(UndoResult {
                    files: files_to_undo.iter().map(PathBuf::from).collect(),
                    undone_commit: entry.hash.clone(),
                    description: format!("{}: {}", entry.operation, entry.target),
                    conflicts,
                });
                continue;
            }

            // For each file in the commit, restore from the parent commit state
            let parent_ref = format!("{}^", entry.hash);

            for file_path in &files_to_undo {
                let worktree_file = self.worktree.join(file_path);
                let actual_file = self.root.join(file_path);

                // Try to get the file content from parent commit
                let show_output = Command::new("git")
                    .args(["show", &format!("{}:{}", parent_ref, file_path)])
                    .current_dir(&self.worktree)
                    .output();

                match show_output {
                    Ok(output) if output.status.success() => {
                        // File existed in parent - restore it
                        if let Some(parent) = actual_file.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::write(&actual_file, &output.stdout).map_err(|e| {
                            ShadowError::Undo(format!("Failed to write {}: {}", file_path, e))
                        })?;
                        // Update worktree too
                        if let Some(parent) = worktree_file.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(&worktree_file, &output.stdout);
                    }
                    _ => {
                        // File didn't exist in parent (was added) - delete it
                        if actual_file.exists() {
                            std::fs::remove_file(&actual_file).map_err(|e| {
                                ShadowError::Undo(format!("Failed to delete {}: {}", file_path, e))
                            })?;
                        }
                        let _ = std::fs::remove_file(&worktree_file);
                    }
                }
            }

            // Stage and commit the undo
            let _ = Command::new("git")
                .args(["add", "-A"])
                .current_dir(&self.worktree)
                .status();

            let undo_msg = format!(
                "moss edit: undo {}\n\nOperation: undo\nTarget: {}\nUndone-Commit: {}\nFiles: {}\nGit-HEAD: {}\n",
                entry.target,
                entry.target,
                entry.hash,
                files_to_undo.join(", "),
                self.get_real_git_head()
                    .unwrap_or_else(|| "none".to_string())
            );

            let _ = Command::new("git")
                .args(["commit", "-m", &undo_msg, "--allow-empty"])
                .current_dir(&self.worktree)
                .status();

            results.push(UndoResult {
                files: files_to_undo.iter().map(PathBuf::from).collect(),
                undone_commit: entry.hash.clone(),
                description: format!("{}: {}", entry.operation, entry.target),
                conflicts: vec![], // Already checked/forced above
            });
        }

        Ok(results)
    }

    /// Detect files that have been modified externally since last moss edit.
    /// Returns list of file paths that differ between actual filesystem and shadow git HEAD.
    fn detect_conflicts(&self, entries: &[HistoryEntry]) -> Vec<String> {
        let mut conflicts = Vec::new();

        for entry in entries {
            for file_path in &entry.files {
                let actual_file = self.root.join(file_path);

                // Get expected content from shadow git HEAD
                let show_output = Command::new("git")
                    .args(["show", &format!("HEAD:{}", file_path)])
                    .current_dir(&self.worktree)
                    .output();

                match show_output {
                    Ok(output) if output.status.success() => {
                        // File exists in shadow - compare with actual
                        if actual_file.exists() {
                            if let Ok(actual_content) = std::fs::read(&actual_file) {
                                if actual_content != output.stdout {
                                    conflicts.push(file_path.clone());
                                }
                            }
                        } else {
                            // File was deleted externally
                            conflicts.push(file_path.clone());
                        }
                    }
                    _ => {
                        // File doesn't exist in shadow but might exist on disk
                        if actual_file.exists() {
                            conflicts.push(file_path.clone());
                        }
                    }
                }
            }
        }

        conflicts
    }

    /// Redo the most recently undone edit.
    /// Only works if the last operation was an undo.
    pub fn redo(&self) -> Result<UndoResult, ShadowError> {
        if !self.exists() {
            return Err(ShadowError::Undo("No shadow history exists".to_string()));
        }

        // Get the most recent entry to check if it's an undo
        let entries = self.history(None, 1);
        let latest = entries
            .first()
            .ok_or_else(|| ShadowError::Undo("No history to redo".to_string()))?;

        if latest.operation != "undo" {
            return Err(ShadowError::Undo(
                "Last operation was not an undo - nothing to redo".to_string(),
            ));
        }

        // Find the commit that was undone (from the undo commit message)
        let log_output = Command::new("git")
            .args(["log", "-1", "--format=%B", &latest.hash])
            .current_dir(&self.worktree)
            .output()
            .map_err(|e| ShadowError::Undo(format!("Failed to get log: {}", e)))?;

        let body = String::from_utf8_lossy(&log_output.stdout);
        let undone_hash = body
            .lines()
            .find_map(|line| line.strip_prefix("Undone-Commit: "))
            .ok_or_else(|| ShadowError::Undo("Cannot find undone commit reference".to_string()))?;

        // Get file list from the undone commit
        let files_output = Command::new("git")
            .args(["show", "--format=", "--name-only", undone_hash])
            .current_dir(&self.worktree)
            .output()
            .map_err(|e| ShadowError::Undo(format!("Failed to get files: {}", e)))?;

        let files: Vec<String> = String::from_utf8_lossy(&files_output.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();

        // For each file, restore from the undone commit state
        for file_path in &files {
            let worktree_file = self.worktree.join(file_path);
            let actual_file = self.root.join(file_path);

            // Get the file content from the undone commit
            let show_output = Command::new("git")
                .args(["show", &format!("{}:{}", undone_hash, file_path)])
                .current_dir(&self.worktree)
                .output();

            match show_output {
                Ok(output) if output.status.success() => {
                    // File existed in undone commit - restore it
                    if let Some(parent) = actual_file.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::write(&actual_file, &output.stdout).map_err(|e| {
                        ShadowError::Undo(format!("Failed to write {}: {}", file_path, e))
                    })?;
                    // Update worktree too
                    if let Some(parent) = worktree_file.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&worktree_file, &output.stdout);
                }
                _ => {
                    // File was deleted in undone commit - delete it
                    if actual_file.exists() {
                        std::fs::remove_file(&actual_file).map_err(|e| {
                            ShadowError::Undo(format!("Failed to delete {}: {}", file_path, e))
                        })?;
                    }
                    let _ = std::fs::remove_file(&worktree_file);
                }
            }
        }

        // Stage and commit the redo
        let _ = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.worktree)
            .status();

        let redo_msg = format!(
            "moss edit: redo {}\n\nOperation: redo\nTarget: {}\nRedone-Commit: {}\nFiles: {}\nGit-HEAD: {}\n",
            latest.target,
            latest.target,
            undone_hash,
            files.join(", "),
            self.get_real_git_head()
                .unwrap_or_else(|| "none".to_string())
        );

        let _ = Command::new("git")
            .args(["commit", "-m", &redo_msg, "--allow-empty"])
            .current_dir(&self.worktree)
            .status();

        Ok(UndoResult {
            files: files.iter().map(PathBuf::from).collect(),
            undone_commit: undone_hash.to_string(),
            description: format!("redo: {}", latest.target),
            conflicts: vec![], // Redo doesn't check for conflicts
        })
    }

    /// Jump to a specific commit in shadow history, restoring file state from that point.
    /// Can use full SHA, short SHA, or relative refs like HEAD~2.
    pub fn goto(
        &self,
        ref_str: &str,
        dry_run: bool,
        force: bool,
    ) -> Result<UndoResult, ShadowError> {
        if !self.exists() {
            return Err(ShadowError::Undo("No shadow history exists".to_string()));
        }

        // Resolve the ref to a full commit hash
        let rev_parse = Command::new("git")
            .args(["rev-parse", ref_str])
            .current_dir(&self.worktree)
            .output()
            .map_err(|e| ShadowError::Undo(format!("Failed to resolve ref: {}", e)))?;

        if !rev_parse.status.success() {
            return Err(ShadowError::Undo(format!(
                "Invalid ref '{}': not found in shadow history",
                ref_str
            )));
        }

        let target_hash = String::from_utf8_lossy(&rev_parse.stdout)
            .trim()
            .to_string();

        // Get files changed in the target commit
        let files_output = Command::new("git")
            .args(["show", "--format=", "--name-only", &target_hash])
            .current_dir(&self.worktree)
            .output()
            .map_err(|e| ShadowError::Undo(format!("Failed to get files: {}", e)))?;

        let files: Vec<String> = String::from_utf8_lossy(&files_output.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();

        // Get the commit message for description
        let log_output = Command::new("git")
            .args(["log", "-1", "--format=%s", &target_hash])
            .current_dir(&self.worktree)
            .output()
            .map_err(|e| ShadowError::Undo(format!("Failed to get log: {}", e)))?;

        let description = String::from_utf8_lossy(&log_output.stdout)
            .trim()
            .to_string();

        if dry_run {
            return Ok(UndoResult {
                files: files.iter().map(PathBuf::from).collect(),
                undone_commit: target_hash,
                description,
                conflicts: vec![],
            });
        }

        // Check for conflicts if not forcing
        if !force {
            // Create a fake HistoryEntry for conflict detection
            let fake_entry = HistoryEntry {
                id: 0,
                hash: target_hash.clone(),
                subject: description.clone(),
                operation: "goto".to_string(),
                target: ref_str.to_string(),
                files: files.clone(),
                message: None,
                workflow: None,
                git_head: String::new(),
                timestamp: String::new(),
            };
            let conflicts = self.detect_conflicts(&[fake_entry]);
            if !conflicts.is_empty() {
                let files_str = conflicts.join(", ");
                return Err(ShadowError::Undo(format!(
                    "Files modified externally: {}. Use --force to override.",
                    files_str
                )));
            }
        }

        // Restore files from target commit state
        for file_path in &files {
            let worktree_file = self.worktree.join(file_path);
            let actual_file = self.root.join(file_path);

            let show_output = Command::new("git")
                .args(["show", &format!("{}:{}", target_hash, file_path)])
                .current_dir(&self.worktree)
                .output();

            match show_output {
                Ok(output) if output.status.success() => {
                    if let Some(parent) = actual_file.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::write(&actual_file, &output.stdout).map_err(|e| {
                        ShadowError::Undo(format!("Failed to write {}: {}", file_path, e))
                    })?;
                    if let Some(parent) = worktree_file.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&worktree_file, &output.stdout);
                }
                _ => {
                    // File doesn't exist in target commit
                    if actual_file.exists() {
                        std::fs::remove_file(&actual_file).map_err(|e| {
                            ShadowError::Undo(format!("Failed to delete {}: {}", file_path, e))
                        })?;
                    }
                    let _ = std::fs::remove_file(&worktree_file);
                }
            }
        }

        // Stage and commit the goto
        let _ = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.worktree)
            .status();

        let goto_msg = format!(
            "moss edit: goto {}\n\nOperation: goto\nTarget: {}\nGoto-Commit: {}\nFiles: {}\nGit-HEAD: {}\n",
            ref_str,
            ref_str,
            target_hash,
            files.join(", "),
            self.get_real_git_head()
                .unwrap_or_else(|| "none".to_string())
        );

        let _ = Command::new("git")
            .args(["commit", "-m", &goto_msg, "--allow-empty"])
            .current_dir(&self.worktree)
            .status();

        Ok(UndoResult {
            files: files.iter().map(PathBuf::from).collect(),
            undone_commit: target_hash,
            description,
            conflicts: vec![],
        })
    }
}

/// Result of an undo operation.
pub struct UndoResult {
    /// Files that were modified by the undo
    pub files: Vec<PathBuf>,
    /// The commit that was undone
    pub undone_commit: String,
    /// Description of what was undone
    pub description: String,
    /// Files that have been modified externally (only populated in dry-run)
    pub conflicts: Vec<String>,
}

/// Shadow git errors.
#[derive(Debug)]
pub enum ShadowError {
    Init(String),
    Commit(String),
    Undo(String),
    Validation(String),
}

impl std::fmt::Display for ShadowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShadowError::Init(msg) => write!(f, "Shadow init error: {}", msg),
            ShadowError::Commit(msg) => write!(f, "Shadow commit error: {}", msg),
            ShadowError::Undo(msg) => write!(f, "Shadow undo error: {}", msg),
            ShadowError::Validation(msg) => write!(f, "Shadow validation error: {}", msg),
        }
    }
}

impl std::error::Error for ShadowError {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_shadow_new() {
        let dir = TempDir::new().unwrap();
        let shadow = Shadow::new(dir.path());

        assert!(!shadow.exists());
        assert_eq!(shadow.shadow_dir, dir.path().join(".moss").join("shadow"));
    }

    #[test]
    fn test_shadow_init() {
        let dir = TempDir::new().unwrap();
        let shadow = Shadow::new(dir.path());

        // Initialize as if it's the first edit
        shadow.init().unwrap();

        assert!(shadow.exists());
        assert!(shadow.worktree.exists());
    }

    #[test]
    fn test_shadow_before_after_edit() {
        let dir = TempDir::new().unwrap();

        // Create a test file
        let test_file = dir.path().join("test.rs");
        std::fs::write(&test_file, "fn foo() {}").unwrap();

        let shadow = Shadow::new(dir.path());

        // Before edit
        shadow.before_edit(&[&test_file]).unwrap();

        // Simulate edit
        std::fs::write(&test_file, "fn bar() {}").unwrap();

        // After edit
        let info = EditInfo {
            operation: "replace".to_string(),
            target: "test.rs/foo".to_string(),
            files: vec![test_file.clone()],
            message: Some("Renamed foo to bar".to_string()),
            workflow: None,
        };
        shadow.after_edit(&info).unwrap();

        assert_eq!(shadow.edit_count(), 1);
    }
}
