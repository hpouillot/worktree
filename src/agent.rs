use anyhow::{bail, Context, Result};
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn offer_agent_for_dirty_changes(path: &Path) -> Result<()> {
    let prompt = "This git worktree has uncommitted changes. Inspect the changes, make an appropriate commit, and stop. Do not merge branches or delete worktrees. After committing, summarize what you committed.";
    offer_agent(
        path,
        "Uncommitted changes detected. Spawn a pi agent to commit them?",
        prompt,
    )
}

pub fn offer_agent_for_merge_conflicts(
    path: &Path,
    source_branch: &str,
    dest_branch: &str,
) -> Result<()> {
    let prompt = format!(
        "The merge of branch '{source_branch}' into '{dest_branch}' has conflicts in this repository. Resolve the merge conflicts, run relevant checks if appropriate, commit the merge, and stop. Do not delete worktrees or branches."
    );
    offer_agent(
        path,
        "Merge conflicts detected. Spawn a pi agent to resolve them?",
        &prompt,
    )
}

fn offer_agent(path: &Path, question: &str, prompt: &str) -> Result<()> {
    let Some(agent) = detect_agent() else {
        eprintln!("No agent CLI found (looked for pi, then claude).");
        return Ok(());
    };
    if !confirm(&format!("{question} ({agent}) [y/N] "))? {
        return Ok(());
    }
    spawn_agent_workspace(path, &agent, prompt)
}

fn detect_agent() -> Option<String> {
    ["pi", "claude"]
        .into_iter()
        .find(|cmd| command_exists(cmd))
        .map(str::to_string)
}

fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .args([
            "-lc",
            &format!("command -v {} >/dev/null 2>&1", shell_quote(command)),
        ])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn confirm(question: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        eprintln!("{question}no (stdin is not interactive)");
        return Ok(false);
    }
    eprint!("{question}");
    io::stderr().flush().ok();
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .context("read confirmation")?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
}

fn spawn_agent_workspace(path: &Path, agent: &str, prompt: &str) -> Result<()> {
    let command = format!("{} {}", shell_quote(agent), shell_quote(prompt));
    let status = Command::new("cmux")
        .args(["new-workspace", "--cwd"])
        .arg(path)
        .args(["--command", &command])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("spawn {agent} in cmux workspace at {}", path.display()))?;
    if !status.success() {
        bail!("cmux new-workspace exited with {status}");
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
