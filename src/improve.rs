//! Auto-improvement: every video should be at least as good as the best one
//! before it.
//!
//! After the self-review (review.rs) scores a run, `improve` compares the
//! score with a target: the best score on the scorecard (storage/LESSONS.md)
//! or IMPROVE_TARGET, whichever is higher (capped at 9.5). Below it, the
//! review's problems and measured failures go to the scene editor
//! (revise::edit_scenes) as a change request: the scenes at fault are
//! rewritten, re-narrated and re-rendered, and the run is reviewed again.
//!
//! The new version is kept only if its review scores higher. Otherwise the
//! previous video, storyboard, narration and review are put back, so an
//! attempt can never make a video worse. Every attempt is logged to
//! `<run>/improve.json`, and the re-review still adds what it learned to the
//! lessons, so a failed attempt teaches the next video too.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::pipeline::{self, AppState};
use crate::review::{self, Review};
use crate::{lessons, revise};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Outcome {
    /// Score before any attempt.
    pub before: f64,
    /// Score of the version that was kept.
    pub after: f64,
    /// The score the video had to reach.
    pub target: f64,
    /// The best earlier video, if any: "<score> <title>".
    pub best_earlier: Option<String>,
    pub attempts: Vec<Attempt>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Attempt {
    pub scenes: Vec<usize>,
    pub summary: String,
    /// The re-review's score (None if the attempt failed before it).
    pub score: Option<f64>,
    pub kept: bool,
    #[serde(default)]
    pub error: Option<String>,
}

/// Outputs moved aside during an attempt, and small state copied, so a worse
/// attempt can be undone.
const MOVED: [&str; 7] = ["final.mp4", "master.mp4", "short.mp4", "short/master.mp4", "contact.png", "review.json", "final_telegram.mp4"];
const COPIED: [&str; 8] = [
    "storyboard.json",
    "timeline.json",
    "subtitles.ass",
    "short.ass",
    "youtube.txt",
    "result.json",
    "short/timeline.json",
    "short/short.json",
];

pub async fn run(state: &AppState, run_id: &str) -> AppResult<Outcome> {
    pipeline::validate_run_id(run_id)?;
    let cfg = &state.cfg;
    let dir = pipeline::run_dir(cfg, run_id);
    let mut current: Review = match tokio::fs::read(dir.join("review.json")).await {
        Ok(b) => serde_json::from_slice(&b)?,
        Err(_) => review::run(state, run_id).await?,
    };
    let best = lessons::best_other(cfg, run_id).await;
    let target = cfg.improve_target.max(best.as_ref().map_or(0.0, |b| b.score)).min(9.5);
    let mut out = Outcome {
        before: current.score,
        after: current.score,
        target,
        best_earlier: best.map(|b| format!("{:.1} {}", b.score, b.title)),
        attempts: Vec::new(),
    };

    for round in 1..=cfg.improve_rounds {
        if current.score >= target {
            break;
        }
        let Some(instruction) = instruction(&current, target) else { break };
        tracing::info!(round, score = current.score, target, "improving the video");
        let snap = dir.join("improve_prev");
        snapshot(&dir, &snap).await?;
        let mut attempt = Attempt::default();
        match revise::edit_scenes(state, run_id, &instruction).await {
            Ok(edit) if edit.scenes.is_empty() => {
                attempt.summary = format!("nothing to change in the scenes: {}", edit.summary.trim());
                restore(&dir, &snap).await?;
            }
            Ok(edit) => {
                attempt.scenes = edit.scenes;
                attempt.summary = edit.summary.trim().to_string();
                match review::run(state, run_id).await {
                    Ok(new) if new.score > current.score => {
                        attempt.score = Some(new.score);
                        attempt.kept = true;
                        current = new;
                        let _ = tokio::fs::remove_dir_all(&snap).await;
                    }
                    Ok(new) => {
                        attempt.score = Some(new.score);
                        restore(&dir, &snap).await?;
                        rescore(state, run_id, &current).await?;
                    }
                    Err(e) => {
                        attempt.error = Some(e.to_string());
                        restore(&dir, &snap).await?;
                        rescore(state, run_id, &current).await?;
                    }
                }
            }
            Err(e) => {
                attempt.error = Some(e.to_string());
                restore(&dir, &snap).await?;
            }
        }
        let kept = attempt.kept;
        out.attempts.push(attempt);
        if !kept {
            break; // the same instruction would most likely fail the same way
        }
    }
    out.after = current.score;
    tokio::fs::write(dir.join("improve.json"), serde_json::to_vec_pretty(&out)?).await?;
    Ok(out)
}

/// The change request for the scene editor, or None when the review names nothing to fix.
fn instruction(r: &Review, target: f64) -> Option<String> {
    let m = &r.metrics;
    let mut fixes: Vec<String> = r.problems.iter().take(6).cloned().collect();
    if !m.placeholder_parts.is_empty() {
        fixes.push(format!(
            "process parts the kit can't draw (they show as empty boxes): {}; use kit parts that exist instead",
            m.placeholder_parts.join(", ")
        ));
    }
    for (what, list) in [("failed to render and fell back to a plain card", &m.render_fallbacks),
                         ("are custom scenes whose code failed", &m.custom_failed),
                         ("failed to render in the 9:16 short", &m.short_fallbacks)] {
        if !list.is_empty() {
            let ids: Vec<String> = list.iter().map(|i| i.to_string()).collect();
            fixes.push(format!("scenes {} {what}; give them a simpler, reliable layout", ids.join(", ")));
        }
    }
    if fixes.is_empty() {
        return None;
    }
    Some(format!(
        "The self-review scored this video {:.1}/10; it has to reach {target:.1}. Fix these problems by rewriting \
         only the scenes at fault (better layout choice, fewer parts or labels, shorter labels, clearer steps, a \
         stronger wow). Keep everything else, including the hook, as it is. Change narration only where the \
         problem is in the words, and keep its length.\n- {}",
        r.score,
        fixes.join("\n- ")
    ))
}

/// Put the review that is being kept back on the scorecard.
async fn rescore(state: &AppState, run_id: &str, kept: &Review) -> AppResult<()> {
    let dir = pipeline::run_dir(&state.cfg, run_id);
    let title = tokio::fs::read(dir.join("storyboard.json"))
        .await
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v["title"].as_str().map(str::to_string))
        .unwrap_or_default();
    let worst = kept.problems.first().map(String::as_str).unwrap_or("");
    lessons::score(&state.cfg, run_id, &title, kept.score, worst).await
}

