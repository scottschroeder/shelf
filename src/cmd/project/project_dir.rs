use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use skim::SkimItem;

use crate::{
    config::{NamedColor, ProjectGroup},
    skim_style,
};

#[derive(Debug, Clone)]
pub struct WorktreeProjectMetadata {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub path: PathBuf,
    pub typename: String,
    pub title: String,
    pub worktree: Option<WorktreeProjectMetadata>,
    pub project_color: Option<NamedColor>,
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
            worktree: None,
            project_color: self.config.color,
        })
    }
}

impl SkimItem for Project {
    fn text(&self) -> std::borrow::Cow<'_, str> {
        Cow::Owned(self.plain_display_text())
    }

    fn display<'a>(&'a self, _context: skim::DisplayContext<'a>) -> skim::AnsiString<'a> {
        skim::AnsiString::parse(&self.styled_display_text())
    }
}

impl Project {
    fn plain_display_text(&self) -> String {
        if let Some(worktree) = &self.worktree {
            format!(
                "[{}] {} (worktree {})",
                self.typename, self.title, worktree.name
            )
        } else {
            format!("[{}] {}", self.typename, self.title)
        }
    }

    fn styled_display_text(&self) -> String {
        let mut text = skim_style::project_tag_style(self.project_color)
            .paint(format!("[{}]", self.typename))
            .to_string();
        text.push(' ');
        text.push_str(&self.title);

        if let Some(worktree) = &self.worktree {
            text.push(' ');
            text.push_str(
                &skim_style::worktree_suffix_style()
                    .paint(format!("(worktree {})", worktree.name))
                    .to_string(),
            );
        }

        text
    }

    pub fn from_manual_directory(path: PathBuf, label: Option<String>) -> Self {
        let title = label.unwrap_or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| path.display().to_string())
        });

        Self {
            path,
            typename: "config".to_string(),
            title,
            worktree: None,
            project_color: None,
        }
    }

    pub fn with_worktree_metadata(mut self, metadata: Option<WorktreeProjectMetadata>) -> Self {
        self.worktree = metadata;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(text: &str) -> String {
        let re = regex::Regex::new("\\x1b\\[[0-9;]*m").expect("valid regex");
        re.replace_all(text, "").to_string()
    }

    fn project_fixture() -> Project {
        Project {
            path: PathBuf::from("/tmp/demo"),
            typename: "work".to_string(),
            title: "demo".to_string(),
            worktree: None,
            project_color: Some(NamedColor::Cyan),
        }
    }

    #[test]
    fn plain_project_text_format_is_unchanged() {
        let project = project_fixture();
        assert_eq!(project.text().as_ref(), "[work] demo");
    }

    #[test]
    fn plain_worktree_text_format_is_unchanged() {
        let mut project = project_fixture();
        project.worktree = Some(WorktreeProjectMetadata {
            name: "feature-123".to_string(),
        });

        assert_eq!(
            project.text().as_ref(),
            "[work] demo (worktree feature-123)"
        );
    }

    #[test]
    fn styled_output_preserves_same_text_content() {
        let mut project = project_fixture();
        project.worktree = Some(WorktreeProjectMetadata {
            name: "feature-123".to_string(),
        });

        let styled = project.styled_display_text();
        assert!(styled.contains("\x1b["));
        assert_eq!(
            strip_ansi(&styled),
            "[work] demo (worktree feature-123)".to_string()
        );
    }
}
