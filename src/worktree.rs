use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::Context;

use crate::config::load_config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorktreeInfo {
    pub(crate) main_repo_path: PathBuf,
    pub(crate) worktree_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkedWorktree {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
}

pub(crate) fn resolve_worktree_root(config_override: Option<&Path>) -> anyhow::Result<PathBuf> {
    let config = load_config(config_override)?;
    if let Some(root) = config.worktrees.root {
        return Ok(root);
    }

    if let Some(config_override) = config_override {
        anyhow::bail!(
            "missing `worktrees.root` in config `{}`",
            config_override.display()
        );
    }

    anyhow::bail!(
        "missing `worktrees.root` in default config; set it in ~/.config/shelf/shelf.yml or pass --config"
    )
}

pub(crate) fn inspect_repo_worktree(repo_path: &Path) -> anyhow::Result<Option<WorktreeInfo>> {
    let dot_git = repo_path.join(".git");
    if !dot_git.is_file() {
        return Ok(None);
    }

    let gitdir = read_gitdir_from_file(&dot_git)?;
    let gitdir = resolve_gitdir_path(repo_path, &gitdir);

    Ok(
        parse_worktree_gitdir(&gitdir).map(|(main_repo_path, worktree_name)| WorktreeInfo {
            main_repo_path,
            worktree_name,
        }),
    )
}

pub(crate) fn resolve_main_repo_path(repo_path: &Path) -> anyhow::Result<PathBuf> {
    if let Some(info) = inspect_repo_worktree(repo_path)? {
        Ok(info.main_repo_path)
    } else {
        Ok(repo_path.to_path_buf())
    }
}

pub(crate) fn list_linked_worktrees(main_repo_path: &Path) -> anyhow::Result<Vec<LinkedWorktree>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(main_repo_path)
        .args(["worktree", "list", "--porcelain"])
        .stdin(Stdio::null())
        .output()
        .with_context(|| {
            format!(
                "failed to execute `git worktree list --porcelain` for `{}`",
                main_repo_path.display()
            )
        })?;

    if !output.status.success() {
        anyhow::bail!(
            "`git worktree list --porcelain` failed for `{}`",
            main_repo_path.display()
        );
    }

    let text = String::from_utf8(output.stdout).context("worktree list output was not utf-8")?;
    let entries = parse_worktree_list_porcelain(&text)
        .into_iter()
        .filter(|entry| entry.path != main_repo_path)
        .collect();
    Ok(entries)
}

pub(crate) fn build_worktree_destination(
    main_repo_path: &Path,
    worktree_name: &str,
    worktree_root: &Path,
) -> anyhow::Result<PathBuf> {
    let parent = worktree_root.join(slug_repo_path(main_repo_path));
    Ok(parent.join(worktree_name))
}

fn read_gitdir_from_file(dot_git_path: &Path) -> anyhow::Result<PathBuf> {
    let contents = std::fs::read_to_string(dot_git_path)
        .with_context(|| format!("failed to read `{:?}`", dot_git_path))?;
    let raw_path = contents
        .lines()
        .find_map(|line| line.strip_prefix("gitdir:").map(str::trim))
        .context("missing `gitdir:` entry in .git file")?;
    Ok(PathBuf::from(raw_path))
}

fn resolve_gitdir_path(repo_path: &Path, gitdir: &Path) -> PathBuf {
    if gitdir.is_absolute() {
        gitdir.to_path_buf()
    } else {
        repo_path.join(gitdir)
    }
}

fn parse_worktree_gitdir(gitdir: &Path) -> Option<(PathBuf, String)> {
    let worktree_name = gitdir.file_name()?.to_string_lossy().to_string();
    let worktrees_dir = gitdir.parent()?;
    if worktrees_dir.file_name()? != "worktrees" {
        return None;
    }

    let dot_git = worktrees_dir.parent()?;
    if dot_git.file_name()? != ".git" {
        return None;
    }

    let main_repo = dot_git.parent()?.to_path_buf();
    Some((main_repo, worktree_name))
}

fn slug_repo_path(main_repo_path: &Path) -> String {
    let raw = main_repo_path.display().to_string();
    let mut slug = String::with_capacity(raw.len());
    let mut prev_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    while slug.starts_with('-') {
        slug.remove(0);
    }
    if slug.is_empty() {
        slug.push_str("repo");
    }

    let mut hasher = DefaultHasher::new();
    main_repo_path.hash(&mut hasher);
    let hash = hasher.finish();

    format!("{}-{:x}", slug, hash)
}

