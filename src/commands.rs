use anyhow::{anyhow, bail, Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::Stdio;

use crate::agent::{offer_agent_for_dirty_changes, offer_agent_for_merge_conflicts};
use crate::cmux::{close_cmux_workspace_for, default_title, open_cmux_workspace};
use crate::git::{git, git_capture, path_str, path_str_lossy, shell_command};
use crate::worktree::{
    branch_exists, current_branch, ensure_branch_only_checked_out_in, entry_name, find_worktree,
    is_dirty, is_managed, load_worktree_root, managed_dir, open_path_for_root,
    remove_worktree_root, resolve_open_root, resolve_root_arg, same_path, save_worktree_root,
    validate_branch, validate_name, worktree_for_branch, worktrees, WorktreeEntry,
};

pub fn print_shell_init() {
    println!(
        r#"wt() {{
  if [ "$1" = "open" ] && [ "$#" -eq 2 ] && [ "${{2#-}}" = "$2" ]; then
    local __wt_path
    __wt_path="$(command wt path "$2")" || return $?
    cd "$__wt_path"
  else
    command wt "$@"
  fi
}}"#
    );
}

pub fn create(
    repo_root: &Path,
    current_worktree_root: &Path,
    cwd: &Path,
    name: &str,
    branch: Option<&str>,
    base: &str,
    existing: bool,
    root: Option<&Path>,
    open_after_create: bool,
) -> Result<()> {
    validate_name(name)?;
    let branch = branch.unwrap_or(name);
    validate_branch(branch)?;

    let managed_dir = managed_dir(repo_root);
    fs::create_dir_all(&managed_dir)
        .with_context(|| format!("create {}", managed_dir.display()))?;

    let path = managed_dir.join(name);
    if path.exists() {
        bail!("{} already exists", path.display());
    }

    let open_root = resolve_root_arg(current_worktree_root, cwd, root)?;
    let branch_preexisted = branch_exists(repo_root, branch)?;

    if existing || branch_preexisted {
        git(repo_root, ["worktree", "add", path_str(&path)?, branch])?;
    } else {
        git(
            repo_root,
            ["worktree", "add", "-b", branch, path_str(&path)?, base],
        )?;
    }

    if let Err(err) = save_worktree_root(repo_root, name, open_root.as_deref()) {
        rollback_created_worktree(repo_root, &path, branch, !branch_preexisted);
        return Err(err).with_context(|| {
            format!(
                "save root for {}; rolled back created worktree",
                path.display()
            )
        });
    }

    println!("{}", path.display());
    if open_after_create {
        let entry = find_worktree(repo_root, name)?;
        let open_path = open_path_for_root(&entry.path, open_root.as_deref())?;
        open_cmux_workspace(&open_path, Some(&default_title(&entry)), false)?;
    }
    Ok(())
}

fn rollback_created_worktree(repo_root: &Path, path: &Path, branch: &str, delete_branch: bool) {
    eprintln!(
        "warning: rolling back partially created worktree {}",
        path.display()
    );
    if let Err(err) = git(
        repo_root,
        [
            "worktree",
            "remove",
            "--force",
            path_str_lossy(path).as_ref(),
        ],
    ) {
        eprintln!(
            "warning: failed to remove partially created worktree {}: {err}",
            path.display()
        );
    }
    if delete_branch {
        if let Err(err) = git(repo_root, ["branch", "-D", branch]) {
            eprintln!("warning: failed to delete partially created branch {branch}: {err}");
        }
    }
}

pub fn list(repo_root: &Path, all: bool) -> Result<()> {
    let entries = worktrees(repo_root)?;
    let rows: Vec<_> = entries
        .into_iter()
        .filter(|entry| all || is_managed(repo_root, &entry.path))
        .collect();

    if rows.is_empty() {
        println!(
            "No managed worktrees in {}",
            managed_dir(repo_root).display()
        );
        return Ok(());
    }

    for entry in rows {
        let name = if is_managed(repo_root, &entry.path) {
            entry
                .path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("-")
                .to_string()
        } else if same_path(&entry.path, repo_root) {
            "main".to_string()
        } else {
            "-".to_string()
        };
        let branch =
            entry
                .branch
                .as_deref()
                .unwrap_or(if entry.detached { "DETACHED" } else { "-" });
        let kind = if entry.bare {
            "bare"
        } else if is_managed(repo_root, &entry.path) {
            "managed"
        } else {
            "external"
        };
        println!("{name}\t{branch}\t{kind}\t{}", entry.path.display());
    }
    Ok(())
}

