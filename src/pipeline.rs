//! End-to-end orchestration: topic -> storyboard -> narration -> timeline ->
//! scene renders -> final composite.
//!
//! Everything for a run lives in `storage/runs/<run_id>/`:
//!
//!   request.json      what was asked for
//!   storyboard.json   the script + visual plan (editable; re-run with --resume)
//!   audio/            per-scene narration + word timings (cached by text)
//!   timeline.json     exact scene timings for the target duration
//!   scenes/           per-scene render specs, clips and fallbacks
//!   subtitles.ass     word-highlight captions
//!   final.mp4         the video
//!   result.json       summary
//!
//! Every stage reuses its outputs when inputs are unchanged, so a resumed run
//! only pays for what changed.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use futures::stream::{self, StreamExt, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::catalog::Catalog;
use crate::config::{Aspect, Config, MusicSource, Quality, TtsProvider, VisualStyle};
use crate::custom;
use crate::error::{AppError, AppResult};
use crate::llm::{self, StoryRequest};
use crate::media;
use crate::render::{self, SceneRender};
use crate::storyboard::{Budget, Storyboard};
use crate::subtitles::{self, CaptionStyle};
use crate::timeline::{self, FitError};
use crate::tts::{self, Word};

#[derive(Clone)]
pub struct AppState {
    pub client: Client,
    pub cfg: Arc<Config>,
    pub catalog: Arc<Catalog>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VideoRequest {
    /// What the video is about (ignored if `storyboard` is given).
    #[serde(default)]
    pub topic: Option<String>,
    /// A ready-made storyboard; skips the LLM.
    #[serde(default)]
    pub storyboard: Option<Storyboard>,
    #[serde(default)]
    pub voice_id: Option<String>,
    /// landscape | portrait
    #[serde(default)]
    pub aspect: Option<String>,
    /// draft | standard | high | ultra
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    /// Use silent narration with estimated timings (no TTS cost).
    #[serde(default)]
    pub silent: Option<bool>,
    /// Set false to disable background music for this video.
    #[serde(default)]
    pub music: Option<bool>,
    /// diagram | infographic
    #[serde(default)]
    pub style: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub run_id: String,
    pub title: String,
    pub video: PathBuf,
    pub run_dir: PathBuf,
    pub duration: f64,
    pub scenes: usize,
    pub tempo: f64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub elapsed_secs: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub stage: &'static str,
    pub progress: f32,
    pub message: String,
}

pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

fn report(p: &ProgressFn, stage: &'static str, progress: f32, message: impl Into<String>) {
    let message = message.into();
    tracing::info!(stage, progress = format!("{:.0}%", progress * 100.0), "{message}");
    p(Progress { stage, progress, message });
}

pub fn run_dir(cfg: &Config, run_id: &str) -> PathBuf {
    cfg.runs_dir().join(run_id)
}

pub fn validate_run_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::BadRequest("invalid run id".into()));
    }
    Ok(())
}

async fn write_json<T: Serialize>(path: &Path, v: &T) -> AppResult<()> {
    tokio::fs::write(path, serde_json::to_vec_pretty(v)?).await?;
    Ok(())
}

async fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let bytes = tokio::fs::read(path).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub async fn run(state: &AppState, run_id: &str, req: VideoRequest, progress: ProgressFn) -> AppResult<RunResult> {
    let started = Instant::now();
    let cfg = &state.cfg;
    validate_run_id(run_id)?;
    let aspect = req.aspect.as_deref().map(Aspect::parse).transpose()?.unwrap_or(cfg.aspect);
    let quality = req.quality.as_deref().map(Quality::parse).transpose()?.unwrap_or(cfg.quality);
    let silent = req.silent.unwrap_or(cfg.tts_provider == TtsProvider::Silent);
    let (short, long, fps) = quality.dims();
    let (width, height) = match aspect {
        Aspect::Landscape => (long, short),
        Aspect::Portrait => (short, long),
    };

    let dir = run_dir(cfg, run_id);
    for sub in ["audio", "scenes"] {
        tokio::fs::create_dir_all(dir.join(sub)).await?;
    }
    write_json(&dir.join("request.json"), &req).await?;

    // ---- 1. storyboard -------------------------------------------------
    let sb_path = dir.join("storyboard.json");
    let user_storyboard = req.storyboard.is_some();
    let mut board = obtain_storyboard(state, &req, &sb_path, aspect, None, &progress).await?;

    // ---- 2. narration + 3. timeline (with one automatic shorten retry) ---
    let voice = if silent {
        "silent".to_string()
    } else {
        tts::resolve_voice(&state.client, cfg, req.voice_id.as_deref()).await?
    };
    let mut shortened = false;
    let (timeline, voice_files) = loop {
        let (files, durations, words) = narrate(state, &board, &dir, &voice, silent, &progress).await?;
        match timeline::fit(&durations, &words, cfg.target_duration, cfg.transition_secs, cfg.max_tempo) {
            Ok(tl) => break (tl, files),
            Err(FitError::TooLong { needed_tempo, speech_secs }) if !user_storyboard && !shortened => {
                shortened = true;
                let budget = Budget::for_duration(cfg.target_duration);
                let max_words = ((board.total_words() as f64 / needed_tempo) * 0.95) as usize;
                let note = format!(
                    "IMPORTANT: a previous draft's narration took {speech_secs:.0}s when read aloud, too long for \
                     {:.0}s. Keep the total narration at or below {} words.",
                    cfg.target_duration,
                    max_words.min(budget.target_words)
                );
                report(&progress, "storyboard", 0.08, "narration ran long — asking the writer to tighten it");
                let _ = tokio::fs::remove_file(&sb_path).await;
                board = obtain_storyboard(state, &req, &sb_path, aspect, Some(&note), &progress).await?;
            }
            Err(e) => return Err(AppError::Timeline(e.to_string())),
        }
    };
    write_json(&dir.join("timeline.json"), &timeline).await?;
    report(
        &progress,
        "timeline",
        0.34,
        format!("fitted {} scenes to {:.1}s (speech tempo {:.2}x)", timeline.scenes.len(), timeline.total, timeline.tempo),
    );

    // ---- 4. music + captions ------------------------------------------
    let music = if req.music == Some(false) { None } else { obtain_music(state, &board, &dir).await };
    let theme = state.catalog.theme(&board.theme).clone();
    let subs_path = dir.join("subtitles.ass");
    let subs = if cfg.burn_subtitles {
        let ass = subtitles::build(
            &timeline,
            &CaptionStyle { font: &cfg.subtitle_font, aspect, highlight: &theme.accent },
        );
        tokio::fs::write(&subs_path, ass).await?;
        Some(subs_path)
    } else {
        None
    };

    // ---- 5. render scenes ----------------------------------------------
    let n = board.scenes.len();
    let seed_base: u64 = run_id.bytes().fold(7u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64)) % 10_000;
    let specs: Vec<serde_json::Value> = board
        .scenes
        .iter()
        .zip(&timeline.scenes)
        .enumerate()
        .map(|(i, (scene, t))| {
            json!({
                "version": 1,
                "index": i,
                "count": n,
                "duration": t.clip_len,
                "content_end": t.slot,
                "fps": fps,
                "width": width,
                "height": height,
                "theme": theme_json(&theme),
                "seed": seed_base + i as u64,
                "words": t.words,
                "scene": scene,
            })
        })
        .collect();

    // ---- 5a. one-off diagrams written by the LLM (validated, with fallbacks)
    let n_custom = specs.iter().filter(|s| s["scene"]["layout"] == "custom").count();
    if n_custom > 0 {
        report(&progress, "custom", 0.35, format!("writing {n_custom} custom diagram scene(s)"));
    }
    let custom_code = custom::prepare(state, &specs, &dir).await;
    let jobs: Vec<SceneRender> = specs
        .into_iter()
        .enumerate()
        .map(|(i, mut spec)| {
            if spec["scene"]["layout"] == "custom" {
                match custom_code.get(&i) {
                    Some(p) => spec["custom_code"] = json!(p.to_string_lossy()),
                    None => spec = render::fallback_spec(&spec),
                }
            }
            SceneRender {
                index: i,
                spec,
                spec_path: dir.join("scenes").join(format!("scene_{i:02}.json")),
                out: dir.join("scenes").join(format!("scene_{i:02}.mp4")),
            }
        })
        .collect();
    let clips: Vec<PathBuf> = jobs.iter().map(|j| j.out.clone()).collect();
    report(&progress, "render", 0.36, format!("rendering {n} scenes ({width}x{height} @ {fps}fps)"));
    let p2 = progress.clone();
    render::render_all(
        cfg,
        jobs,
        Arc::new(move |done, total| {
            let frac = 0.36 + 0.5 * done as f32 / total as f32;
            report(&p2, "render", frac, format!("rendered scene {done}/{total}"));
        }),
    )
    .await?;

    // ---- 6. compose ------------------------------------------------------
    report(&progress, "compose", 0.88, "compositing transitions, narration, music and captions");
    let transitions: Vec<String> = board.scenes.iter().map(|s| s.transition.clone()).collect();
    let final_path = dir.join("final.mp4");
    media::compose(
        cfg,
        media::ComposeInput {
            timeline: &timeline,
            clips: &clips,
            voices: &voice_files,
            transitions: &transitions,
            music: music.as_deref(),
            subtitles: subs.as_deref(),
            fps,
            out: &final_path,
        },
    )
    .await?;
    let duration = media::probe_duration(cfg, &final_path).await?;
    if (duration - cfg.target_duration).abs() > 2.0 / fps as f64 {
        tracing::warn!(duration, target = cfg.target_duration, "final duration differs from target");
    }

    let result = RunResult {
        run_id: run_id.to_string(),
        title: board.title.clone(),
        video: final_path,
        run_dir: dir.clone(),
        duration,
        scenes: n,
        tempo: timeline.tempo,
        width,
        height,
        fps,
        elapsed_secs: started.elapsed().as_secs_f64(),
    };
    write_json(&dir.join("result.json"), &result).await?;
    report(&progress, "done", 1.0, format!("done: {} ({duration:.2}s)", result.video.display()));
    Ok(result)
}

