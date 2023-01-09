use std::{collections::VecDeque, sync::Arc};

use project_dir::Project;
use skim::{prelude::SkimOptionsBuilder, Skim, SkimItemReceiver, SkimItemSender};

use self::project_dir::ProjectExtractor;
use crate::{
    argparse::{self, TmuxRename},
    config::{load_config, ProjectGroup},
    scan::scan_git_repos,
    tmux::get_tmux,
};

mod project_dir;

type ProjectQueue = VecDeque<(ProjectGroup, Option<Arc<Project>>)>;
const DEFAULT_TMUX_WINDOW_NAME: &str = "zsh";

pub fn dirs(args: &argparse::ProjectDirs) -> anyhow::Result<()> {
    let mut groups = Vec::new();
    for root in &args.roots {
        let path_text = root.to_string_lossy();
        groups.push(ProjectGroup {
            root: root.to_path_buf(),
            exclude: Vec::new(),
            title: format!("{}", path_text),
            extract: format!("{}/(.*)", path_text),
            recurse: args.git_recurse,
        });
    }
    let project = search(groups)?;
    update_tmux_and_display_results(&project, args.tmux_rename.as_ref())
}
pub fn preset(args: &argparse::ProjectPreset) -> anyhow::Result<()> {
    let config = load_config(args.config.as_deref())?;
    let project = search(config.projects)?;
    update_tmux_and_display_results(&project, args.tmux_rename.as_ref())
}

fn update_tmux_and_display_results(
    project: &Project,
    tmux_rename: Option<&TmuxRename>,
) -> anyhow::Result<()> {
    if let Some(tmux_rename) = tmux_rename {
        rename_tmux_default_window(&project.title, tmux_rename)?;
    }
    println!("{}", project.path.display());
    Ok(())
}

fn search(groups: Vec<ProjectGroup>) -> anyhow::Result<Project> {
    log::debug!("groups: {:#?}", groups);

    let mut queue: ProjectQueue = VecDeque::new();
    for root in groups {
        queue.push_back((root, None))
    }

    let (send, recv): (SkimItemSender, SkimItemReceiver) = skim::prelude::unbounded();
    std::thread::spawn(move || scan_groups(queue, send));
    let resp = select_and_return_first(recv);

    if let Some(proj) = resp {
        return Ok(proj);
    }

    anyhow::bail!("no item was selected");
}

fn scan_groups(mut queue: ProjectQueue, send: SkimItemSender) -> anyhow::Result<()> {
    let default_config = ProjectGroup {
        root: "".into(),
        exclude: Vec::new(),
        title: "unknown".to_string(),
        extract: "(.*)".to_string(),
        recurse: false,
    };
    let default_extract = ProjectExtractor::new(&default_config).expect("bad config");

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
            let proj = Arc::new(proj);
            if let Err(e) = send.send(proj.clone()) {
                anyhow::bail!("channel send failure for `{:?}`: {}", proj.path, e);
            };
            // println!("{:?}", x);
            if group_config.recurse {
                let mut new_group = group_config.clone();
                new_group.root = proj.path.clone();
                queue.push_back((new_group, Some(proj)));
            }
        }
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
            .get(0)?
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
                if tmux.count_tmux_panes()? > 1 && tmux.get_tmux_name()? != DEFAULT_TMUX_WINDOW_NAME
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
