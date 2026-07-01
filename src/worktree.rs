use anyhow::{anyhow, bail, Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git::git_capture;

const WORKTREES_SUFFIX: &str = "worktrees";
const ROOTS_DIR: &str = ".wt-roots";

#[derive(Debug, Clone)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
}

pub fn resolve_root_arg(
    current_worktree_root: &Path,
    cwd: &Path,
    root: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let root = match root {
        Some(root) => normalize_repo_relative(root)?,
        None => relative_to(cwd, current_worktree_root),
    };
    Ok(root.filter(|path| path != Path::new(".")))
}

pub fn resolve_open_root(
    repo_root: &Path,
    current_worktree_root: &Path,
    cwd: &Path,
    entry: &WorktreeEntry,
    root: Option<&Path>,
) -> Result<Option<PathBuf>> {
    if let Some(root) = root {
        return Ok(normalize_repo_relative(root)?.filter(|path| path != Path::new(".")));
    }
    if let Some(saved) = load_worktree_root(repo_root, entry)? {
        return Ok(Some(saved));
    }
    Ok(relative_to(cwd, current_worktree_root).filter(|path| path != Path::new(".")))
}

fn relative_to(path: &Path, base: &Path) -> Option<PathBuf> {
    path.strip_prefix(base).ok().map(|rel| {
        if rel.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            rel.to_path_buf()
        }
    })
}

fn normalize_repo_relative(path: &Path) -> Result<Option<PathBuf>> {
    if path.as_os_str().is_empty() || path == Path::new(".") {
        return Ok(None);
    }
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!(
            "root must be a relative path inside the repo: {}",
            path.display()
        );
    }
    Ok(Some(path.to_path_buf()))
}

pub fn open_path_for_root(worktree_path: &Path, root: Option<&Path>) -> Result<PathBuf> {
    let path = root.map_or_else(
        || worktree_path.to_path_buf(),
        |root| worktree_path.join(root),
    );
    if !path.is_dir() {
        bail!(
            "open root does not exist or is not a directory: {}",
            path.display()
        );
    }
    Ok(path)
}

fn roots_dir(repo_root: &Path) -> PathBuf {
    managed_dir(repo_root).join(ROOTS_DIR)
}

fn root_file_for(repo_root: &Path, entry_name: &str) -> PathBuf {
    roots_dir(repo_root).join(entry_name)
}

pub fn entry_name(entry: &WorktreeEntry) -> Option<&str> {
    entry.path.file_name().and_then(OsStr::to_str)
}

pub fn save_worktree_root(repo_root: &Path, name: &str, root: Option<&Path>) -> Result<()> {
    let file = root_file_for(repo_root, name);
    if let Some(root) = root {
        fs::create_dir_all(roots_dir(repo_root))
            .with_context(|| format!("create {}", roots_dir(repo_root).display()))?;
        fs::write(&file, format!("{}\n", root.display()))
            .with_context(|| format!("write {}", file.display()))?;
    } else if file.exists() {
        fs::remove_file(&file).with_context(|| format!("remove {}", file.display()))?;
    }
    Ok(())
}

pub fn load_worktree_root(repo_root: &Path, entry: &WorktreeEntry) -> Result<Option<PathBuf>> {
    let Some(name) = entry_name(entry) else {
        return Ok(None);
    };
    let file = root_file_for(repo_root, name);
    if !file.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&file).with_context(|| format!("read {}", file.display()))?;
    normalize_repo_relative(Path::new(raw.trim()))
}

pub fn remove_worktree_root(repo_root: &Path, entry: &WorktreeEntry) -> Result<()> {
    let Some(name) = entry_name(entry) else {
        return Ok(());
    };
    let file = root_file_for(repo_root, name);
    if file.exists() {
        fs::remove_file(&file).with_context(|| format!("remove {}", file.display()))?;
    }
    Ok(())
}

pub fn primary_worktree(cwd: &Path) -> Result<PathBuf> {
    let entries = worktrees_from(cwd)?;
    entries
        .into_iter()
        .find(|entry| !entry.bare)
        .map(|entry| entry.path)
        .ok_or_else(|| anyhow!("no non-bare worktree found"))
}