fn theme_json(t: &crate::catalog::Theme) -> serde_json::Value {
    json!({
        "name": t.name, "description": t.description, "bg": t.bg, "bg2": t.bg2, "ink": t.ink, "muted": t.muted,
        "accent": t.accent, "accent2": t.accent2, "accent3": t.accent3, "dark": t.dark,
        "font": t.font, "style": t.style
    })
}

async fn obtain_storyboard(
    state: &AppState,
    req: &VideoRequest,
    path: &Path,
    aspect: Aspect,
    extra_note: Option<&str>,
    progress: &ProgressFn,
) -> AppResult<Storyboard> {
    let cfg = &state.cfg;
    if let Some(sb) = read_json::<Storyboard>(path).await {
        report(progress, "storyboard", 0.1, format!("reusing storyboard '{}'", sb.title));
        return Ok(sb);
    }
    let budget = Budget::for_duration(cfg.target_duration);
    let board = if let Some(sb) = &req.storyboard {
        let mut sb = sb.clone();
        sb.normalize(&state.catalog);
        if let Some(t) = req.theme.as_deref().filter(|t| state.catalog.themes.contains_key(*t)) {
            sb.theme = t.to_string();
        }
        if sb.scenes.is_empty() {
            return Err(AppError::Storyboard("storyboard has no scenes".into()));
        }
        for p in sb.validate(&budget) {
            tracing::warn!("storyboard: {p}");
        }
        report(progress, "storyboard", 0.1, format!("using provided storyboard '{}'", sb.title));
        sb
    } else {
        let topic = req
            .topic
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| AppError::BadRequest("provide a topic or a storyboard".into()))?;
        if topic.chars().count() > 2000 {
            return Err(AppError::BadRequest("topic too long (max 2000 chars)".into()));
        }
        report(progress, "storyboard", 0.02, "writing storyboard");
        let audience = match (req.audience.as_deref(), extra_note) {
            (Some(a), Some(n)) => Some(format!("{a}\n{n}")),
            (None, Some(n)) => Some(n.to_string()),
            (a, None) => a.map(str::to_string),
        };
        let sb = llm::write_storyboard(
            &state.client,
            cfg,
            &state.catalog,
            &StoryRequest {
                topic,
                theme: req.theme.as_deref(),
                audience: audience.as_deref(),
                portrait: aspect == Aspect::Portrait,
                style: req.style.as_deref().map(VisualStyle::parse).transpose()?.unwrap_or(cfg.style),
            },
        )
        .await?;
        report(
            progress,
            "storyboard",
            0.1,
            format!("storyboard '{}': {} scenes, {} words", sb.title, sb.scenes.len(), sb.total_words()),
        );
        sb
    };
    write_json(path, &board).await?;
    Ok(board)
}