fn parse_worktree_list_porcelain(text: &str) -> Vec<LinkedWorktree> {
    text.split("\n\n")
        .flat_map(|block| block.lines())
        .filter_map(|line| {
            line.strip_prefix("worktree ")
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
        })
        .map(|p| {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_string())
                .unwrap_or_else(|| p.display().to_string());
            LinkedWorktree { path: p, name }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("valid clock")
            .as_nanos();
        std::env::temp_dir().join(format!("shelf-{}-{}", name, nanos))
    }

    #[test]
    fn parse_worktree_gitdir_from_standard_path() {
        let gitdir = Path::new("/home/alice/src/github/acme/foo/.git/worktrees/feature-123");
        let parsed = parse_worktree_gitdir(gitdir).expect("expected worktree path");

        assert_eq!(parsed.0, PathBuf::from("/home/alice/src/github/acme/foo"));
        assert_eq!(parsed.1, "feature-123");
    }

    #[test]
    fn ignore_non_worktree_gitdir_path() {
        let gitdir = Path::new("/home/alice/src/github/acme/foo/.git/modules/bar");
        let parsed = parse_worktree_gitdir(gitdir);

        assert!(parsed.is_none());
    }

    #[test]
    fn destination_nests_when_repo_under_src_root() {
        let repo = Path::new("/home/alice/src/github/acme/foo");
        let worktree_root = Path::new("/home/alice/src/worktrees");

        let destination =
            build_worktree_destination(repo, "feature-123", worktree_root).expect("destination");
        let expected = worktree_root.join(slug_repo_path(repo)).join("feature-123");
        assert_eq!(destination, expected);
    }

    #[test]
    fn destination_uses_slug_when_repo_outside_src_root() {
        let repo = Path::new("/opt/company/repo");
        let worktree_root = Path::new("/home/alice/src/worktrees");

        let destination =
            build_worktree_destination(repo, "feature-123", worktree_root).expect("destination");

        assert_eq!(
            destination.file_name(),
            Some(std::ffi::OsStr::new("feature-123"))
        );
        let parent = destination.parent().expect("destination has parent");
        assert!(parent.starts_with(worktree_root));
        assert_ne!(parent, worktree_root);
        assert_eq!(parent, worktree_root.join(slug_repo_path(repo)));
    }

    #[test]
    fn parse_worktree_list_porcelain_extracts_paths_and_names() {
        let text = "worktree /home/alice/src/github/org/repo\nHEAD 111\nbranch refs/heads/main\n\nworktree /home/alice/src/worktrees/github/org/repo/example\nHEAD 222\nbranch refs/heads/example\n\n";

        let parsed = parse_worktree_list_porcelain(text);

        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[1],
            LinkedWorktree {
                path: PathBuf::from("/home/alice/src/worktrees/github/org/repo/example"),
                name: "example".to_string(),
            }
        );
    }

    #[test]
    fn resolve_worktree_root_uses_config_when_present() {
        let config_dir = unique_temp_path("config");
        std::fs::create_dir_all(&config_dir).expect("create temp config dir");
        let config_path = config_dir.join("shelf.yml");
        std::fs::write(&config_path, "worktrees:\n  root: /tmp/custom-worktrees\n")
            .expect("write config");

        let resolved = resolve_worktree_root(Some(&config_path)).expect("resolve worktree root");
        assert_eq!(resolved, PathBuf::from("/tmp/custom-worktrees"));

        std::fs::remove_file(&config_path).expect("cleanup config file");
        std::fs::remove_dir_all(&config_dir).expect("cleanup config dir");
    }

    #[test]
    fn resolve_worktree_root_errors_when_missing() {
        let config_dir = unique_temp_path("config-missing-root");
        std::fs::create_dir_all(&config_dir).expect("create temp config dir");
        let config_path = config_dir.join("shelf.yml");
        std::fs::write(&config_path, "projects: []\n").expect("write config");

        let err =
            resolve_worktree_root(Some(&config_path)).expect_err("expected missing root error");
        let err_text = format!("{}", err);
        assert!(err_text.contains("missing `worktrees.root`"));

        std::fs::remove_file(&config_path).expect("cleanup config file");
        std::fs::remove_dir_all(&config_dir).expect("cleanup config dir");
    }
}
