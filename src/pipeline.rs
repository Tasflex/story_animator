//! End-to-end orchestration: topic -> storyboard -> narration -> timeline ->
//! scene renders -> master -> YouTube video + vertical short.
//!
//! Everything for a run lives in `storage/runs/<run_id>/`:
//!
//!   request.json      what was asked for
//!   storyboard.json   the script + visual plan (editable; re-run with --resume)
//!   audio/            per-scene narration + word timings (cached by text)
//!   timeline.json     exact scene timings for the target duration
//!   scenes/           per-scene render specs, clips and fallbacks
//!   master.mp4        the clean render (no captions) both outputs are cut from
//!   subtitles.ass     word-highlight captions for the long video
//!   final.mp4         the 16:9 YouTube video
//!   short.ass         captions + title for the short
//!   short.mp4         the 9:16 short (<= SHORT_MAX_SECS), cut from the master
//!   youtube.txt       title, description, chapter timestamps, short title
//!   result.json       summary
//!
//! Every stage reuses its outputs when inputs are unchanged, so a resumed run
//! only pays for what changed. Narration is the only stage that costs TTS
//! credits, so the script's length is checked with free estimated timings
//! before anything is sent to ElevenLabs, and silent scenes are never sent.

use std::fmt::Write as _;
use std::ops::Range;
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
use crate::subtitles::{self, CaptionStyle, ShortStyle};
use crate::timeline::{self, FitError, Timeline};
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
    /// Make the video even if the topic doesn't fit the channel niche.
    #[serde(default)]
    pub force: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub run_id: String,
    pub title: String,
    pub video: PathBuf,
    #[serde(default)]
    pub short: Option<PathBuf>,
    #[serde(default)]
    pub short_duration: Option<f64>,
    #[serde(default)]
    pub youtube: Option<PathBuf>,
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

