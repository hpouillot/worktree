mod agent;
mod cli;
mod cmux;
mod commands;
mod git;
mod worktree;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Commands::Init = cli.command {
        commands::print_shell_init();
        return Ok(());
    }

    let cwd = std::env::current_dir().context("read current directory")?;
    let current_worktree_root = worktree::current_worktree_root(&cwd)?;
    let repo_root = worktree::primary_worktree(&cwd)?;

    match cli.command {
        Commands::Init => unreachable!(),
        Commands::Create {
            name,
            branch,
            base,
            existing,
            root,
            no_open,
        } => commands::create(
            &repo_root,
            &current_worktree_root,
            &cwd,
            &name,
            branch.as_deref(),
            &base,
            existing,
            root.as_deref(),
            !no_open,
        ),
        Commands::List { all } => commands::list(&repo_root, all),
        Commands::Open {
            target,
            command,
            title,
            root,
            no_rename,
            pin,
        } => commands::open(
            &repo_root,
            &current_worktree_root,
            &cwd,
            &target,
            command.as_deref(),
            title.as_deref(),
            root.as_deref(),
            no_rename,
            pin,
        ),
        Commands::Path { target } => commands::path(&repo_root, &target),
        Commands::Merge {
            target,
            into,
            keep_branch,
        } => commands::merge(&repo_root, &target, into.as_deref(), keep_branch),
        Commands::Delete {
            target,
            force,
            delete_branch,
        } => commands::delete(&repo_root, &target, force, delete_branch),
    }
}
