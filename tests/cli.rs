use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

struct TestRepo {
    _temp: TempDir,
    root: PathBuf,
}

impl TestRepo {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("create temp dir");
        let temp_root = fs::canonicalize(temp.path()).expect("canonicalize temp dir");
        let root = temp_root.join("repo");
        fs::create_dir(&root).expect("create repo dir");

        run_git(&root, &["init", "-b", "main"]);
        run_git(&root, &["config", "user.name", "Test User"]);
        run_git(&root, &["config", "user.email", "test@example.com"]);

        fs::write(root.join("README.md"), "# test repo\n").expect("write readme");
        fs::create_dir_all(root.join("crates/api")).expect("create nested dir");
        fs::write(root.join("crates/api/lib.rs"), "pub fn api() {}\n").expect("write nested file");
        run_git(&root, &["add", "."]);
        run_git(&root, &["commit", "-m", "initial"]);

        Self { _temp: temp, root }
    }

    fn managed_worktree(&self, name: &str) -> PathBuf {
        self.root
            .parent()
            .expect("repo has parent")
            .join("repo-worktrees")
            .join(name)
    }
}

fn wt_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_wt"))
}

fn run_wt(cwd: &Path, args: &[&str]) -> Output {
    Command::new(wt_bin())
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run wt")
}

fn run_git(cwd: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git");
    assert_success("git", args, &output);
    output
}

fn assert_success(program: &str, args: &[&str], output: &Output) {
    assert!(
        output.status.success(),
        "{program} {args:?} failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn last_stdout_line(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .expect("stdout has a line")
        .to_string()
}

#[test]
fn create_list_path_and_delete_managed_worktree() {
    let repo = TestRepo::new();
    let worktree = repo.managed_worktree("feature");

    let output = run_wt(&repo.root, &["create", "feature", "--no-open"]);
    assert_success("wt", &["create", "feature", "--no-open"], &output);
    assert_eq!(last_stdout_line(&output), worktree.display().to_string());
    assert!(worktree.is_dir());

    let output = run_wt(&repo.root, &["list"]);
    assert_success("wt", &["list"], &output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!(
        "feature\tfeature\tmanaged\t{}",
        worktree.display()
    )));

    let output = run_wt(&repo.root, &["path", "feature"]);
    assert_success("wt", &["path", "feature"], &output);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        worktree.display().to_string()
    );

    let output = run_wt(&repo.root, &["delete", "feature", "--delete-branch"]);
    assert_success("wt", &["delete", "feature", "--delete-branch"], &output);
    assert!(!worktree.exists());

    let output = run_wt(&repo.root, &["list"]);
    assert_success("wt", &["list"], &output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("No managed worktrees"));
}

#[test]
fn create_from_subdirectory_saves_relative_root_for_path() {
    let repo = TestRepo::new();
    let worktree = repo.managed_worktree("api-change");
    let cwd = repo.root.join("crates/api");

    let output = run_wt(&cwd, &["create", "api-change", "--no-open"]);
    assert_success("wt", &["create", "api-change", "--no-open"], &output);

    let output = run_wt(&repo.root, &["path", "api-change"]);
    assert_success("wt", &["path", "api-change"], &output);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        worktree.join("crates/api").display().to_string()
    );
}

#[test]
fn create_refuses_existing_branch_without_existing_flag() {
    let repo = TestRepo::new();
    run_git(&repo.root, &["branch", "feature"]);

    let output = run_wt(&repo.root, &["create", "feature", "--no-open"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("branch feature already exists; pass --existing to reuse it"));
    assert!(!repo.managed_worktree("feature").exists());
}

#[test]
fn create_refuses_missing_branch_with_existing_flag() {
    let repo = TestRepo::new();

    let output = run_wt(
        &repo.root,
        &["create", "missing", "--existing", "--no-open"],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("branch missing does not exist; omit --existing to create it"));
    assert!(!repo.managed_worktree("missing").exists());
}

#[test]
fn create_can_reuse_an_existing_branch() {
    let repo = TestRepo::new();
    let worktree = repo.managed_worktree("hotfix-wt");
    run_git(&repo.root, &["branch", "hotfix"]);

    let output = run_wt(
        &repo.root,
        &[
            "create",
            "hotfix-wt",
            "--branch",
            "hotfix",
            "--existing",
            "--no-open",
        ],
    );
    assert_success(
        "wt",
        &[
            "create",
            "hotfix-wt",
            "--branch",
            "hotfix",
            "--existing",
            "--no-open",
        ],
        &output,
    );
    assert!(worktree.is_dir());

    let output = run_wt(&repo.root, &["list"]);
    assert_success("wt", &["list"], &output);
    assert!(String::from_utf8_lossy(&output.stdout).contains(&format!(
        "hotfix-wt\thotfix\tmanaged\t{}",
        worktree.display()
    )));
}

#[test]
fn delete_refuses_dirty_worktree_without_force() {
    let repo = TestRepo::new();
    let worktree = repo.managed_worktree("dirty");

    let output = run_wt(&repo.root, &["create", "dirty", "--no-open"]);
    assert_success("wt", &["create", "dirty", "--no-open"], &output);
    fs::write(worktree.join("dirty.txt"), "uncommitted\n").expect("dirty worktree");

    let output = run_wt(&repo.root, &["delete", "dirty"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("uncommitted changes"));

    let output = run_wt(&repo.root, &["delete", "dirty", "--force"]);
    assert_success("wt", &["delete", "dirty", "--force"], &output);
    assert!(!worktree.exists());
}
