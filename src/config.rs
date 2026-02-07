use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

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
pub struct ManualDirectory {
    pub path: PathBuf,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShelfConfig {
    #[serde(default)]
    pub projects: Vec<ProjectGroup>,
    #[serde(default)]
    pub directories: Vec<ManualDirectory>,
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
        let config_path = get_xdg_config_path()?;
        read_config(&config_path)
    }
}

fn get_xdg_config_path() -> anyhow::Result<PathBuf> {
    if let Ok(xdg_home) = std::env::var("XDG_CONFIG_HOME") {
        let xdg_home = xdg_home.trim();
        if !xdg_home.is_empty() {
            return Ok(PathBuf::from(xdg_home).join(APP).join(CONFIG_NAME));
        }
    }

    let home = std::env::var("HOME").context("HOME is not set and XDG_CONFIG_HOME is empty")?;
    let home = home.trim();
    if home.is_empty() {
        anyhow::bail!("HOME is empty and XDG_CONFIG_HOME is not set");
    }

    Ok(PathBuf::from(home)
        .join(".config")
        .join(APP)
        .join(CONFIG_NAME))
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
        assert_eq!(config.directories.len(), 0);
    }

    #[test]
    fn loadconfig_with_manual_directories() {
        let conf = r###"
            projects:
              - root: ~/src/local/
                title: "Local"
                extract: src/local/(.*)
            directories:
              - path: ~/src/local/scratch
                label: "Scratch"
              - path: ~/src/local/playground
        "###;

        let config: ShelfConfig = serde_yaml::from_str(conf).unwrap();

        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.directories.len(), 2);
        assert_eq!(config.directories[0].label.as_deref(), Some("Scratch"));
        assert!(config.directories[1].label.is_none());
    }

    #[test]
    fn loadconfig_directories_only() {
        let conf = r###"
            directories:
              - path: ~/src/local/scratch
                label: "Scratch"
        "###;

        let config: ShelfConfig = serde_yaml::from_str(conf).unwrap();

        assert_eq!(config.projects.len(), 0);
        assert_eq!(config.directories.len(), 1);
        assert_eq!(config.directories[0].label.as_deref(), Some("Scratch"));
    }
}
