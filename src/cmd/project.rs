use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};

use project_dir::Project;
use skim::{prelude::SkimOptionsBuilder, Skim, SkimItemReceiver, SkimItemSender};

use self::project_dir::ProjectExtractor;
use crate::{
    argparse::{self, TmuxRename},
    config::{load_config, ManualDirectory, ProjectGroup},
    scan::scan_git_repos,
    tmux::get_tmux,
    worktree,
};

mod project_dir;

type ProjectQueue = VecDeque<(ProjectGroup, Option<Arc<Project>>)>;

pub fn dirs(args: &argparse::ProjectDirs) -> anyhow::Result<()> {
    let mut groups = Vec::new();
    for root in &args.roots {
        let path_text = root.to_string_lossy();
        groups.push(ProjectGroup {
            root: root.to_path_buf(),
            exclude: Vec::new(),
            title: format!("{}", path_text),
            extract: format!("{}/(.*)", path_text),
            color: None,
            recurse: args.git_recurse,
        });
    }
    let project = search(groups, Vec::new())?;
    update_tmux_and_display_results(&project, args.tmux_rename.as_ref())
}
pub fn preset(args: &argparse::ProjectPreset) -> anyhow::Result<()> {
    let config = load_config(args.config.as_deref())?;
    let project = search(config.projects, config.directories)?;
    update_tmux_and_display_results(&project, args.tmux_rename.as_ref())
}

fn update_tmux_and_display_results(
    project: &Project,
    tmux_rename: Option<&TmuxRename>,
) -> anyhow::Result<()> {
    if let Some(tmux_rename) = tmux_rename {
        let window_name = tmux_window_name(project);
        rename_tmux_default_window(&window_name, tmux_rename)?;
    }
    println!("{}", project.path.display());
    Ok(())
}

fn tmux_window_name(project: &Project) -> String {
    if let Some(worktree) = &project.worktree {
        format!("{}({})", project.title, worktree.name)
    } else {
        project.title.clone()
    }
}

fn search(groups: Vec<ProjectGroup>, directories: Vec<ManualDirectory>) -> anyhow::Result<Project> {
    log::debug!("groups: {:#?}", groups);
    log::debug!("manual directories: {:#?}", directories);

    let mut queue: ProjectQueue = VecDeque::new();
    for root in groups {
        queue.push_back((root, None))
    }

    let (send, recv): (SkimItemSender, SkimItemReceiver) = skim::prelude::unbounded();
    std::thread::spawn(move || {
        if let Err(err) = send_manual_directories(&send, &directories) {
            log::error!("failed to send manual directories: {}", err);
        }
        if let Err(err) = scan_groups(queue, send) {
            log::error!("failed to scan project groups: {}", err);
        }
    });
    let resp = select_and_return_first(recv);

    if let Some(proj) = resp {
        return Ok(proj);
    }

    anyhow::bail!("no item was selected");
}

fn send_manual_directories(
    send: &SkimItemSender,
    directories: &[ManualDirectory],
) -> anyhow::Result<()> {
    let mut sent_paths: HashSet<std::path::PathBuf> = HashSet::new();
    for manual_directory in directories {
        let project = Project::from_manual_directory(
            manual_directory.path.clone(),
            manual_directory.label.clone(),
        );
        let (project, is_linked_worktree) =
            annotate_worktree_metadata(&manual_directory.path, project);
        send_project_if_new(send, &mut sent_paths, project.clone())?;
        if !is_linked_worktree {
            send_linked_worktree_projects(send, &mut sent_paths, &manual_directory.path, &project)?;
        }
    }
    Ok(())
}

fn scan_groups(mut queue: ProjectQueue, send: SkimItemSender) -> anyhow::Result<()> {
    let default_config = ProjectGroup {
        root: "".into(),
        exclude: Vec::new(),
        title: "unknown".to_string(),
        extract: "(.*)".to_string(),
        color: None,
        recurse: false,
    };
    let default_extract = ProjectExtractor::new(&default_config).expect("bad config");
    let mut sent_paths: HashSet<std::path::PathBuf> = HashSet::new();

    while let Some((group_config, parent)) = queue.pop_front() {
        let project_extract = ProjectExtractor::new(&group_config).expect("bad config");
        let ignore_set = regex::bytes::RegexSet::new(group_config.exclude.as_slice())
            .expect("bad exclude config");
        let parent_proj = parent.as_ref().map(|p| p.as_ref());
        for repo_path in scan_git_repos(&group_config.root, ignore_set) {
            let proj = project_extract
                .extract(&repo_path, parent_proj)
                .unwrap_or_else(|| {
                    default_extract
                        .extract(&repo_path, parent_proj)
                        .expect("default extraction config must return project")
                });

            let (proj, is_linked_worktree) = annotate_worktree_metadata(&repo_path, proj);
            send_project_if_new(&send, &mut sent_paths, proj.clone())?;
            if !is_linked_worktree {
                send_linked_worktree_projects(&send, &mut sent_paths, &repo_path, &proj)?;
            }

            // println!("{:?}", x);
            if group_config.recurse {
                let mut new_group = group_config.clone();
                new_group.root = proj.path.clone();
                queue.push_back((new_group, Some(Arc::new(proj))));
            }
        }
    }
    Ok(())
}

