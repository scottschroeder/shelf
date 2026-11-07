use std::{
    borrow::Cow,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use anyhow::Context;
use serde::Serialize;
use skim::{
    prelude::{Key, SkimOptionsBuilder},
    Skim, SkimItem, SkimItemReceiver, SkimItemSender,
};

use crate::{argparse, config, skim_style, worktree};

pub fn create(args: &argparse::WorktreeCreate) -> anyhow::Result<()> {
    let (_, main_repo) = resolve_repo_from_cwd()?;
    let branch = if args.detach {
        None
    } else {
        Some(args.branch.as_deref().unwrap_or(&args.name))
    };
    let destination = add_worktree(
        &main_repo,
        &args.name,
        branch,
        args.detach,
        args.commitish.as_deref(),
        args.config.as_deref(),
    )?;
    println!("{}", destination.display());
    Ok(())
}

pub fn interactive(args: &argparse::WorktreeInteractive) -> anyhow::Result<()> {
    let (_, main_repo) = resolve_repo_from_cwd()?;

    if let Some(name) = args.name.as_deref() {
        return jump_or_create(&main_repo, name, args);
    }

    let details = worktree::list_linked_worktree_details(&main_repo)?;
    if details.is_empty() {
        anyhow::bail!("no linked worktrees available");
    }

    let candidates: Vec<Arc<CleanupCandidate>> = details
        .into_iter()
        .map(|detail| Arc::new(CleanupCandidate::from_details(detail, &main_repo)))
        .collect();

    match select_worktree_or_query(candidates) {
        Selection::Worktree(candidate) => {
            println!("{}", candidate.path.display());
            Ok(())
        }
        Selection::Create(query) => jump_or_create(&main_repo, &query, args),
        Selection::Abort => anyhow::bail!("no worktree selected"),
    }
}

fn jump_or_create(
    main_repo: &Path,
    name: &str,
    args: &argparse::WorktreeInteractive,
) -> anyhow::Result<()> {
    let name = name.trim();
    anyhow::ensure!(!name.is_empty(), "worktree name is empty");
    let details = worktree::list_linked_worktree_details(main_repo)?;

    if let Some(found) = details.iter().find(|detail| detail.name == name) {
        println!("{}", found.path.display());
        return Ok(());
    }

    let branch = args.branch.as_deref();
    let destination = add_worktree(
        main_repo,
        name,
        branch,
        branch.is_none(),
        None,
        args.config.as_deref(),
    )?;
    println!("{}", destination.display());
    Ok(())
}

/// Resolve the current directory to `(repo_workdir, main_repo)`, where
/// `main_repo` is the primary checkout when run from inside a linked worktree.
fn resolve_repo_from_cwd() -> anyhow::Result<(PathBuf, PathBuf)> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let repo = git2::Repository::discover(&cwd).context("git")?;
    let repo_workdir = repo
        .workdir()
        .context("bare repositories are not supported")?
        .to_path_buf();
    let main_repo = worktree::resolve_main_repo_path(&repo_workdir)
        .context("failed to resolve main repository path")?;
    Ok((repo_workdir, main_repo))
}

fn add_worktree(
    main_repo: &Path,
    name: &str,
    branch: Option<&str>,
    detach: bool,
    commitish: Option<&str>,
    config_path: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    let worktree_root = worktree::resolve_worktree_root(config_path)?;
    let destination = worktree::build_worktree_destination(main_repo, name, &worktree_root)
        .context("failed to build worktree destination")?;

    ensure_destination_parent_exists(&destination)?;
    ensure_destination_missing(&destination)?;

    run_git_worktree_add(main_repo, &destination, branch, detach, commitish)?;

    if let Ok(cfg) = config::load_config(config_path) {
        symlink_setup_paths(main_repo, &destination, &cfg.worktrees.setup);
    }

    Ok(destination)
}

#[derive(Debug, Clone, Serialize)]
struct CleanupCandidate {
    path: std::path::PathBuf,
    main_repo: std::path::PathBuf,
    name: String,
    branch: Option<String>,
    upstream: Option<String>,
    commit_message: Option<String>,
    dirty: bool,
    detached: bool,
    locked: bool,
    prunable: bool,
    #[serde(skip)]
    display_str: skim::AnsiString<'static>,
}

impl CleanupCandidate {
    fn from_details(
        details: worktree::LinkedWorktreeDetails,
        main_repo: &Path,
    ) -> CleanupCandidate {
        Self::from_details_with_repo(details, None, main_repo)
    }

