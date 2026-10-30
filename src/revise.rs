//! Apply the channel owner's change request to a finished run, e.g. "make the
//! thumbnail about the pressure instead" or "scene 12 is confusing, show the
//! valve opening".
//!
//! The LLM decides what the request touches. Packaging changes go to
//! package.rs as the owner's notes. Video changes replace the affected scenes
//! in storyboard.json and resume the run: unchanged scenes reuse their
//! narration and renders, so only what changed costs TTS credits or render
//! time. A request that states a general preference also becomes an (owner)
//! lesson, so every later video follows it too.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::pipeline::{self, AppState, Progress, VideoRequest};
use crate::storyboard::{Scene, Storyboard, json_body};
use crate::{lessons, llm, package, review};

#[derive(Deserialize)]
struct Plan {
    #[serde(default)]
    scenes: Vec<SceneEdit>,
    #[serde(default)]
    packaging_notes: String,
    #[serde(default)]
    lesson: String,
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize)]
struct SceneEdit {
    index: usize,
    scene: Value,
}

/// What a change request did to the video.
pub struct Edited {
    /// Scenes replaced and re-rendered (empty: the request was about packaging only).
    pub scenes: Vec<usize>,
    pub packaging_notes: String,
    pub lesson: String,
    pub summary: String,
}

pub async fn run(state: &AppState, run_id: &str, instruction: &str) -> AppResult<String> {
    let cfg = &state.cfg;
    let plan = edit_scenes(state, run_id, instruction).await?;
    let mut done = Vec::new();
    if !plan.scenes.is_empty() {
        let edited: Vec<String> = plan.scenes.iter().map(|i| i.to_string()).collect();
        done.push(format!("re-rendered scene(s) {}", edited.join(", ")));
    }

    // Packaging follows the video: re-package after any change, with the owner's words as notes.
    let notes = format!("{instruction}\n{}", plan.packaging_notes);
    package::run(state, run_id, Some(&notes)).await?;
    done.push("re-packaged (title, description, tags, thumbnail)".into());
    if !plan.scenes.is_empty() {
        let _ = review::run(state, run_id).await;
    }

    if !plan.lesson.trim().is_empty() {
        lessons::add(&state.client, cfg, &[format!("{} {}", lessons::OWNER, plan.lesson.trim())]).await?;
        done.push(format!("saved for future videos: {}", plan.lesson.trim()));
    }
    Ok(format!("{}\n{}", plan.summary.trim(), done.join("; ")))
}

/// Let the LLM rewrite the scenes `instruction` touches, save them to
/// storyboard.json and resume the run, so only those scenes are narrated and
/// rendered again. Packaging is left to the caller.
pub async fn edit_scenes(state: &AppState, run_id: &str, instruction: &str) -> AppResult<Edited> {
    pipeline::validate_run_id(run_id)?;
    let cfg = &state.cfg;
    let dir = pipeline::run_dir(cfg, run_id);
    let board_path = dir.join("storyboard.json");
    let mut board: Storyboard = serde_json::from_slice(
        &tokio::fs::read(&board_path).await.map_err(|_| AppError::NotFound(format!("run {run_id}")))?,
    )?;
    let youtube = tokio::fs::read_to_string(dir.join("youtube.txt")).await.unwrap_or_default();

    let system = r#"You apply the channel owner's change request to a finished animated explainer video.
You get the storyboard (scenes with their full JSON) and the current YouTube packaging.

Return ONE JSON object:
{"scenes": [{"index": n, "scene": {full replacement scene JSON, same schema as the storyboard's scenes}}],
 "packaging_notes": "what to change in the title, description, tags or thumbnail, or \"\"",
 "lesson": "if the request states a general preference for ALL future videos, that rule in <= 30 words; else \"\"",
 "summary": "one or two sentences telling the owner what you changed"}

Rules:
- Change only what the request asks for. "scenes" is empty when only the packaging changes.
- Replacement scenes keep the same layouts' content schema, keep every "cue" a word that appears in that
  scene's narration, and keep narration length within 20% of the original so the video still fits its length.
- Write numbers in narration as words. Don't add audio tags the original style doesn't use."#;
    let user = format!(
        "## Change request\n{instruction}\n\n## Current packaging (youtube.txt)\n{youtube}\n\n## Storyboard\n{}",
        serde_json::to_string(&board)?
    );
    let raw = llm::complete(&state.client, cfg, system, &user, true).await?;
    let plan: Plan = json_body(&raw)
        .and_then(|b| serde_json::from_str(b).map_err(|e| e.to_string()))
        .map_err(|e| AppError::Llm(format!("revise: {e}")))?;

    if !plan.scenes.is_empty() {
        for edit in &plan.scenes {
            let scene: Scene = serde_json::from_value(edit.scene.clone())
                .map_err(|e| AppError::Llm(format!("revise: scene {}: {e}", edit.index)))?;
            let slot = board
                .scenes
                .get_mut(edit.index)
                .ok_or_else(|| AppError::Llm(format!("revise: no scene {}", edit.index)))?;
            *slot = scene;
        }
        board.normalize(&state.catalog);
        tokio::fs::write(&board_path, serde_json::to_vec_pretty(&board)?).await?;
        // Everything derived from the old video is stale now.
        for stale in ["contact.png", "review.json"] {
            let _ = tokio::fs::remove_file(dir.join(stale)).await;
        }
        let req: VideoRequest = serde_json::from_slice(&tokio::fs::read(dir.join("request.json")).await?)?;
        pipeline::run(state, run_id, req, Arc::new(|_p: Progress| {})).await?;
    }
    Ok(Edited {
        scenes: plan.scenes.iter().map(|e| e.index).collect(),
        packaging_notes: plan.packaging_notes,
        lesson: plan.lesson,
        summary: plan.summary,
    })
}
