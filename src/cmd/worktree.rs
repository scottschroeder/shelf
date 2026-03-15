use std::{
    borrow::Cow,
    ffi::OsString,
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
};

use anyhow::Context;
use skim::{prelude::SkimOptionsBuilder, Skim, SkimItem, SkimItemReceiver, SkimItemSender};

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

    run_git_worktree_add(&main_repo, &destination, args)?;

    println!("{}", destination.display());
    Ok(())
}

#[derive(Debug, Clone)]
struct CleanupCandidate {
    path: std::path::PathBuf,
    name: String,
    branch: Option<String>,
    upstream: Option<String>,
    commit_message: Option<String>,
    dirty: bool,
    detached: bool,
    locked: bool,
    prunable: bool,
    display_str: skim::AnsiString<'static>,
}

impl CleanupCandidate {
    fn from_details(details: worktree::LinkedWorktreeDetails) -> CleanupCandidate {
        let branch = details
            .branch_ref
            .as_deref()
            .and_then(|name| name.strip_prefix("refs/heads/"))
            .map(ToOwned::to_owned);
        let upstream = find_upstream_branch(&details.path, branch.as_deref());
        let commit_message = find_head_commit_message(&details.path);
        let dirty = is_worktree_dirty(&details.path);

        let mut tags = Vec::new();
        if details.detached {
            tags.push("(detached)".to_string());
        } else if let Some(branch) = &branch {
            tags.push(format!("(branch {})", branch));
        }
        if let Some(upstream) = &upstream {
            tags.push(format!("(upstream {})", upstream));
        }
        if dirty {
            tags.push("(dirty)".to_string());
        } else {
            tags.push("(clean)".to_string());
        }
        if details.locked {
            tags.push("(locked)".to_string());
        }
        if details.prunable {
            tags.push("(prunable)".to_string());
        }

        let mut text = if tags.is_empty() {
            format!("[{}]", details.name)
        } else {
            format!("[{}] {}", details.name, tags.join(" "))
        };
        if let Some(commit_message) = &commit_message {
            text.push(' ');
            text.push_str(commit_message);
        }

        CleanupCandidate {
            path: details.path,
            name: details.name,
            branch,
            upstream,
            commit_message,
            dirty,
            detached: details.detached,
            locked: details.locked,
            prunable: details.prunable,
            display_str: skim::AnsiString::parse(&text),
        }
    }
}

impl SkimItem for CleanupCandidate {
    fn text(&self) -> Cow<'_, str> {
        let mut parts = vec![self.name.clone()];
        if let Some(branch) = &self.branch {
            parts.push(branch.clone());
        }
        if let Some(upstream) = &self.upstream {
            parts.push(upstream.clone());
        }
        if let Some(commit_message) = &self.commit_message {
            parts.push(commit_message.clone());
        }
        if self.dirty {
            parts.push("dirty".to_string());
        }
        if self.detached {
            parts.push("detached".to_string());
        }
        if self.locked {
            parts.push("locked".to_string());
        }
        if self.prunable {
            parts.push("prunable".to_string());
        }
        Cow::Owned(parts.join(" "))
    }

    fn display<'a>(&'a self, _context: skim::DisplayContext<'a>) -> skim::AnsiString<'a> {
        self.display_str.clone()
    }
}

pub fn cleanup(_args: &argparse::WorktreeCleanup) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let repo = git2::Repository::discover(&cwd).context("git")?;
    let repo_workdir = repo
        .workdir()
        .context("bare repositories are not supported")?;
    let main_repo = worktree::resolve_main_repo_path(repo_workdir)
        .context("failed to resolve main repository path")?;

    let candidates = build_cleanup_candidates(&main_repo, repo_workdir)?;
    if candidates.is_empty() {
        anyhow::bail!("no linked worktrees available to clean up");
    }

    let selected = select_worktrees_to_cleanup(candidates);
    if selected.is_empty() {
        anyhow::bail!("no worktrees selected for cleanup");
    }

    let mut failures = Vec::new();
    for selected_worktree in selected {
        if let Err(err) = run_git_worktree_remove(&main_repo, &selected_worktree.path) {
            failures.push(format!("{}: {}", selected_worktree.path.display(), err));
            continue;
        }
        println!("{}", selected_worktree.path.display());
    }

    if !failures.is_empty() {
        anyhow::bail!(
            "failed to remove some selected worktrees:\n{}",
            failures.join("\n")
        );
    }

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

fn build_worktree_add_args(args: &argparse::WorktreeCreate, destination: &Path) -> Vec<OsString> {
    let mut git_args: Vec<OsString> = vec!["worktree".into(), "add".into()];

    if args.detach {
        git_args.push("--detach".into());
    } else {
        git_args.push("-b".into());
        git_args.push(args.branch.as_deref().unwrap_or(&args.name).into());
    }

    git_args.push(destination.as_os_str().to_os_string());

    if let Some(commitish) = &args.commitish {
        git_args.push(commitish.into());
    }

    git_args
}