fn annotate_worktree_metadata(repo_path: &std::path::Path, proj: Project) -> (Project, bool) {
    match worktree::inspect_repo_worktree(repo_path) {
        Ok(Some(info)) => (
            proj.with_worktree_metadata(Some(project_dir::WorktreeProjectMetadata {
                name: info.worktree_name,
            })),
            true,
        ),
        Ok(None) => (proj, false),
        Err(err) => {
            log::debug!(
                "could not inspect worktree metadata for `{:?}`: {}",
                repo_path,
                err
            );
            (proj, false)
        }
    }
}

fn send_project_if_new(
    send: &SkimItemSender,
    sent_paths: &mut HashSet<std::path::PathBuf>,
    proj: Project,
) -> anyhow::Result<()> {
    if !sent_paths.insert(proj.path.clone()) {
        return Ok(());
    }
    let proj = Arc::new(proj);
    if let Err(e) = send.send(proj.clone()) {
        anyhow::bail!("channel send failure for `{:?}`: {}", proj.path, e);
    }
    Ok(())
}

fn send_linked_worktree_projects(
    send: &SkimItemSender,
    sent_paths: &mut HashSet<std::path::PathBuf>,
    repo_path: &std::path::Path,
    proj: &Project,
) -> anyhow::Result<()> {
    let linked_worktrees = match worktree::list_linked_worktrees(repo_path) {
        Ok(worktrees) => worktrees,
        Err(err) => {
            log::debug!(
                "could not list linked worktrees for `{:?}`: {}",
                repo_path,
                err
            );
            Vec::new()
        }
    };

    for linked in linked_worktrees {
        let worktree_project = Project {
            path: linked.path,
            typename: proj.typename.clone(),
            title: proj.title.clone(),
            worktree: Some(project_dir::WorktreeProjectMetadata { name: linked.name }),
            project_color: proj.project_color,
        };
        send_project_if_new(send, sent_paths, worktree_project)?;
    }
    Ok(())
}

fn select_and_return_first(recv: SkimItemReceiver) -> Option<Project> {
    let options = SkimOptionsBuilder::default()
        // .height(Some("50%"))
        .multi(false)
        .build()
        .unwrap();

    let result = Skim::run_with(&options, Some(recv))?;
    if result.is_abort {
        None
    } else {
        result
            .selected_items
            .first()?
            .as_any()
            .downcast_ref::<Project>()
            .cloned()
    }
}

fn rename_tmux_default_window(
    name: &str,
    tmux_rename: &argparse::TmuxRename,
) -> anyhow::Result<()> {
    if let Some(tmux) = get_tmux() {
        match tmux_rename {
            argparse::TmuxRename::DefaultOnly => {
                let default_name = get_default_tmux_window_name();
                let current_name = tmux.get_tmux_name()?;
                if tmux.count_tmux_panes()? > 1
                    && default_name
                        .as_deref()
                        .is_none_or(|name| current_name != name)
                {
                    return Ok(());
                }
            }
            argparse::TmuxRename::Force => {}
        }

        tmux.set_tmux_current_window_name(name)?;
    }
    Ok(())
}

fn get_default_tmux_window_name() -> Option<String> {
    let shell = std::env::var("SHELL").ok()?;
    let shell = shell.trim();
    if shell.is_empty() {
        return None;
    }
    std::path::Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{tmux_window_name, Project};
    use crate::cmd::project::project_dir::WorktreeProjectMetadata;

    #[test]
    fn tmux_window_name_uses_project_title_for_normal_repo() {
        let project = Project {
            path: PathBuf::from("/tmp/demo"),
            typename: "work".to_string(),
            title: "demo".to_string(),
            worktree: None,
            project_color: None,
        };

        assert_eq!(tmux_window_name(&project), "demo");
    }

    #[test]
    fn tmux_window_name_includes_worktree_suffix_for_worktrees() {
        let project = Project {
            path: PathBuf::from("/tmp/demo-worktree"),
            typename: "work".to_string(),
            title: "demo".to_string(),
            worktree: Some(WorktreeProjectMetadata {
                name: "feature-123".to_string(),
            }),
            project_color: None,
        };

        assert_eq!(tmux_window_name(&project), "demo(feature-123)");
    }
}
