use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::worktree::WorktreeEntry;

pub fn default_title(entry: &WorktreeEntry) -> String {
    let name = entry
        .branch
        .as_deref()
        .or_else(|| entry.path.file_name().and_then(OsStr::to_str))
        .unwrap_or("worktree");
    format!("🌿 {name}")
}

pub fn open_cmux_workspace(path: &Path, title: Option<&str>, pin: bool) -> Result<()> {
    let output = Command::new("cmux")
        .args(["new-workspace", "--cwd"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| {
            format!(
                "open {} with cmux; install cmux or use `wt path <target>` to print the path",
                path.display()
            )
        })?;
    if !output.status.success() {
        bail!("cmux exited with {}", output.status);
    }

    let workspace = parse_cmux_workspace_handle(&String::from_utf8_lossy(&output.stdout));

    if let (Some(workspace), Some(title)) = (workspace.as_deref(), title) {
        cmux_workspace_action(workspace, "rename", Some(title))?;
        if pin {
            cmux_workspace_action(workspace, "pin", None)?;
        }
        println!("Opened {workspace}: {title}");
    } else if let Some(workspace) = workspace.as_deref() {
        if pin {
            cmux_workspace_action(workspace, "pin", None)?;
        }
        println!("Opened {workspace}");
    }
    Ok(())
}

fn parse_cmux_workspace_handle(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|token| token.starts_with("workspace:"))
        .map(str::to_string)
        .or_else(|| {
            output
                .lines()
                .next()
                .map(str::trim)
                .filter(|line| !line.is_empty() && *line != "OK")
                .map(str::to_string)
        })
}

fn cmux_workspace_action(workspace: &str, action: &str, title: Option<&str>) -> Result<()> {
    let mut command = Command::new("cmux");
    command.args([
        "workspace-action",
        "--workspace",
        workspace,
        "--action",
        action,
    ]);
    if let Some(title) = title {
        command.args(["--title", title]);
    }
    let status = command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("run cmux workspace-action {action}"))?;
    if !status.success() {
        bail!("cmux workspace-action {action} exited with {status}");
    }
    Ok(())
}

pub fn close_cmux_workspace_for(entry: &WorktreeEntry) {
    let title = default_title(entry);
    match cmux_workspace_by_title(&title) {
        Ok(Some(workspace)) => {
            if let Err(err) = close_cmux_workspace(&workspace) {
                eprintln!("warning: failed to close cmux workspace {workspace}: {err}");
            }
        }
        Ok(None) => {}
        Err(err) => eprintln!("warning: failed to find cmux workspace to close: {err}"),
    }
}

fn cmux_workspace_by_title(title: &str) -> Result<Option<String>> {
    let output = Command::new("cmux")
        .arg("list-workspaces")
        .stdin(Stdio::null())
        .output()
        .context("run cmux list-workspaces")?;
    if !output.status.success() {
        bail!("cmux list-workspaces exited with {}", output.status);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim_start_matches([' ', '*']).trim_start();
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let Some(handle) = parts.next() else { continue };
        let Some(rest) = parts.next() else { continue };
        if handle.starts_with("workspace:") && rest.trim() == title {
            return Ok(Some(handle.to_string()));
        }
    }
    Ok(None)
}

fn close_cmux_workspace(workspace: &str) -> Result<()> {
    let status = Command::new("cmux")
        .args(["close-workspace", "--workspace", workspace])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("run cmux close-workspace")?;
    if !status.success() {
        bail!("cmux close-workspace exited with {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cmux_workspace_handle() {
        assert_eq!(
            parse_cmux_workspace_handle("OK workspace:6\n").as_deref(),
            Some("workspace:6")
        );
        assert_eq!(
            parse_cmux_workspace_handle("workspace:7\n").as_deref(),
            Some("workspace:7")
        );
    }
}
