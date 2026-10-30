//! Self-review of a finished run, so the next video is better than this one.
//!
//! Two kinds of evidence:
//!   - measured: parts the kit can't draw (they render as placeholder boxes),
//!     scenes that fell back to a safe layout, custom scenes that failed, how
//!     much of the video shows a mechanism, how hard the speech was sped up,
//!     and every number shown on screen;
//!   - seen: a contact sheet of every scene (thumbnail.py) shown to the LLM
//!     with the script, so it judges what the viewer actually sees.
//!
//! The reviewer also gets the scorecard of earlier videos and says how this
//! one compares with the best so far, so scores stay calibrated over time.
//! The contact sheet ends with frames of the 9:16 short, which is reviewed too.
//!
//! The verdict goes to `<run>/review.json`; its lessons are merged into
//! `storage/LESSONS.md` (lessons.rs), which the writer reads for every new
//! video, and a good video's strengths go to the file's "What works" list.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::process::Command;

use crate::catalog::normalize_kind;
use crate::error::{AppError, AppResult};
use crate::pipeline::{self, AppState};
use crate::storyboard::{ICON_LED, MECHANISM, Storyboard, json_body};
use crate::{lessons, llm, media, niche};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub scenes: usize,
    pub narrated: usize,
    pub words: usize,
    pub speech_tempo: f64,
    pub mechanism_share: f64,
    pub icon_share: f64,
    /// Process parts with no drawing in the kit: they render as labelled placeholder boxes.
    pub placeholder_parts: Vec<String>,
    /// Scenes that failed to render and were replaced by a safe layout.
    pub render_fallbacks: Vec<usize>,
    /// `custom` scenes whose code failed validation (their fallback layout was used).
    pub custom_failed: Vec<usize>,
    /// Scenes of the 9:16 short that failed to render and used a safe layout.
    #[serde(default)]
    pub short_fallbacks: Vec<usize>,
    /// Numbers and claims shown on screen (stat, fun_fact, scale), to fact-check.
    pub on_screen: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Review {
    pub score: f64,
    #[serde(default)]
    pub strengths: Vec<String>,
    #[serde(default)]
    pub problems: Vec<String>,
    #[serde(default)]
    pub questionable_facts: Vec<String>,
    #[serde(default)]
    pub lessons: Vec<String>,
    /// How this video compares with the best earlier one, in a sentence.
    #[serde(default)]
    pub vs_best: String,
    #[serde(default)]
    pub metrics: Metrics,
}

/// A strength is only worth repeating if the video it came from was good.
const STRENGTH_MIN_SCORE: f64 = 7.5;

