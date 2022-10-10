use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

const QUALIFIER: &str = "io";
const ORGANIZATION: &str = "scottschroeder";
const APP: &str = "shelf";
const CONFIG_NAME: &str = "shelf.yml";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProjectGroup {
    pub root: PathBuf,
    #[serde(default)]
    pub exclude: Vec<String>,
    pub title: String,
    pub extract: String,
    #[serde(default)]
    pub recurse: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShelfConfig {
    pub projects: Vec<ProjectGroup>,
}

fn read_config(config_path: &Path) -> anyhow::Result<ShelfConfig> {
    let cf = std::fs::File::open(config_path)
        .with_context(|| format!("could not open config at `{:?}`", config_path))?;
    serde_yaml::from_reader(cf)
        .with_context(|| format!("could not parse config at `{:?}`", config_path))
}

pub fn load_config(config_override: Option<&Path>) -> anyhow::Result<ShelfConfig> {
    if let Some(config_path) = config_override {
        read_config(config_path)
    } else {
        let dirs = directories::ProjectDirs::from(QUALIFIER, ORGANIZATION, APP).unwrap();
        let config_path = dirs.config_dir().join(CONFIG_NAME);
        read_config(&config_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loadconfig() {
        let conf = r###"
            projects:
              - root: ~/src/local/
                title: "Local"
                extract: src/local/(.*)
        "###;

        let config: ShelfConfig = serde_yaml::from_str(conf).unwrap();

        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.projects[0].title, "Local");
    }
}
