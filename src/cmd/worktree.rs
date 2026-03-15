use std::{
    path::Path,
    process::{Command, Stdio},
};

use anyhow::Context;

use crate::{argparse, worktree};

pub fn create(args: &argparse::WorktreeCreate) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let repo = git2::Repository::discover(&cwd).context("git")?;
    let repo_workdir = repo
        .workdir()
        .context("bare repositories are not supported")?;

    let main_repo = worktree::resolve_main_repo_path(repo_workdir)
        .context("failed to resolve main repository path")?;

    let worktree_root = worktree::resolve_worktree_root(args.config.as_deref())?;
    let destination = worktree::build_worktree_destination(&main_repo, &args.name, &worktree_root)
        .context("failed to build worktree destination")?;

    ensure_destination_parent_exists(&destination)?;
    ensure_destination_missing(&destination)?;

    run_git_worktree_add(&main_repo, &destination, &args.name)?;

    println!("{}", destination.display());
    Ok(())
}

fn ensure_destination_parent_exists(destination: &Path) -> anyhow::Result<()> {
    let parent = destination
        .parent()
        .context("worktree destination has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create destination parent `{:?}`", parent))
}

fn ensure_destination_missing(destination: &Path) -> anyhow::Result<()> {
    if destination.exists() {
        anyhow::bail!(
            "worktree destination already exists: `{}`",
            destination.display()
        );
    }
    Ok(())
}

fn run_git_worktree_add(main_repo: &Path, destination: &Path, name: &str) -> anyhow::Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(main_repo)
        .args(["worktree", "add", "-b"])
        .arg(name)
        .arg(destination)
        .stdin(Stdio::null())
        .status()
        .with_context(|| {
            format!(
                "failed to execute `git worktree add` for repository `{}`",
                main_repo.display()
            )
        })?;

    if !status.success() {
        anyhow::bail!(
            "`git worktree add` failed for destination `{}`",
            destination.display()
        );
    }
    Ok(())
}
