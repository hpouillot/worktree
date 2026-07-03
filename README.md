# worktree

A small Rust CLI for managing git worktrees next to the current repository.

The binary is `wt`. It creates managed worktrees in a sibling directory named
`../<repo>-worktrees/<name>` and remembers per-worktree open roots under that
managed directory.

## Features

- Create git worktrees with matching branch names by default.
- List managed worktrees, or all linked worktrees.
- Open a worktree in `cmux`, optionally pinned or with a custom title.
- Run commands inside a selected worktree.
- Save and reuse a per-worktree subdirectory root.
- Merge a worktree branch back and clean up the worktree.
- Delete managed worktrees and optionally delete their branches.

## Installation

From this repository:

```sh
cargo install --path . --force
```

Or during development:

```sh
cargo build
cargo run -- list
```

`wt open` and the default `wt create` flow use `cmux` to open workspaces. Use `wt create --no-open`, `wt open --command ...`, or `wt path <target>` if you only need paths/commands.

## Shell integration

Install the shell wrapper so `wt open <target>` can `cd` your current shell into the worktree instead of opening `cmux`:

```sh
# zsh/bash
source <(wt init)
```

To make it permanent, add that line to your shell rc file.

## Usage

```sh
wt <COMMAND>
```

Commands:

- `wt init` — print shell integration.
- `wt create <name>` — create `../<repo>-worktrees/<name>`.
- `wt list` — list managed worktrees.
- `wt open <target>` — open a worktree in `cmux` or run a command in it.
- `wt merge <target>` — merge a managed worktree branch and remove it.
- `wt delete <target>` — remove a managed worktree.
- `wt path <target>` — print the resolved worktree path (hidden command, used by shell integration).

A target can be a worktree name, branch name, or path.

## Examples

Create a worktree and branch named `feature-login`, then open it in `cmux`:

```sh
wt create feature-login
```

Create from a specific base without opening it:

```sh
wt create feature-login --base origin/main --no-open
```

Create a worktree with a different branch name:

```sh
wt create login-ui --branch feature/login-ui
```

Reuse an existing branch:

```sh
wt create hotfix --branch hotfix/issue-123 --existing
```

Existing branches are only reused when `--existing` is passed; otherwise `wt create` fails instead of silently checking out that branch.

List managed worktrees:

```sh
wt list
```

Include the main worktree and unmanaged linked worktrees:

```sh
wt list --all
```

Open a worktree:

```sh
wt open feature-login
```

Run a command inside a worktree:

```sh
wt open feature-login --command "cargo test"
```

Open a saved or explicit subdirectory root:

```sh
wt create docs-update --root docs
wt open docs-update
wt open docs-update --root crates/api
```

Merge a worktree branch into the current main-worktree branch and remove the worktree:

```sh
wt merge feature-login
```

Merge into a specific destination branch and keep the source branch:

```sh
wt merge feature-login --into main --keep-branch
```

Delete a managed worktree:

```sh
wt delete feature-login
```

Force-delete a dirty worktree and delete its branch:

```sh
wt delete feature-login --force --delete-branch
```

## Development

Common tasks are available through `just`:

```sh
just build
just test
just fmt
just check
just install
just release
```

The main entrypoint is `src/main.rs`.

## Notes

- `wt` must be run from inside a git repository or one of its worktrees.
- Managed worktrees are protected: `merge` and `delete` refuse unmanaged worktrees.
- `merge` checks for dirty source/destination worktrees before merging.
- If a merge conflict or dirty worktree is detected, `wt` can offer to spawn an agent in `cmux` when `pi` or `claude` is installed.