pub fn open(
    repo_root: &Path,
    current_worktree_root: &Path,
    cwd: &Path,
    target: &str,
    command: Option<&str>,
    title: Option<&str>,
    root: Option<&Path>,
    no_rename: bool,
    pin: bool,
) -> Result<()> {
    let entry = find_worktree(repo_root, target)?;
    let root = resolve_open_root(repo_root, current_worktree_root, cwd, &entry, root)?;
    let open_path = open_path_for_root(&entry.path, root.as_deref())?;
    if let Some(command) = command {
        let status = shell_command(command)
            .current_dir(&open_path)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("run command in {}", open_path.display()))?;
        if !status.success() {
            bail!("command exited with {status}");
        }
    } else {
        let title = if no_rename {
            None
        } else {
            Some(
                title
                    .map(str::to_string)
                    .unwrap_or_else(|| default_title(&entry)),
            )
        };
        open_cmux_workspace(&open_path, title.as_deref(), pin)?;
    }
    Ok(())
}

pub fn path(repo_root: &Path, target: &str) -> Result<()> {
    let entry = find_worktree(repo_root, target)?;
    let root = load_worktree_root(repo_root, &entry)?;
    let path = open_path_for_root(&entry.path, root.as_deref())?;
    println!("{}", path.display());
    Ok(())
}

pub fn delete(repo_root: &Path, target: &str, force: bool, delete_branch: bool) -> Result<()> {
    let entry = find_worktree(repo_root, target)?;
    if !is_managed(repo_root, &entry.path) {
        bail!(
            "refusing to delete unmanaged worktree {}",
            entry.path.display()
        );
    }

    if !force && is_dirty(&entry.path)? {
        bail!(
            "{} has uncommitted changes; use --force to remove it",
            entry.path.display()
        );
    }

    if delete_branch {
        let branch = entry
            .branch
            .as_deref()
            .ok_or_else(|| anyhow!("worktree {} has no branch to delete", entry.path.display()))?;
        ensure_branch_only_checked_out_in(repo_root, branch, &entry.path)?;
    }

    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path_str(&entry.path)?);
    git(repo_root, args)?;
    remove_worktree_root(repo_root, &entry)?;

    if delete_branch {
        if let Some(branch) = entry.branch.as_deref() {
            git(repo_root, ["branch", "-D", branch])?;
        }
    }

    println!("Removed {}", entry.path.display());
    Ok(())
}