fn run_git_worktree_add(
    main_repo: &Path,
    destination: &Path,
    args: &argparse::WorktreeCreate,
) -> anyhow::Result<()> {
    let git_args = build_worktree_add_args(args, destination);
    let status = Command::new("git")
        .arg("-C")
        .arg(main_repo)
        .args(&git_args)
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

fn build_cleanup_candidates(
    main_repo: &Path,
    current_workdir: &Path,
) -> anyhow::Result<Vec<Arc<CleanupCandidate>>> {
    let linked_worktrees = worktree::list_linked_worktree_details(main_repo)?;
    let mut candidates = Vec::new();

    for details in linked_worktrees {
        if details.path == current_workdir {
            continue;
        }
        candidates.push(Arc::new(CleanupCandidate::from_details(details)));
    }

    Ok(candidates)
}

fn select_worktrees_to_cleanup(candidates: Vec<Arc<CleanupCandidate>>) -> Vec<CleanupCandidate> {
    let options = SkimOptionsBuilder::default().multi(true).build().unwrap();

    let (send, recv): (SkimItemSender, SkimItemReceiver) = skim::prelude::unbounded();
    for candidate in candidates {
        if let Err(err) = send.send(candidate) {
            log::error!("unable to send worktree candidate: {}", err);
            break;
        }
    }
    drop(send);

    let Some(result) = Skim::run_with(&options, Some(recv)) else {
        return Vec::new();
    };
    if result.is_abort {
        return Vec::new();
    }

    result
        .selected_items
        .iter()
        .filter_map(|item| item.as_any().downcast_ref::<CleanupCandidate>().cloned())
        .collect()
}

fn build_worktree_remove_args(destination: &Path) -> Vec<OsString> {
    vec![
        "worktree".into(),
        "remove".into(),
        "--force".into(),
        destination.as_os_str().to_os_string(),
    ]
}

fn run_git_worktree_remove(main_repo: &Path, destination: &Path) -> anyhow::Result<()> {
    let git_args = build_worktree_remove_args(destination);
    let status = Command::new("git")
        .arg("-C")
        .arg(main_repo)
        .args(&git_args)
        .stdin(Stdio::null())
        .status()
        .with_context(|| {
            format!(
                "failed to execute `git worktree remove --force` for repository `{}`",
                main_repo.display()
            )
        })?;

    if !status.success() {
        anyhow::bail!(
            "`git worktree remove --force` failed for destination `{}`",
            destination.display()
        );
    }

    Ok(())
}

fn find_upstream_branch(path: &Path, local_branch: Option<&str>) -> Option<String> {
    let local_branch = local_branch?;
    let repo = git2::Repository::open(path).ok()?;
    let branch = repo
        .find_branch(local_branch, git2::BranchType::Local)
        .ok()?;
    let upstream = branch.upstream().ok()?;
    let upstream_name = upstream.name().ok().flatten()?;
    Some(
        upstream_name
            .strip_prefix("refs/remotes/")
            .unwrap_or(upstream_name)
            .to_string(),
    )
}

fn is_worktree_dirty(path: &Path) -> bool {
    let repo = match git2::Repository::open(path) {
        Ok(repo) => repo,
        Err(err) => {
            log::debug!("failed to open worktree `{:?}`: {}", path, err);
            return false;
        }
    };

    let mut status_opts = git2::StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .include_ignored(false);

    let statuses = repo.statuses(Some(&mut status_opts));
    match statuses {
        Ok(statuses) => !statuses.is_empty(),
        Err(err) => {
            log::debug!("failed to inspect worktree status `{:?}`: {}", path, err);
            false
        }
    }
}

fn find_head_commit_message(path: &Path) -> Option<String> {
    let repo = git2::Repository::open(path).ok()?;
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let message = String::from_utf8_lossy(commit.message_bytes());
    let first_line = message.lines().next().map(str::trim).unwrap_or("");
    if first_line.is_empty() {
        return None;
    }
    Some(first_line.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{build_worktree_add_args, build_worktree_remove_args};
    use crate::argparse::WorktreeCreate;

    fn mk_args(
        name: &str,
        branch: Option<&str>,
        detach: bool,
        commitish: Option<&str>,
    ) -> WorktreeCreate {
        WorktreeCreate {
            name: name.to_string(),
            branch: branch.map(ToString::to_string),
            detach,
            commitish: commitish.map(ToString::to_string),
            config: None,
        }
    }

    fn to_strings(args: Vec<std::ffi::OsString>) -> Vec<String> {
        args.into_iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn defaults_to_branch_named_after_worktree() {
        let args = mk_args("handle-foo", None, false, None);
        let got = to_strings(build_worktree_add_args(&args, Path::new("/tmp/wt")));

        assert_eq!(got, vec!["worktree", "add", "-b", "handle-foo", "/tmp/wt"]);
    }

    #[test]
    fn supports_distinct_branch_name() {
        let args = mk_args("handle-foo", Some("alice/feature-x"), false, None);
        let got = to_strings(build_worktree_add_args(&args, Path::new("/tmp/wt")));

        assert_eq!(
            got,
            vec!["worktree", "add", "-b", "alice/feature-x", "/tmp/wt"]
        );
    }

    #[test]
    fn supports_detached_creation_with_commitish() {
        let args = mk_args("handle-foo", None, true, Some("origin/main"));
        let got = to_strings(build_worktree_add_args(&args, Path::new("/tmp/wt")));

        assert_eq!(
            got,
            vec!["worktree", "add", "--detach", "/tmp/wt", "origin/main"]
        );
    }

    #[test]
    fn supports_branch_creation_from_commitish() {
        let args = mk_args("handle-foo", None, false, Some("origin/main"));
        let got = to_strings(build_worktree_add_args(&args, Path::new("/tmp/wt")));

        assert_eq!(
            got,
            vec![
                "worktree",
                "add",
                "-b",
                "handle-foo",
                "/tmp/wt",
                "origin/main"
            ]
        );
    }

    #[test]
    fn cleanup_removes_worktree_with_force() {
        let got = to_strings(build_worktree_remove_args(Path::new("/tmp/wt")));

        assert_eq!(got, vec!["worktree", "remove", "--force", "/tmp/wt"]);
    }
}
