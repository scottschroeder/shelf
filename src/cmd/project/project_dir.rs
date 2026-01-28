use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use skim::SkimItem;

use crate::config::ProjectGroup;

#[derive(Debug, Clone)]
pub struct Project {
    pub path: PathBuf,
    pub typename: String,
    pub title: String,
}

pub struct ProjectExtractor<'a> {
    config: &'a ProjectGroup,
    extract_regex: regex::Regex,
}

impl<'a> ProjectExtractor<'a> {
    pub fn new(config: &ProjectGroup) -> anyhow::Result<ProjectExtractor<'_>> {
        let extract_regex = regex::Regex::new(&config.extract)?;
        Ok(ProjectExtractor {
            config,
            extract_regex,
        })
    }
    pub fn extract(&self, dir: &Path, parent: Option<&Project>) -> Option<Project> {
        let path_text = dir.to_string_lossy();

        let title = self
            .extract_regex
            .captures(&path_text)
            .and_then(|captures| captures.get(1))
            .map(|m| m.as_str().to_string())?;

        let typename = if let Some(parent) = parent {
            format!("{}/{}", parent.typename, parent.title)
        } else {
            self.config.title.clone()
        };

        Some(Project {
            path: dir.to_path_buf(),
            typename,
            title,
        })
    }
}

impl SkimItem for Project {
    fn text(&self) -> std::borrow::Cow<'_, str> {
        Cow::Owned(format!("[{}] {}", self.typename, self.title))
    }
}