async fn snapshot(dir: &Path, snap: &Path) -> AppResult<()> {
    let _ = tokio::fs::remove_dir_all(snap).await;
    tokio::fs::create_dir_all(snap.join("short")).await?;
    for f in MOVED {
        let p = dir.join(f);
        if p.exists() {
            tokio::fs::rename(&p, snap.join(f)).await?;
        }
    }
    for f in COPIED {
        let p = dir.join(f);
        if p.exists() {
            tokio::fs::copy(&p, snap.join(f)).await?;
        }
    }
    // Narration is cached by text: keeping the old files avoids paying for it again after an undo.
    copy_dir(&dir.join("audio"), &snap.join("audio")).await
}

async fn restore(dir: &Path, snap: &Path) -> AppResult<()> {
    if !snap.exists() {
        return Err(AppError::NotFound(snap.display().to_string()));
    }
    for f in MOVED {
        let _ = tokio::fs::remove_file(dir.join(f)).await;
        if snap.join(f).exists() {
            tokio::fs::rename(snap.join(f), dir.join(f)).await?;
        }
    }
    for f in COPIED {
        if snap.join(f).exists() {
            tokio::fs::copy(snap.join(f), dir.join(f)).await?;
        }
    }
    let _ = tokio::fs::remove_dir_all(dir.join("audio")).await;
    copy_dir(&snap.join("audio"), &dir.join("audio")).await?;
    let _ = tokio::fs::remove_dir_all(snap).await;
    Ok(())
}

async fn copy_dir(from: &Path, to: &Path) -> AppResult<()> {
    tokio::fs::create_dir_all(to).await?;
    let mut rd = match tokio::fs::read_dir(from).await {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };
    while let Some(e) = rd.next_entry().await? {
        let p: PathBuf = e.path();
        if p.is_file() {
            tokio::fs::copy(&p, to.join(e.file_name())).await?;
        }
    }
    Ok(())
}

/// Text for the bot and the CLI.
pub fn summary(o: &Outcome) -> String {
    let mut s = match o.attempts.iter().find(|a| a.kept) {
        _ if o.attempts.is_empty() && o.before >= o.target => {
            format!("Score {:.1} already meets the target {:.1}; nothing to improve.", o.before, o.target)
        }
        _ if o.attempts.is_empty() => format!("Score {:.1} (target {:.1}); the review named nothing fixable.", o.before, o.target),
        Some(_) => format!("Improved {:.1} → {:.1} (target {:.1}).", o.before, o.after, o.target),
        None => format!("Tried to improve {:.1} (target {:.1}); no attempt scored higher, kept the original.", o.before, o.target),
    };
    if let Some(b) = &o.best_earlier {
        s.push_str(&format!(" Best earlier video: {b}."));
    }
    for a in &o.attempts {
        let what = match (&a.error, a.score) {
            (Some(e), _) => format!("failed: {}", e.lines().next().unwrap_or("")),
            (None, Some(sc)) => format!("{sc:.1}{}", if a.kept { ", kept" } else { ", undone" }),
            (None, None) => "no change".into(),
        };
        s.push_str(&format!("\n• scenes {:?}: {} ({what})", a.scenes, a.summary));
    }
    s
}
