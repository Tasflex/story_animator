//! The visual vocabulary (layouts, icons, backgrounds, themes...) exported by
//! the Python engine. Loading it from `python -m story_engine catalog` keeps
//! the engine the single source of truth: the storyboard prompt and the
//! validator can never drift from what the renderer actually supports.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::media::run_cmd;

#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub name: String,
    pub description: String,
    pub bg: String,
    pub bg2: String,
    pub ink: String,
    pub muted: String,
    pub accent: String,
    pub accent2: String,
    pub accent3: String,
    pub dark: bool,
    #[serde(default)]
    pub font: String,
    /// "diagram" or "infographic"
    #[serde(default)]
    pub style: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    pub version: String,
    pub layouts: BTreeMap<String, Value>,
    pub icons: Vec<String>,
    pub backgrounds: BTreeMap<String, String>,
    pub themes: BTreeMap<String, Theme>,
    pub cameras: Vec<String>,
    pub transitions: Vec<String>,
    pub poses: Vec<String>,
    pub expressions: Vec<String>,
    pub looks: Vec<String>,
    /// Instructions + API reference for LLM-written `custom` scenes.
    #[serde(default)]
    pub custom_api: String,
    /// Whether LaTeX is available for `equation`/MathTex rendering.
    #[serde(default)]
    pub latex: bool,
}

impl Catalog {
    pub async fn load(cfg: &Config) -> AppResult<Self> {
        let out = run_cmd(
            tokio::process::Command::new(&cfg.python_bin)
                .args(["-m", "story_engine", "catalog"])
                .current_dir(cfg.engine_dir()),
            120,
        )
        .await
        .map_err(|e| {
            AppError::Config(format!(
                "could not load engine catalog via `{} -m story_engine catalog` — is the Python env set up? ({e})",
                cfg.python_bin
            ))
        })?;
        serde_json::from_str(&out).map_err(|e| AppError::Config(format!("bad catalog JSON: {e}")))
    }

    pub fn theme(&self, name: &str) -> &Theme {
        self.themes.get(name).or_else(|| self.themes.get("blueprint")).unwrap_or_else(|| {
            self.themes.values().next().expect("catalog has at least one theme")
        })
    }
}