#[derive(Serialize, Deserialize)]
struct NarrationCache {
    text: String,
    voice: String,
    model: String,
    duration: f64,
    words: Vec<Word>,
}

async fn narrate(
    state: &AppState,
    board: &Storyboard,
    dir: &Path,
    voice: &str,
    silent: bool,
    progress: &ProgressFn,
) -> AppResult<(Vec<PathBuf>, Vec<f64>, Vec<Vec<Word>>)> {
    let cfg = &state.cfg;
    let n = board.scenes.len();
    report(progress, "narration", 0.12, format!("narrating {n} scenes{}", if silent { " (silent)" } else { "" }));
    let model = if silent { "silent".to_string() } else { cfg.elevenlabs_model.clone() };
    let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut results = stream::iter((0..n).map(|i| {
        let scene = &board.scenes[i];
        let prev = i.checked_sub(1).map(|p| board.scenes[p].narration.as_str());
        let next = board.scenes.get(i + 1).map(|s| s.narration.as_str());
        let model = model.clone();
        let done = done.clone();
        async move {
            let ext = if silent { "wav" } else { "mp3" };
            let audio = dir.join("audio").join(format!("scene_{i:02}.{ext}"));
            let meta = dir.join("audio").join(format!("scene_{i:02}.json"));
            if let Some(c) = read_json::<NarrationCache>(&meta).await
                && c.text == scene.narration && c.voice == voice && c.model == model && audio.exists() {
                    return Ok::<_, AppError>((i, audio, c.duration, c.words));
                }
            let (words, duration) = if silent {
                let (words, dur) = tts::estimate_words(&scene.narration);
                media::silent_audio(cfg, dur, &audio).await?;
                (words, dur)
            } else {
                let words =
                    tts::synthesize(&state.client, cfg, voice, &scene.narration, prev, next, &audio).await?;
                (words, media::probe_duration(cfg, &audio).await?)
            };
            write_json(&meta, &NarrationCache { text: scene.narration.clone(), voice: voice.to_string(), model, duration, words: words.clone() })
                .await?;
            let d = done.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            report(progress, "narration", 0.12 + 0.2 * d as f32 / n as f32, format!("narrated scene {d}/{n} ({duration:.1}s)"));
            Ok((i, audio, duration, words))
        }
    }))
    .buffer_unordered(cfg.tts_concurrency)
    .try_collect::<Vec<_>>()
    .await?;
    results.sort_by_key(|r| r.0);
    let total: f64 = results.iter().map(|r| r.2).sum();
    report(progress, "narration", 0.32, format!("narration ready: {total:.1}s of speech"));
    Ok((
        results.iter().map(|r| r.1.clone()).collect(),
        results.iter().map(|r| r.2).collect(),
        results.into_iter().map(|r| r.3).collect(),
    ))
}

async fn obtain_music(state: &AppState, board: &Storyboard, dir: &Path) -> Option<PathBuf> {
    let cfg = &state.cfg;
    match &cfg.music {
        MusicSource::None => None,
        MusicSource::File(p) => Some(p.clone()),
        MusicSource::ElevenLabs => {
            let out = dir.join("audio").join("music.mp3");
            if out.exists() {
                return Some(out);
            }
            let prompt = board.music.clone().unwrap_or_else(|| "warm, curious, modern documentary underscore".into());
            match tts::generate_music(&state.client, cfg, &prompt, cfg.target_duration, &out).await {
                Ok(()) => Some(out),
                Err(e) => {
                    tracing::warn!(error = %e, "music generation failed; continuing without music");
                    None
                }
            }
        }
    }
}
