//! Growing the mechanism kit: parts built on demand and kept for reuse.
//!
//! When a storyboard's `process` scene asks for a part kind the kit doesn't
//! have (with a `make` line saying how to draw it), the LLM writes it against
//! the engine's PART_API, `python -m story_engine validate-part` sandbox-checks
//! and dry-runs it (build, scale, every action), errors go back for another
//! attempt, and a part that passes is saved to
//! `engine/story_engine/kit/library/<kind>.py`. Every later video can use it.
//!
//! A part that can't be validated is simply not saved: the engine draws a
//! labelled block in its place, so a missing part never breaks a render.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tokio::process::Command;

use crate::config::Config;
use crate::llm::{self, PartCodeRequest};
use crate::media::run_cmd;
use crate::pipeline::AppState;
use crate::storyboard::{Storyboard, new_part_requests};

/// New parts built per video at most (each costs a few LLM calls).
const MAX_NEW_PARTS: usize = 3;

pub fn library_dir(cfg: &Config) -> PathBuf {
    cfg.engine_dir().join("story_engine").join("kit").join("library")
}

/// Build and store every new part the storyboard needs. Returns the kinds added.
pub async fn grow(state: &AppState, board: &Storyboard, run_dir: &Path, topic: &str) -> Vec<String> {
    let cfg = &state.cfg;
    let requests = new_part_requests(&board.scenes, &state.catalog);
    if requests.is_empty() {
        return Vec::new();
    }
    if !cfg.has_llm() {
        tracing::info!(parts = ?requests.iter().map(|r| &r.0).collect::<Vec<_>>(), "no LLM key; new parts draw as blocks");
        return Vec::new();
    }
    let lib = library_dir(cfg);
    let _ = tokio::fs::create_dir_all(&lib).await;
    let mut added = Vec::new();
    for (kind, make, actions, labels) in requests.into_iter().take(MAX_NEW_PARTS) {
        let target = lib.join(format!("{kind}.py"));
        if target.exists() {
            continue; // another video made it since the catalog was loaded
        }
        match build_one(state, &kind, &make, &actions, &labels, topic, run_dir).await {
            Some(code) => {
                let header = format!(
                    "# Kit part \"{kind}\", generated for \"{}\" and kept for reuse.\n# Validated with \
                     `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).\n",
                    topic.replace('\n', " ").chars().take(80).collect::<String>()
                );
                match tokio::fs::write(&target, format!("{header}{code}\n")).await {
                    Ok(()) => {
                        tracing::info!(part = %kind, file = %target.display(), "new kit part saved to the library");
                        added.push(kind);
                    }
                    Err(e) => tracing::warn!(part = %kind, error = %e, "could not save kit part"),
                }
            }
            None => tracing::warn!(part = %kind, "kit part could not be validated; it will draw as a labelled block"),
        }
    }
    added
}

async fn build_one(
    state: &AppState,
    kind: &str,
    make: &str,
    actions: &[String],
    labels: &[String],
    topic: &str,
    run_dir: &Path,
) -> Option<String> {
    let cfg = &state.cfg;
    let work = run_dir.join("scenes").join(format!("part_{kind}.py"));
    let mut prev_code: Option<String> = None;
    let mut prev_err: Option<String> = None;
    for attempt in 1..=cfg.custom_attempts {
        let req = PartCodeRequest {
            kind,
            make,
            actions,
            labels,
            topic,
            previous_code: prev_code.as_deref(),
            previous_error: prev_err.as_deref(),
        };
        let code = match llm::write_part_code(&state.client, cfg, &state.catalog, &req).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(part = kind, attempt, error = %e, "kit part: LLM call failed");
                prev_err = Some(e.to_string());
                continue;
            }
        };
        if tokio::fs::write(&work, &code).await.is_err() {
            return None;
        }
        match validate(cfg, &work, kind).await {
            Ok(()) => {
                tracing::info!(part = kind, attempt, "kit part validated");
                return Some(code);
            }
            Err(err) => {
                tracing::warn!(part = kind, attempt, error = %err.lines().next().unwrap_or(""), "kit part failed validation");
                prev_code = Some(code);
                prev_err = Some(err);
            }
        }
    }
    None
}

async fn validate(cfg: &Config, file: &Path, kind: &str) -> Result<(), String> {
    let out = run_cmd(
        Command::new(&cfg.python_bin)
            .args(["-m", "story_engine", "validate-part", "--file"])
            .arg(file)
            .args(["--name", kind])
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

/// For each scene in a `process` chain (consecutive process scenes joined by
/// `"continue": true`), the chain's contents and the scene's position in it.
/// Single scenes get `None`.
pub fn chains(board: &Storyboard) -> Vec<Option<(Vec<Value>, usize)>> {
    let n = board.scenes.len();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in 0..n {
        let s = &board.scenes[i];
        if s.layout != "process" {
            continue;
        }
        let joins = crate::storyboard::continues(s)
            && i > 0
            && board.scenes[i - 1].layout == "process"
            && groups.last().is_some_and(|g| g.last() == Some(&(i - 1)));
        if joins {
            groups.last_mut().expect("checked").push(i);
        } else {
            groups.push(vec![i]);
        }
    }
    let mut out = vec![None; n];
    for g in groups.into_iter().filter(|g| g.len() > 1) {
        let contents: Vec<Value> = g.iter().map(|&i| board.scenes[i].content.clone()).collect();
        for (pos, &i) in g.iter().enumerate() {
            out[i] = Some((contents.clone(), pos));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(layout: &str, content: Value) -> crate::storyboard::Scene {
        serde_json::from_value(json!({ "narration": "words here", "layout": layout, "content": content })).unwrap()
    }

    #[test]
    fn process_scenes_chain_only_with_continue() {
        let board = Storyboard {
            title: "T".into(),
            theme: "blueprint".into(),
            music: None,
            description: None,
            chapters: vec![],
            short: None,
            scenes: vec![
                scene("process", json!({"parts": [{"id": "a", "kind": "tank"}]})),
                scene("process", json!({"continue": true})),
                scene("stat", json!({})),
                scene("process", json!({"continue": true})),
                scene("process", json!({})),
            ],
        };
        let c = chains(&board);
        assert_eq!(c[0].as_ref().map(|x| (x.0.len(), x.1)), Some((2, 0)));
        assert_eq!(c[1].as_ref().map(|x| x.1), Some(1));
        assert!(c[2].is_none() && c[3].is_none() && c[4].is_none(), "a stat scene breaks the chain");
    }
}
