//! LLM-written one-off scenes (`layout: "custom"`).
//!
//! For each custom scene: ask the LLM for a `build(ctx)` file, statically
//! check + dry-run it with `python -m story_engine validate`, and feed any
//! error back for another attempt. Validated code is cached per scene, so a
//! resumed run doesn't pay for it again. If every attempt fails, the scene
//! renders its `content.fallback` layout instead — one bad diagram never
//! breaks the video.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::process::Command;

use crate::config::Config;
use crate::error::AppResult;
use crate::llm::{self, SceneCodeRequest};
use crate::media::run_cmd;
use crate::pipeline::AppState;

#[derive(Serialize, Deserialize, PartialEq)]
struct CodeMeta {
    brief: String,
    narration: String,
    duration_ms: u64,
}

/// Generate + validate code for every custom scene. `specs[i]` is the full
/// render spec for scene i. Returns scene index -> validated code path.
pub async fn prepare(state: &AppState, specs: &[Value], dir: &Path) -> HashMap<usize, PathBuf> {
    let cfg = &state.cfg;
    let jobs: Vec<usize> = specs
        .iter()
        .enumerate()
        .filter(|(_, s)| s["scene"]["layout"].as_str() == Some("custom"))
        .map(|(i, _)| i)
        .collect();
    if jobs.is_empty() {
        return HashMap::new();
    }
    if !cfg.custom_scenes || !cfg.has_llm() {
        tracing::info!(scenes = ?jobs, "custom scenes disabled or no LLM key; using their fallbacks");
        return HashMap::new();
    }
    stream::iter(jobs.into_iter().map(|i| async move {
        match prepare_one(state, &specs[i], i, dir).await {
            Ok(Some(p)) => Some((i, p)),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(scene = i, error = %e, "custom scene generation errored; using fallback");
                None
            }
        }
    }))
    .buffer_unordered(2)
    .filter_map(|x| async move { x })
    .collect()
    .await
}

async fn prepare_one(state: &AppState, spec: &Value, i: usize, dir: &Path) -> AppResult<Option<PathBuf>> {
    let cfg = &state.cfg;
    let scene = &spec["scene"];
    let brief = scene["content"]["brief"].as_str().unwrap_or("").trim().to_string();
    if brief.is_empty() {
        return Ok(None);
    }
    let code_path = dir.join("scenes").join(format!("scene_{i:02}.custom.py"));
    let meta_path = dir.join("scenes").join(format!("scene_{i:02}.custom.json"));
    let meta = CodeMeta {
        brief: brief.clone(),
        narration: scene["narration"].as_str().unwrap_or("").to_string(),
        duration_ms: (spec["duration"].as_f64().unwrap_or(0.0) * 1000.0).round() as u64,
    };
    if code_path.exists()
        && let Ok(prev) = tokio::fs::read(&meta_path).await
        && serde_json::from_slice::<CodeMeta>(&prev).ok().as_ref() == Some(&meta)
    {
        tracing::info!(scene = i, "reusing validated custom scene code");
        return Ok(Some(code_path));
    }

    let theme: crate::catalog::Theme = serde_json::from_value(spec["theme"].clone())?;
    let words: Vec<(String, f64)> = spec["words"]
        .as_array()
        .map(|ws| ws.iter().map(|w| (w["w"].as_str().unwrap_or("").to_string(), w["s"].as_f64().unwrap_or(0.0))).collect())
        .unwrap_or_default();
    let validate_spec = dir.join("scenes").join(format!("scene_{i:02}.validate.json"));
    let mut prev_code: Option<String> = None;
    let mut prev_err: Option<String> = None;

    for attempt in 1..=cfg.custom_attempts {
        let req = SceneCodeRequest {
            brief: &brief,
            narration: meta.narration.as_str(),
            words: &words,
            duration: spec["duration"].as_f64().unwrap_or(5.0),
            content_end: spec["content_end"].as_f64().unwrap_or(5.0),
            theme: &theme,
            portrait: spec["height"].as_u64() > spec["width"].as_u64(),
            title: scene["title"].as_str(),
            previous_code: prev_code.as_deref(),
            previous_error: prev_err.as_deref(),
        };
        let code = match llm::write_scene_code(&state.client, cfg, &state.catalog, &req).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(scene = i, attempt, error = %e, "custom scene: LLM call failed");
                prev_err = Some(e.to_string());
                continue;
            }
        };
        tokio::fs::write(&code_path, &code).await?;
        let mut vs = spec.clone();
        vs["custom_code"] = json!(code_path.to_string_lossy());
        tokio::fs::write(&validate_spec, serde_json::to_vec_pretty(&vs)?).await?;
        match validate(cfg, &validate_spec).await {
            Ok(()) => {
                tracing::info!(scene = i, attempt, "custom scene code validated");
                tokio::fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?).await?;
                return Ok(Some(code_path));
            }
            Err(err) => {
                tracing::warn!(scene = i, attempt, error = %err.lines().next().unwrap_or(""), "custom scene failed validation");
                prev_code = Some(code);
                prev_err = Some(err);
            }
        }
    }
    let _ = tokio::fs::remove_file(&code_path).await;
    let _ = tokio::fs::remove_file(&meta_path).await;
    tracing::warn!(scene = i, "custom scene could not be validated; using its fallback layout");
    Ok(None)
}

/// Run the engine's dry-run validator; Err carries the message for the LLM.
async fn validate(cfg: &Config, spec_path: &Path) -> Result<(), String> {
    let out = run_cmd(
        Command::new(&cfg.python_bin)
            .args(["-m", "story_engine", "validate", "--spec"])
            .arg(spec_path)
            .current_dir(cfg.engine_dir())
            .env("PYTHONIOENCODING", "utf-8"),
        240,
    )
    .await;
    let text = match out {
        Ok(s) => s,
        Err(e) => e.to_string(),
    };
    let verdict = text
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .unwrap_or_else(|| json!({ "ok": false, "error": text.chars().take(1500).collect::<String>() }));
    if verdict["ok"].as_bool() == Some(true) {
        Ok(())
    } else {
        Err(verdict["error"].as_str().unwrap_or("validation failed").chars().take(2000).collect())
    }
}