/// How much to scale the word budget so a draft that doesn't fit will.
fn rescale_for(e: &FitError) -> f64 {
    match e {
        FitError::TooLong { needed_tempo, .. } => 0.95 / needed_tempo,
        FitError::TooShort { fill, .. } => (1.06 / fill.max(0.3)).min(1.6),
    }
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

    // ---- 1. storyboard, length-checked for free before any TTS ----------
    let sb_path = dir.join("storyboard.json");
    let user_storyboard = req.storyboard.is_some();
    let mut word_scale = 1.0;
    let mut board = obtain_storyboard(state, &req, &sb_path, aspect, word_scale, &progress).await?;
    if !user_storyboard {
        for _ in 0..2 {
            let (durations, words) = estimate_narration(&board);
            match timeline::fit(&durations, &words, &holds(&board), cfg.target_duration, cfg.transition_secs, cfg.max_tempo) {
                Ok(_) => break,
                Err(e) => {
                    word_scale *= rescale_for(&e);
                    report(&progress, "storyboard", 0.08, format!("script would not fit ({e}); rewriting at {word_scale:.2}x length"));
                    let _ = tokio::fs::remove_file(&sb_path).await;
                    board = obtain_storyboard(state, &req, &sb_path, aspect, word_scale, &progress).await?;
                }
            }
        }
    }

    // ---- 2. narration + 3. timeline (one rewrite if the real voice misses) --
    let voice = if silent {
        "silent".to_string()
    } else {
        tts::resolve_voice(&state.client, cfg, req.voice_id.as_deref()).await?
    };
    let mut rewritten = false;
    let (timeline, voice_files) = loop {
        let (files, durations, words) = narrate(state, &board, &dir, &voice, silent, &progress).await?;
        match timeline::fit(&durations, &words, &holds(&board), cfg.target_duration, cfg.transition_secs, cfg.max_tempo) {
            Ok(tl) => break (tl, files),
            Err(e) if !user_storyboard && !rewritten => {
                rewritten = true;
                word_scale *= rescale_for(&e);
                report(&progress, "storyboard", 0.3, format!("narration missed the target ({e}); rewriting"));
                let _ = tokio::fs::remove_file(&sb_path).await;
                board = obtain_storyboard(state, &req, &sb_path, aspect, word_scale, &progress).await?;
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
    let seed_base: u64 = run_id.bytes().fold(7u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64)) % 10_000;
    let music = if req.music == Some(false) { None } else { obtain_music(state, &board, &dir, seed_base).await };
    let theme = state.catalog.theme(&board.theme).clone();
    let subs_path = dir.join("subtitles.ass");
    if cfg.burn_subtitles {
        let ass = subtitles::build(
            &timeline,
            &CaptionStyle { font: &cfg.subtitle_font, aspect, highlight: &theme.accent },
        );
        tokio::fs::write(&subs_path, ass).await?;
    }

    // ---- 5. render scenes ----------------------------------------------
    let n = board.scenes.len();
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
            let frac = 0.36 + 0.46 * done as f32 / total as f32;
            report(&p2, "render", frac, format!("rendered scene {done}/{total}"));
        }),
    )
    .await?;

    // ---- 6. master, then the YouTube video ------------------------------
    report(&progress, "compose", 0.83, "compositing transitions, narration and music");
    let transitions: Vec<String> = board.scenes.iter().map(|s| s.transition.clone()).collect();
    let master = dir.join("master.mp4");
    media::compose(
        cfg,
        media::ComposeInput {
            timeline: &timeline,
            clips: &clips,
            voices: &voice_files,
            transitions: &transitions,
            music: music.as_deref(),
            fps,
            out: &master,
        },
    )
    .await?;
    let final_path = dir.join("final.mp4");
    if cfg.burn_subtitles {
        report(&progress, "compose", 0.9, "burning captions");
        media::burn_captions(cfg, &master, &subs_path, &final_path).await?;
    } else {
        tokio::fs::copy(&master, &final_path).await?;
    }
    let duration = media::probe_duration(cfg, &final_path).await?;
    if (duration - cfg.target_duration).abs() > 2.0 / fps as f64 {
        tracing::warn!(duration, target = cfg.target_duration, "final duration differs from target");
    }

    // ---- 7. the vertical short, cut from the same master -----------------
    let mut short_out = None;
    if cfg.shorts && aspect == Aspect::Landscape {
        match short_segment(&board, &timeline, cfg.short_max_secs) {
            Some((range, start, end)) => {
                report(&progress, "short", 0.94, format!("cutting a {:.1}s vertical short", end - start));
                let title = board.short.as_ref().map(|s| s.title.as_str()).filter(|t| !t.is_empty()).unwrap_or(&board.title);
                let ass = subtitles::build_short(&timeline, range, start, end, &ShortStyle {
                    font: &cfg.subtitle_font,
                    highlight: &theme.accent,
                    title,
                    cta: "Full video on the channel",
                    captions_y: media::SHORT_PANEL_TOP + media::SHORT_PANEL_H + 70,
                });
                let short_subs = dir.join("short.ass");
                tokio::fs::write(&short_subs, ass).await?;
                let out = dir.join("short.mp4");
                media::make_short(cfg, media::ShortInput {
                    master: &master,
                    start,
                    duration: end - start,
                    subtitles: &short_subs,
                    accent: &theme.accent,
                    out: &out,
                })
                .await?;
                short_out = Some((out, end - start));
            }
            None => tracing::info!("storyboard picks no segment for a short; skipping it"),
        }
    }
    let youtube = dir.join("youtube.txt");
    tokio::fs::write(&youtube, youtube_text(&board, &timeline)).await?;

    let result = RunResult {
        run_id: run_id.to_string(),
        title: board.title.clone(),
        video: final_path,
        short: short_out.as_ref().map(|s| s.0.clone()),
        short_duration: short_out.as_ref().map(|s| s.1),
        youtube: Some(youtube),
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

fn holds(board: &Storyboard) -> Vec<f64> {
    board.scenes.iter().map(|s| s.hold_secs()).collect()
}

/// Free narration estimate (no TTS): durations and word timings per scene.
fn estimate_narration(board: &Storyboard) -> (Vec<f64>, Vec<Vec<Word>>) {
    board
        .scenes
        .iter()
        .map(|s| if s.is_silent() { (0.0, Vec::new()) } else { let (w, d) = tts::estimate_words(&s.narration); (d, w) })
        .unzip()
}

/// The scenes and `[start, end)` seconds of the master that make the short:
/// the storyboard's pick, minus leading/trailing silent scenes, trimmed at
/// scene boundaries to fit `max` seconds (extended if very short).
pub fn short_segment(board: &Storyboard, tl: &Timeline, max: f64) -> Option<(Range<usize>, f64, f64)> {
    let pick = board.short.as_ref()?;
    let n = tl.scenes.len().min(board.scenes.len());
    if n == 0 || pick.from >= n {
        return None;
    }
    let (mut from, mut to) = (pick.from, pick.to.min(n - 1).max(pick.from));
    while from < to && board.scenes[from].is_silent() {
        from += 1;
    }
    // Skip the tail of the crossfade into the first scene; narration starts later.
    let start = tl.scenes[from].start + if from > 0 { 0.2 } else { 0.0 };
    let end = |t: usize| tl.scenes[t].start + tl.scenes[t].slot;
    while to > from && (end(to) - start > max || board.scenes[to].is_silent()) {
        to -= 1;
    }
    while to + 1 < n && end(to) - start < 40.0 && !board.scenes[to + 1].is_silent() && end(to + 1) - start <= max {
        to += 1;
    }
    Some((from..to + 1, start, end(to).min(start + max)))
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0).floor() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Title, description and chapter timestamps, ready to paste into YouTube.
fn youtube_text(board: &Storyboard, tl: &Timeline) -> String {
    let mut s = format!("{}\n\n", board.title);
    if let Some(d) = &board.description {
        let _ = writeln!(s, "{d}\n");
    }
    if board.chapters.len() >= 3 {
        s.push_str("Chapters\n");
        for (k, name) in board.chapters.iter().enumerate() {
            let first = board.scenes.iter().position(|sc| sc.chapter == Some(k));
            if let Some(i) = first {
                let t = if k == 0 { 0.0 } else { tl.scenes[i].start };
                let _ = writeln!(s, "{} {name}", mmss(t));
            }
        }
    }
    if let Some(sh) = board.short.as_ref().filter(|sh| !sh.title.is_empty()) {
        let _ = writeln!(s, "\nShort title: {}", sh.title);
    }
    s
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
    word_scale: f64,
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
        let sb = llm::write_storyboard(
            &state.client,
            cfg,
            &state.catalog,
            &StoryRequest {
                topic,
                theme: req.theme.as_deref(),
                audience: req.audience.as_deref(),
                portrait: aspect == Aspect::Portrait,
                style: req.style.as_deref().map(VisualStyle::parse).transpose()?.unwrap_or(cfg.style),
                word_scale,
                force: req.force.unwrap_or(false),
            },
        )
        .await?;
        report(
            progress,
            "storyboard",
            0.1,
            format!(
                "storyboard '{}': {} scenes ({} silent), {} words",
                sb.title,
                sb.scenes.len(),
                sb.scenes.iter().filter(|s| s.is_silent()).count(),
                sb.total_words()
            ),
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

/// Narrate every scene that has words (silent scenes cost nothing and get
/// `None`). Cached by text, voice and model, so re-runs only pay for changes.
async fn narrate(
    state: &AppState,
    board: &Storyboard,
    dir: &Path,
    voice: &str,
    silent: bool,
    progress: &ProgressFn,
) -> AppResult<(Vec<Option<PathBuf>>, Vec<f64>, Vec<Vec<Word>>)> {
    let cfg = &state.cfg;
    let n = board.scenes.len();
    let voiced: Vec<usize> = (0..n).filter(|&i| !board.scenes[i].is_silent()).collect();
    let chars: usize = voiced.iter().map(|&i| board.scenes[i].narration.chars().count()).sum();
    report(
        progress,
        "narration",
        0.12,
        format!(
            "narrating {} of {n} scenes (~{chars} characters){}",
            voiced.len(),
            if silent { " (silent)" } else { "" }
        ),
    );
    let model = if silent { "silent".to_string() } else { cfg.elevenlabs_model.clone() };
    let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let total = voiced.len().max(1);
    let mut results = stream::iter(voiced.clone().into_iter().map(|i| {
        let scene = &board.scenes[i];
        // Neighbouring narrated text keeps intonation flowing across scenes.
        let prev = board.scenes[..i].iter().rev().find(|s| !s.is_silent()).map(|s| s.narration.as_str());
        let next = board.scenes[i + 1..].iter().find(|s| !s.is_silent()).map(|s| s.narration.as_str());
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
            report(progress, "narration", 0.12 + 0.2 * d as f32 / total as f32, format!("narrated scene {d}/{total} ({duration:.1}s)"));
            Ok((i, audio, duration, words))
        }
    }))
    .buffer_unordered(cfg.tts_concurrency)
    .try_collect::<Vec<_>>()
    .await?;
    results.sort_by_key(|r| r.0);
    let mut files = vec![None; n];
    let mut durations = vec![0.0; n];
    let mut words = vec![Vec::new(); n];
    for (i, audio, d, w) in results {
        files[i] = Some(audio);
        durations[i] = d;
        words[i] = w;
    }
    let speech: f64 = durations.iter().sum();
    report(progress, "narration", 0.32, format!("narration ready: {speech:.1}s of speech"));
    Ok((files, durations, words))
}

async fn obtain_music(state: &AppState, board: &Storyboard, dir: &Path, seed: u64) -> Option<PathBuf> {
    let cfg = &state.cfg;
    match &cfg.music {
        MusicSource::None => None,
        MusicSource::File(p) => Some(p.clone()),
        MusicSource::Library(lib) => {
            let tracks = music_tracks(lib).await;
            if tracks.is_empty() {
                tracing::warn!(dir = %lib.display(), "music library is empty (run `story_animator music`); no music");
                return None;
            }
            let pick = tracks[seed as usize % tracks.len()].clone();
            tracing::info!(track = %pick.display(), "background music");
            Some(pick)
        }
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

/// Audio files in a music library folder, sorted for a stable pick.
pub async fn music_tracks(lib: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(lib).await {
        while let Ok(Some(e)) = rd.next_entry().await {
            let p = e.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
            if matches!(ext.as_str(), "mp3" | "wav" | "m4a" | "ogg" | "flac") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storyboard::{Scene, ShortPick};
    use crate::timeline::SceneTiming;

    fn scene(narration: &str) -> Scene {
        serde_json::from_value(json!({ "narration": narration })).unwrap()
    }

    fn timing(start: f64, slot: f64) -> SceneTiming {
        SceneTiming { start, slot, clip_len: slot + 0.5, voice_offset: 0.35, voice_len: slot - 1.0, words: vec![] }
    }

    #[test]
    fn short_segment_skips_cards_and_fits_the_limit() {
        // card, then 8 narrated scenes of 9s each
        let mut scenes = vec![scene("")];
        scenes.extend((0..8).map(|_| scene("some words here")));
        let mut tl_scenes = vec![timing(100.0, 3.2)];
        let mut t = 103.2;
        for _ in 0..8 {
            tl_scenes.push(timing(t, 9.0));
            t += 9.0;
        }
        let board = Storyboard {
            title: "T".into(),
            theme: "blueprint".into(),
            music: None,
            description: None,
            chapters: vec![],
            short: Some(ShortPick { from: 0, to: 8, title: "Hook".into() }),
            scenes,
        };
        let tl = Timeline { total: 300.0, tempo: 1.0, transition: 0.5, scenes: tl_scenes };
        let (range, start, end) = short_segment(&board, &tl, 59.0).unwrap();
        assert_eq!(range.start, 1, "starts after the silent chapter card");
        assert!(end - start <= 59.0 && end - start > 50.0, "{}", end - start);
        assert!((start - 103.4).abs() < 1e-9);
    }

    #[test]
    fn youtube_chapters_have_timestamps() {
        let mut a = scene("hello there");
        a.chapter = Some(0);
        let mut card = scene("");
        card.chapter = Some(1);
        let mut b = scene("more words");
        b.chapter = Some(1);
        let mut c = scene("end words");
        c.chapter = Some(2);
        let board = Storyboard {
            title: "How Bridges Stand".into(),
            theme: "blueprint".into(),
            music: None,
            description: Some("Desc.".into()),
            chapters: vec!["Cold open".into(), "Forces".into(), "Payoff".into()],
            short: None,
            scenes: vec![a, card, b, c],
        };
        let tl = Timeline {
            total: 100.0,
            tempo: 1.0,
            transition: 0.5,
            scenes: vec![timing(0.0, 25.0), timing(25.0, 3.0), timing(28.0, 40.0), timing(68.0, 32.0)],
        };
        let txt = youtube_text(&board, &tl);
        assert!(txt.contains("0:00 Cold open") && txt.contains("0:25 Forces") && txt.contains("1:08 Payoff"), "{txt}");
    }
}