pub fn merge(repo_root: &Path, target: &str, into: Option<&str>, keep_branch: bool) -> Result<()> {
    let entry = find_worktree(repo_root, target)?;
    if !is_managed(repo_root, &entry.path) {
        bail!(
            "refusing to merge unmanaged worktree {}",
            entry.path.display()
        );
    }
    if entry.detached {
        bail!(
            "refusing to merge detached worktree {}",
            entry.path.display()
        );
    }

    let source_branch = entry
        .branch
        .as_deref()
        .ok_or_else(|| anyhow!("worktree {} has no branch", entry.path.display()))?;
    let dest_branch = match into {
        Some(branch) => branch.to_string(),
        None => current_branch(repo_root).context("read current branch in main worktree")?,
    };
    if source_branch == dest_branch {
        bail!("refusing to merge branch {source_branch} into itself");
    }
    if !keep_branch {
        ensure_branch_only_checked_out_in(repo_root, source_branch, &entry.path)?;
    }

    if is_dirty(&entry.path)? {
        offer_agent_for_dirty_changes(&entry.path)?;
        bail!(
            "{} has uncommitted changes; commit or discard them before merging",
            entry.path.display()
        );
    }

    let dest_path =
        worktree_for_branch(repo_root, &dest_branch)?.unwrap_or_else(|| repo_root.to_path_buf());
    let actual_dest_branch = current_branch(&dest_path).with_context(|| {
        format!(
            "read current branch in destination worktree {}",
            dest_path.display()
        )
    })?;
    if actual_dest_branch != dest_branch {
        bail!(
            "destination branch {dest_branch} is not checked out in {}; currently on {actual_dest_branch}",
            dest_path.display()
        );
    }
    if is_dirty(&dest_path)? {
        offer_agent_for_dirty_changes(&dest_path)?;
        bail!(
            "destination worktree {} has uncommitted changes; commit or discard them before merging",
            dest_path.display()
        );
    }
    let pre_merge_head = git_capture(&dest_path, ["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    let saved_root = load_worktree_root(repo_root, &entry)?;

    if let Err(err) = git(&dest_path, ["merge", source_branch]) {
        if has_merge_conflicts(&dest_path)? {
            offer_agent_for_merge_conflicts(&dest_path, source_branch, &dest_branch)?;
        }
        return Err(err).with_context(|| format!("merge {source_branch} into {dest_branch}"));
    }

    if let Err(err) = cleanup_merged_worktree(repo_root, &entry, source_branch, keep_branch) {
        rollback_successful_merge(
            repo_root,
            &dest_path,
            &pre_merge_head,
            &entry,
            source_branch,
            saved_root.as_deref(),
        );
        return Err(err).with_context(|| {
            format!(
                "cleanup after merging {source_branch} into {dest_branch}; rolled back destination branch to {pre_merge_head}"
            )
        });
    }

    close_cmux_workspace_for(&entry);
    println!(
        "Merged {source_branch} into {dest_branch} and removed {}",
        entry.path.display()
    );
    Ok(())
}

fn cleanup_merged_worktree(
    repo_root: &Path,
    entry: &WorktreeEntry,
    source_branch: &str,
    keep_branch: bool,
) -> Result<()> {
    git(repo_root, ["worktree", "remove", path_str(&entry.path)?])?;
    remove_worktree_root(repo_root, entry)?;
    if !keep_branch {
        git(repo_root, ["branch", "-d", source_branch])?;
    }
    Ok(())
}

fn rollback_successful_merge(
    repo_root: &Path,
    dest_path: &Path,
    pre_merge_head: &str,
    entry: &WorktreeEntry,
    source_branch: &str,
    saved_root: Option<&Path>,
) {
    eprintln!(
        "warning: rolling back destination branch in {} to {pre_merge_head}",
        dest_path.display()
    );
    if let Err(err) = git(dest_path, ["reset", "--hard", pre_merge_head]) {
        eprintln!(
            "warning: failed to reset destination worktree {} to {pre_merge_head}: {err}",
            dest_path.display()
        );
    }

    if !entry.path.exists() {
        match branch_exists(repo_root, source_branch) {
            Ok(true) => {
                if let Err(err) = git(
                    repo_root,
                    [
                        "worktree",
                        "add",
                        path_str_lossy(&entry.path).as_ref(),
                        source_branch,
                    ],
                ) {
                    eprintln!(
                        "warning: failed to restore source worktree {}: {err}",
                        entry.path.display()
                    );
                } else if let Err(err) = save_worktree_root(
                    repo_root,
                    entry_name(entry).unwrap_or(source_branch),
                    saved_root,
                ) {
                    eprintln!(
                        "warning: failed to restore saved root for {}: {err}",
                        entry.path.display()
                    );
                }
            }
            Ok(false) => eprintln!(
                "warning: source branch {source_branch} no longer exists; cannot restore {}",
                entry.path.display()
            ),
            Err(err) => eprintln!(
                "warning: failed to check source branch {source_branch}; cannot restore {}: {err}",
                entry.path.display()
            ),
        }
    }
}

fn has_merge_conflicts(path: &Path) -> Result<bool> {
    Ok(
        !git_capture(path, ["diff", "--name-only", "--diff-filter=U"])?
            .trim()
            .is_empty(),
    )
}
