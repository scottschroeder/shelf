use std::{collections::VecDeque, sync::Arc};

use project_dir::Project;
use skim::{prelude::SkimOptionsBuilder, Skim, SkimItemReceiver, SkimItemSender};

use self::project_dir::ProjectExtractor;
use crate::{
    argparse,
    config::{load_config, ProjectGroup},
    scan::scan_git_repos,
};

mod project_dir;

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
    search(groups)
}
pub fn preset(args: &argparse::ProjectPreset) -> anyhow::Result<()> {
    let config = load_config(args.config.as_deref())?;
    search(config.projects)
}

fn search(groups: Vec<ProjectGroup>) -> anyhow::Result<()> {
    log::debug!("groups: {:#?}", groups);

    let (send, recv): (SkimItemSender, SkimItemReceiver) = skim::prelude::unbounded();
    let mut queue: VecDeque<(ProjectGroup, Option<Arc<Project>>)> = VecDeque::new();
    for root in groups {
        queue.push_back((root, None))
    }
    std::thread::spawn(move || {
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
                    log::error!("channel send failure for `{:?}`: {}", proj.path, e)
                };
                // println!("{:?}", x);
                if group_config.recurse {
                    let mut new_group = group_config.clone();
                    new_group.root = proj.path.clone();
                    queue.push_back((new_group, Some(proj)));
                }
            }
        }
    });

    let options = SkimOptionsBuilder::default()
        // .height(Some("50%"))
        .multi(false)
        .build()
        .unwrap();

    let selected_items = Skim::run_with(&options, Some(recv))
        .map(|out| out.selected_items)
        .unwrap_or_else(Vec::new);
    let selected_projects = selected_items
        .iter()
        .map(|selected_item| {
            selected_item
                .as_any()
                .downcast_ref::<Project>()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();

    for item in selected_projects.iter().take(1) {
        println!("{}", item.path.display());
    }

    Ok(())
}