pub async fn run(state: &AppState, run_id: &str) -> AppResult<Review> {
    pipeline::validate_run_id(run_id)?;
    let cfg = &state.cfg;
    let dir = pipeline::run_dir(cfg, run_id);
    let board: Storyboard = read(&dir.join("storyboard.json")).await?;
    let timeline: Value = read(&dir.join("timeline.json")).await?;
    let metrics = measure(state, &board, &timeline, &dir).await;
    let sheet = contact_sheet(state, &dir).await;

    let system = format!(
        r#"You are the executive producer of an animated YouTube channel. You review a finished video to make
the NEXT video better. Be honest and specific; praise is only useful if it says what to keep doing.

{brief}

The image (if present) is a contact sheet: one frame from each scene of the 16:9 video, numbered in the
top-left corner. If it has a row marked SHORT, those are frames of the 9:16 vertical short, rendered natively
for phones, with the hook title burned into the top band and captions into the bottom band.

## Look for
- In the frames: placeholder boxes (empty rounded rectangles standing in for a part), empty or sparse
  frames, clutter, overlapping or cut-off text, unreadable labels, the same layout over and over, frames
  that don't show what the narration says.
- In the script: a hook in the first sentence, whether every middle chapter shows the mechanism working,
  whether each wow lands, pacing, and numbers that may be wrong or can't be checked.
- In the SHORT frames: anything cut off, too small to read on a phone, or overlapping the hook title or the
  captions; empty frames; whether the first frame hooks.
- The measured metrics: placeholder_parts, render_fallbacks, custom_failed and short_fallbacks are visible
  failures; speech_tempo above 1.08 sounds rushed; icon_share above 0.2 means too few mechanism scenes.
- Earlier videos (below): score on the same scale as before. Say plainly whether this one is better or worse
  than the best so far, and why.

## Output
ONE JSON object:
{{"score": 1-10 (9-10 only if you'd be proud to publish it as is; placeholder boxes cap it at 6),
  "strengths": ["what to keep doing, 1-3 items"],
  "problems": ["concrete problem with scene numbers, most damaging first"],
  "questionable_facts": ["on-screen number or claim that may be wrong or unverifiable, and why"],
  "vs_best": "one sentence: better / about the same / worse than the best earlier video, and the main reason",
  "lessons": ["2-5 rules for FUTURE videos: imperative, general (not about this topic), <= 30 words each,
              and not already in the lessons list below. Only what the storyboard writer controls: the script,
              layouts, how many parts and labels a diagram has, label length, numbers, pacing. Rendering bugs
              (overlap the writer can't prevent, broken drawing code) go in problems, not lessons."]}}"#,
        brief = niche::BRIEF,
    );
    let user = format!(
        "## Measured\n{metrics}\n\n## Script (scene: [layout] chapter title: narration | content)\n{script}\n\n{history}\n\n{lessons}",
        metrics = serde_json::to_string_pretty(&metrics)?,
        script = script_outline(&board),
        history = lessons::history_block(cfg, run_id).await,
        lessons = lessons::prompt_block(cfg).await,
    );
    let raw = match &sheet {
        Some(png) => llm::complete_vision(&state.client, cfg, &system, &user, png, true).await?,
        None => llm::complete(&state.client, cfg, &system, &user, true).await?,
    };
    let mut review: Review = json_body(&raw)
        .and_then(|b| serde_json::from_str(b).map_err(|e| e.to_string()))
        .map_err(|e| AppError::Llm(format!("review: {e}")))?;
    review.score = review.score.clamp(1.0, 10.0);
    review.metrics = metrics;
    tokio::fs::write(dir.join("review.json"), serde_json::to_vec_pretty(&review)?).await?;

    lessons::add(&state.client, cfg, &review.lessons).await?;
    if review.score >= STRENGTH_MIN_SCORE {
        lessons::add_strengths(&state.client, cfg, &review.strengths).await?;
    }
    let worst = review.problems.first().map(String::as_str).unwrap_or("");
    lessons::score(cfg, run_id, &board.title, review.score, worst).await?;
    Ok(review)
}

async fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> AppResult<T> {
    let bytes = tokio::fs::read(path).await.map_err(|_| AppError::NotFound(path.display().to_string()))?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn measure(state: &AppState, board: &Storyboard, timeline: &Value, dir: &Path) -> Metrics {
    let narrated: Vec<_> = board.scenes.iter().filter(|s| !s.is_silent()).collect();
    let share = |names: &[&str]| {
        narrated.iter().filter(|s| names.contains(&s.layout.as_str())).count() as f64 / narrated.len().max(1) as f64
    };
    let mut m = Metrics {
        scenes: board.scenes.len(),
        narrated: narrated.len(),
        words: board.total_words(),
        speech_tempo: timeline["tempo"].as_f64().unwrap_or(1.0),
        mechanism_share: share(&MECHANISM),
        icon_share: share(&ICON_LED),
        ..Default::default()
    };
    let short_from = read::<Value>(&dir.join("short").join("short.json")).await.ok().and_then(|v| v["from"].as_u64());
    let short_from = short_from.map(|f| f as usize);
    for (i, s) in board.scenes.iter().enumerate() {
        let c = &s.content;
        if s.layout == "process" {
            for p in c["parts"].as_array().into_iter().flatten() {
                let kind = normalize_kind(p["kind"].as_str().unwrap_or(""));
                if !kind.is_empty() && !state.catalog.has_part(&kind) && !m.placeholder_parts.contains(&kind) {
                    m.placeholder_parts.push(kind);
                }
            }
        }
        let scenes = dir.join("scenes");
        if scenes.join(format!("scene_{i:02}.fallback.json")).exists() {
            m.render_fallbacks.push(i);
        }
        if let Some(from) = short_from
            && i >= from
            && dir.join("short").join("scenes").join(format!("scene_{:02}.fallback.json", i - from)).exists()
        {
            m.short_fallbacks.push(i);
        }
        if s.layout == "custom" {
            let spec: Option<Value> = read(&scenes.join(format!("scene_{i:02}.json"))).await.ok();
            if spec.is_none_or(|v| v.get("custom_code").is_none()) {
                m.custom_failed.push(i);
            }
        }
        match s.layout.as_str() {
            "stat" => m.on_screen.push(format!(
                "scene {i}: {}{}{} {} ({})",
                c["prefix"].as_str().unwrap_or(""),
                c["value"],
                c["suffix"].as_str().unwrap_or(""),
                c["label"].as_str().unwrap_or(""),
                c["context"].as_str().unwrap_or("")
            )),
            "fun_fact" => m.on_screen.push(format!("scene {i}: {}", c["fact"].as_str().unwrap_or(""))),
            "scale" => {
                let items: Vec<String> = c["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|it| format!("{} = {}", it["label"].as_str().unwrap_or(""), it["display"].as_str().unwrap_or("")))
                    .collect();
                m.on_screen.push(format!("scene {i}: {} ({})", c["callout"].as_str().unwrap_or(""), items.join(", ")));
            }
            _ => {}
        }
    }
    m
}

/// One line per scene: index, layout, chapter, title, narration and a clipped summary of its content.
pub fn script_outline(board: &Storyboard) -> String {
    board
        .scenes
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let content: String = s.content.to_string().chars().take(220).collect();
            format!(
                "{i:>2} [{}] ch{} {}: \"{}\" | {content}",
                s.layout,
                s.chapter.map(|c| c.to_string()).unwrap_or_else(|| "-".into()),
                s.title.as_deref().unwrap_or(""),
                s.narration
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `<run>/contact.png` (rendered once by the engine), or None if it can't be made.
pub async fn contact_sheet(state: &AppState, dir: &Path) -> Option<Vec<u8>> {
    let out = dir.join("contact.png");
    if !out.exists() {
        let res = media::run_cmd(
            Command::new(&state.cfg.python_bin)
                .args(["-m", "story_engine", "contact-sheet", "--run"])
                .arg(dir)
                .arg("--out")
                .arg(&out)
                .current_dir(state.cfg.engine_dir())
                .env("PYTHONIOENCODING", "utf-8"),
            600,
        )
        .await;
        if let Err(e) = res {
            tracing::warn!(error = %e, "contact sheet failed; reviewing the script only");
            return None;
        }
    }
    tokio::fs::read(&out).await.ok()
}

/// JSON for the bot and the CLI: score, problems, lessons and the measured failures.
pub fn summary(r: &Review) -> Value {
    json!({
        "score": r.score,
        "problems": r.problems,
        "questionable_facts": r.questionable_facts,
        "lessons": r.lessons,
        "vs_best": r.vs_best,
        "short_fallbacks": r.metrics.short_fallbacks,
        "placeholder_parts": r.metrics.placeholder_parts,
        "render_fallbacks": r.metrics.render_fallbacks,
        "custom_failed": r.metrics.custom_failed,
    })
}