pub fn current_worktree_root(cwd: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .context("run git rev-parse")?;
    if !output.status.success() {
        bail!("current directory is not inside a git repository");
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

pub fn current_branch(path: &Path) -> Result<String> {
    Ok(git_capture(path, ["branch", "--show-current"])?
        .trim()
        .to_string())
    .and_then(|branch| {
        if branch.is_empty() {
            bail!("{} is not on a branch", path.display());
        }
        Ok(branch)
    })
}

pub fn worktree_for_branch(repo_root: &Path, branch: &str) -> Result<Option<PathBuf>> {
    Ok(worktrees(repo_root)?
        .into_iter()
        .find(|entry| entry.branch.as_deref() == Some(branch))
        .map(|entry| entry.path))
}

pub fn ensure_branch_only_checked_out_in(
    repo_root: &Path,
    branch: &str,
    allowed_path: &Path,
) -> Result<()> {
    if !branch_exists(repo_root, branch)? {
        bail!("branch {branch} does not exist");
    }
    let other_paths: Vec<_> = worktrees(repo_root)?
        .into_iter()
        .filter(|entry| {
            entry.branch.as_deref() == Some(branch) && !same_path(&entry.path, allowed_path)
        })
        .map(|entry| entry.path)
        .collect();
    if !other_paths.is_empty() {
        let paths = other_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        bail!("branch {branch} is also checked out in {paths}");
    }
    Ok(())
}

pub fn managed_dir(repo_root: &Path) -> PathBuf {
    let repo_name = repo_root
        .file_name()
        .map(|name| {
            let mut dir_name = name.to_os_string();
            dir_name.push("-");
            dir_name.push(WORKTREES_SUFFIX);
            dir_name
        })
        .unwrap_or_else(|| WORKTREES_SUFFIX.into());
    repo_root
        .parent()
        .map(|parent| parent.join(&repo_name))
        .unwrap_or_else(|| repo_root.join(repo_name))
}

pub fn worktrees(repo_root: &Path) -> Result<Vec<WorktreeEntry>> {
    worktrees_from(repo_root)
}

pub fn worktrees_from(cwd: &Path) -> Result<Vec<WorktreeEntry>> {
    let raw = git_capture(cwd, ["worktree", "list", "--porcelain"])?;
    let mut entries = Vec::new();
    let mut current: Option<WorktreeEntry> = None;

    for line in raw.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(WorktreeEntry {
                path: PathBuf::from(path),
                branch: None,
                bare: false,
                detached: false,
            });
        } else if let Some(entry) = current.as_mut() {
            if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                entry.branch = Some(branch.to_string());
            } else if line == "bare" {
                entry.bare = true;
            } else if line == "detached" {
                entry.detached = true;
            }
        }
    }
    if let Some(entry) = current {
        entries.push(entry);
    }
    Ok(entries)
}

pub fn find_worktree(repo_root: &Path, target: &str) -> Result<WorktreeEntry> {
    let direct = PathBuf::from(target);
    let maybe_path = if direct.is_absolute() {
        direct
    } else {
        repo_root.join(target)
    };
    let target_branch = target.strip_prefix("refs/heads/").unwrap_or(target);

    worktrees(repo_root)?
        .into_iter()
        .find(|entry| {
            same_path(&entry.path, &maybe_path)
                || entry.path.file_name().and_then(OsStr::to_str) == Some(target)
                || entry.branch.as_deref() == Some(target_branch)
        })
        .ok_or_else(|| anyhow!("unknown worktree: {target}"))
}

pub fn branch_exists(repo_root: &Path, branch: &str) -> Result<bool> {
    let status = Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(repo_root)
        .status()
        .context("run git show-ref")?;
    Ok(status.success())
}

pub fn is_dirty(path: &Path) -> Result<bool> {
    Ok(!git_capture(path, ["status", "--porcelain"])?
        .trim()
        .is_empty())
}

pub fn is_managed(repo_root: &Path, path: &Path) -> bool {
    path.starts_with(managed_dir(repo_root))
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    let a = fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let b = fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    a == b
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        bail!("invalid worktree name: {name}");
    }
    Ok(())
}

pub fn validate_branch(branch: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["check-ref-format", "--branch", branch])
        .status()
        .context("run git check-ref-format")?;
    if !status.success() {
        bail!("invalid branch name: {branch}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_worktrees() {
        let raw = "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /repo-worktrees/feat\nHEAD def\nbranch refs/heads/feat\n";
        let mut entries = Vec::new();
        let mut current: Option<WorktreeEntry> = None;
        for line in raw.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                if let Some(entry) = current.take() {
                    entries.push(entry);
                }
                current = Some(WorktreeEntry {
                    path: PathBuf::from(path),
                    branch: None,
                    bare: false,
                    detached: false,
                });
            } else if let Some(entry) = current.as_mut() {
                if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                    entry.branch = Some(branch.to_string());
                }
            }
        }
        if let Some(entry) = current {
            entries.push(entry);
        }

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].branch.as_deref(), Some("feat"));
    }

    #[test]
    fn managed_dir_is_sibling_named_after_repo() {
        assert_eq!(
            managed_dir(Path::new("/tmp/example")),
            PathBuf::from("/tmp/example-worktrees")
        );
    }

    #[test]
    fn dot_worktrees_are_not_managed() {
        let repo_root = Path::new("/tmp/example");
        assert!(!is_managed(
            repo_root,
            Path::new("/tmp/example/.worktrees/feat")
        ));
    }
}