    fn from_details_with_repo(
        details: worktree::LinkedWorktreeDetails,
        repo_label: Option<&str>,
        main_repo: &Path,
    ) -> CleanupCandidate {
        let branch = details
            .branch_ref
            .as_deref()
            .and_then(|name| name.strip_prefix("refs/heads/"))
            .map(ToOwned::to_owned);
        let upstream = find_upstream_branch(&details.path, branch.as_deref());
        let commit_message = find_head_commit_message(&details.path);
        let dirty = is_worktree_dirty(&details.path);

        let display_name = match repo_label {
            Some(label) => format!("{}/{}", label, details.name),
            None => details.name.clone(),
        };

        let mut tags = Vec::new();
        let mut styled_tags = Vec::new();
        if details.detached {
            let tag = "(detached)".to_string();
            tags.push(tag.clone());
            styled_tags.push(skim_style::detached_style().paint(tag).to_string());
        } else if let Some(branch) = &branch {
            let tag = format!("(branch {})", branch);
            tags.push(tag.clone());
            styled_tags.push(skim_style::branch_style().paint(tag).to_string());
        }
        if let Some(upstream) = &upstream {
            let tag = format!("(upstream {})", upstream);
            tags.push(tag.clone());
            styled_tags.push(skim_style::upstream_style().paint(tag).to_string());
        }
        if dirty {
            let tag = "(dirty)".to_string();
            tags.push(tag.clone());
            styled_tags.push(skim_style::dirty_style().paint(tag).to_string());
        } else {
            let tag = "(clean)".to_string();
            tags.push(tag.clone());
            styled_tags.push(skim_style::clean_style().paint(tag).to_string());
        }
        if details.locked {
            let tag = "(locked)".to_string();
            tags.push(tag.clone());
            styled_tags.push(skim_style::locked_style().paint(tag).to_string());
        }
        if details.prunable {
            let tag = "(prunable)".to_string();
            tags.push(tag.clone());
            styled_tags.push(skim_style::prunable_style().paint(tag).to_string());
        }

        let mut styled_text = if styled_tags.is_empty() {
            skim_style::worktree_name_style(dirty)
                .paint(format!("[{}]", display_name))
                .to_string()
        } else {
            format!(
                "{} {}",
                skim_style::worktree_name_style(dirty).paint(format!("[{}]", display_name)),
                styled_tags.join(" ")
            )
        };

        if let Some(commit_message) = &commit_message {
            styled_text.push(' ');
            styled_text.push_str(
                &skim_style::commit_message_style()
                    .paint(commit_message)
                    .to_string(),
            );
        }

        CleanupCandidate {
            path: details.path,
            main_repo: main_repo.to_path_buf(),
            name: display_name,
            branch,
            upstream,
            commit_message,
            dirty,
            detached: details.detached,
            locked: details.locked,
            prunable: details.prunable,
            display_str: skim::AnsiString::parse(&styled_text),
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

pub fn cleanup_all(args: &argparse::WorktreeCleanupAll) -> anyhow::Result<()> {
    let worktree_root = worktree::resolve_worktree_root(args.config.as_deref())?;
    let all_repos = worktree::discover_all_worktrees_from_root(&worktree_root)?;

    let mut candidates: Vec<Arc<CleanupCandidate>> = Vec::new();
    for (main_repo, linked_worktrees) in all_repos {
        let repo_label = main_repo
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        for details in linked_worktrees {
            candidates.push(Arc::new(CleanupCandidate::from_details_with_repo(
                details,
                Some(&repo_label),
                &main_repo,
            )));
        }
    }

    if args.json {
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        let entries: Vec<_> = candidates.iter().map(Arc::as_ref).collect();
        serde_json::to_writer_pretty(&mut output, &entries)?;
        use std::io::Write;
        writeln!(output)?;
        return Ok(());
    }

    if args.list {
        for candidate in &candidates {
            println!("{} — {}", candidate.text(), candidate.path.display());
        }
        return Ok(());
    }

    if candidates.is_empty() {
        anyhow::bail!("no linked worktrees found under worktree root");
    }

    let selected = select_worktrees_to_cleanup(candidates);
    if selected.is_empty() {
        anyhow::bail!("no worktrees selected for cleanup");
    }

    let mut failures = Vec::new();
    for selected_worktree in &selected {
        if let Err(err) =
            run_git_worktree_remove(&selected_worktree.main_repo, &selected_worktree.path)
        {
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

pub fn cleanup(_args: &argparse::WorktreeCleanup) -> anyhow::Result<()> {
    let (repo_workdir, main_repo) = resolve_repo_from_cwd()?;

    let candidates = build_cleanup_candidates(&main_repo, &repo_workdir)?;

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

fn symlink_setup_paths(main_repo: &Path, destination: &Path, setup: &[String]) {
    for relative in setup {
        let source = main_repo.join(relative);
        if !source.exists() {
            continue;
        }
        let target = destination.join(relative);
        if target.exists() || target.symlink_metadata().is_ok() {
            continue;
        }
        if let Some(parent) = target.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                log::warn!(
                    "worktree setup: failed to create parent dirs for `{}`: {}",
                    target.display(),
                    err
                );
                continue;
            }
        }
        if let Err(err) = std::os::unix::fs::symlink(&source, &target) {
            log::warn!(
                "worktree setup: failed to symlink `{}` -> `{}`: {}",
                target.display(),
                source.display(),
                err
            );
        }
    }
}

fn build_worktree_add_args(
    branch: Option<&str>,
    detach: bool,
    commitish: Option<&str>,
    destination: &Path,
) -> anyhow::Result<Vec<OsString>> {
    let mut git_args: Vec<OsString> = vec!["worktree".into(), "add".into()];

    if detach {
        git_args.push("--detach".into());
    } else {
        let branch =
            branch.context("branch name required when not creating a detached worktree")?;
        git_args.push("-b".into());
        git_args.push(branch.into());
    }

    git_args.push(destination.as_os_str().to_os_string());

    if let Some(commitish) = commitish {
        git_args.push(commitish.into());
    }

    Ok(git_args)
}

fn run_git_worktree_add(
    main_repo: &Path,
    destination: &Path,
    branch: Option<&str>,
    detach: bool,
    commitish: Option<&str>,
) -> anyhow::Result<()> {
    let git_args = build_worktree_add_args(branch, detach, commitish, destination)?;
    let status = Command::new("git")
        .arg("-C")
        .arg(main_repo)
        .args(&git_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
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
        candidates.push(Arc::new(CleanupCandidate::from_details(details, main_repo)));
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

enum Selection {
    Worktree(CleanupCandidate),
    Create(String),
    Abort,
}

/// Single-select picker: choosing a candidate switches to it; pressing Enter
/// on a query that matched nothing creates a worktree named after the query.
fn select_worktree_or_query(candidates: Vec<Arc<CleanupCandidate>>) -> Selection {
    let options = SkimOptionsBuilder::default().multi(false).build().unwrap();

    let (send, recv): (SkimItemSender, SkimItemReceiver) = skim::prelude::unbounded();
    for candidate in candidates {
        if let Err(err) = send.send(candidate) {
            log::error!("unable to send worktree candidate: {}", err);
            break;
        }
    }
    drop(send);

    let Some(result) = Skim::run_with(&options, Some(recv)) else {
        return Selection::Abort;
    };
    if result.is_abort {
        return Selection::Abort;
    }

    if let Some(candidate) = result
        .selected_items
        .first()
        .and_then(|item| item.as_any().downcast_ref::<CleanupCandidate>())
    {
        return Selection::Worktree(candidate.clone());
    }

    if result.final_key == Key::Enter {
        let query = result.query.trim();
        if valid_worktree_name(query) {
            return Selection::Create(query.to_string());
        }
    }

    Selection::Abort
}

fn valid_worktree_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\0') && name != "." && name != ".."
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
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
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

    use super::{build_worktree_add_args, build_worktree_remove_args, valid_worktree_name};

    fn to_strings(args: Vec<std::ffi::OsString>) -> Vec<String> {
        args.into_iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect()
    }

    fn add_args(branch: Option<&str>, detach: bool, commitish: Option<&str>) -> Vec<String> {
        to_strings(
            build_worktree_add_args(branch, detach, commitish, Path::new("/tmp/wt")).unwrap(),
        )
    }

    #[test]
    fn creates_branch_named_after_worktree() {
        assert_eq!(
            add_args(Some("handle-foo"), false, None),
            vec!["worktree", "add", "-b", "handle-foo", "/tmp/wt"]
        );
    }

    #[test]
    fn supports_distinct_branch_name() {
        assert_eq!(
            add_args(Some("alice/feature-x"), false, None),
            vec!["worktree", "add", "-b", "alice/feature-x", "/tmp/wt"]
        );
    }

    #[test]
    fn supports_detached_creation_with_commitish() {
        assert_eq!(
            add_args(None, true, Some("origin/main")),
            vec!["worktree", "add", "--detach", "/tmp/wt", "origin/main"]
        );
    }

    #[test]
    fn supports_branch_creation_from_commitish() {
        assert_eq!(
            add_args(Some("handle-foo"), false, Some("origin/main")),
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
    fn interactive_defaults_to_detached_creation() {
        assert_eq!(
            add_args(None, true, None),
            vec!["worktree", "add", "--detach", "/tmp/wt"]
        );
    }

    #[test]
    fn rejects_branchless_non_detached_creation() {
        assert!(build_worktree_add_args(None, false, None, Path::new("/tmp/wt")).is_err());
    }

    #[test]
    fn cleanup_removes_worktree_with_force() {
        let got = to_strings(build_worktree_remove_args(Path::new("/tmp/wt")));

        assert_eq!(got, vec!["worktree", "remove", "--force", "/tmp/wt"]);
    }

    #[test]
    fn accepts_plain_worktree_names() {
        assert!(valid_worktree_name("foo"));
        assert!(valid_worktree_name("handle-foo_2.bar"));
    }

    #[test]
    fn rejects_unusable_worktree_names() {
        for name in ["", "foo/bar", ".", "..", "foo\0bar"] {
            assert!(
                !valid_worktree_name(name),
                "expected `{}` to be rejected",
                name
            );
        }
    }
}
