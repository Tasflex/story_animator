use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::{self, StreamExt, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(thiserror::Error, Debug)]
enum AppError {
    #[error("gemini: {0}")]
    Gemini(String),
    #[error("elevenlabs: {0}")]
    ElevenLabs(String),
    #[error("deepseek: {0}")]
    DeepSeek(String),
    #[error("render: {0}")]
    Render(String),
    #[error("ffmpeg: {0}")]
    Ffmpeg(String),
    #[error("invalid request: {0}")]
    BadRequest(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        match &self {
            AppError::BadRequest(msg) => tracing::warn!(error = %msg, "bad request"),
            _ => tracing::error!(error = ?self, "request failed"),
        }
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

type AppResult<T> = Result<T, AppError>;

// ---------------------------------------------------------------------------
// Quality — typed, so a bad flag cannot exist
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Quality {
    Low,
    Medium,
    High,
    FourK,
}

impl Quality {
    fn flag(self) -> &'static str {
        match self {
            Quality::Low => "-ql",
            Quality::Medium => "-qm",
            Quality::High => "-qh",
            Quality::FourK => "-qk",
        }
    }

    fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim() {
            "ql" => Ok(Quality::Low),
            "qm" => Ok(Quality::Medium),
            "qh" => Ok(Quality::High),
            "qk" => Ok(Quality::FourK),
            other => Err(format!(
                "MANIM_QUALITY must be one of ql/qm/qh/qk, got {other:?}. \
                 '-V' is not a valid Manim quality flag."
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

struct Config {
    gemini_api_key: String,
    gemini_model: String,
    elevenlabs_api_key: String,
    deepseek_api_key: String,
    manim_bin: String,
    ffmpeg_bin: String,
    ffprobe_bin: String,
    manim_quality: Quality,
    manim_resolution: String,
    manim_fps: u32,
    manim_disable_caching: bool,
    manim_timeout_secs: u64,
    max_total_duration_secs: f64,
    min_total_duration_secs: f64,
    elevenlabs_model: String,
    background_music: Option<PathBuf>,
    manim_max_retries: u32,
    tts_concurrency: usize,
    premium_examples: bool,
    log_json: bool,
    enable_antialiasing: bool,
    enable_typst: bool,
    burn_subtitles: bool,
    subtitle_font: String,
    subtitle_font_size: u32,
}

fn env_required(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("missing required env var: {name}"))
}

fn env_default(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_parse<T: std::str::FromStr>(name: &str, default: T) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(v) if v.is_empty() => Ok(default),
        Ok(v) => v
            .trim()
            .parse::<T>()
            .map_err(|e| format!("invalid {name}={v:?}: {e}")),
        Err(_) => Ok(default),
    }
}

fn env_bare_exe(name: &str, default: &str) -> Result<String, String> {
    let raw = env_default(name, default);
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Ok(default.to_string());
    }
    if trimmed.chars().any(|c| c.is_whitespace()) {
        return Err(format!(
            "{name} must be a bare executable path/name (no arguments), got {trimmed:?}. \
             Flags like '-V', '-qh', '--renderer=...' must be set via their own env vars."
        ));
    }
    Ok(trimmed)
}

impl Config {
    fn from_env() -> Result<Self, String> {
        dotenv::dotenv().ok();

        if let Ok(v) = env::var("MANIM_RENDERER") {
            let v = v.trim();
            if !v.is_empty() && v != "cairo" {
                return Err(format!(
                    "MANIM_RENDERER={v:?} is not supported. Remove it from .env \
                     (Cairo is the only supported renderer)."
                ));
            }
        }

        let manim_quality = Quality::parse(&env_default("MANIM_QUALITY", "qh"))?;

        let manim_resolution = env_default("MANIM_RESOLUTION", "1920,1080")
            .trim()
            .to_string();
        let parts: Vec<&str> = manim_resolution.split(',').collect();
        if parts.len() != 2
            || parts[0].trim().parse::<u32>().is_err()
            || parts[1].trim().parse::<u32>().is_err()
        {
            return Err(format!(
                "MANIM_RESOLUTION must be WIDTH,HEIGHT, got {manim_resolution:?}"
            ));
        }

        let max_total = env_parse("MAX_TOTAL_DURATION_SECS", 180.0_f64)?;
        let min_total = env_parse("MIN_TOTAL_DURATION_SECS", 12.0_f64)?;
        if max_total <= min_total {
            return Err(format!(
                "MAX_TOTAL_DURATION_SECS ({max_total}) must exceed \
                 MIN_TOTAL_DURATION_SECS ({min_total})"
            ));
        }

        Ok(Config {
            gemini_api_key: env_required("GEMINI_API_KEY")?,
            gemini_model: env_default("GEMINI_MODEL", "gemini-3.5-flash"),
            elevenlabs_api_key: env_required("ELEVENLABS_API_KEY")?,
            deepseek_api_key: env_required("DEEPSEEK_API_KEY")?,
            manim_bin: env_bare_exe("MANIM_BIN", "manim")?,
            ffmpeg_bin: env_bare_exe("FFMPEG_BIN", "ffmpeg")?,
            ffprobe_bin: env_bare_exe("FFPROBE_BIN", "ffprobe")?,
            manim_quality,
            manim_resolution,
            manim_fps: env_parse("MANIM_FPS", 60_u32)?,
            manim_disable_caching: env_default("MANIM_DISABLE_CACHING", "0").trim() == "1",
            manim_timeout_secs: env_parse("MANIM_TIMEOUT_SECS", 600_u64)?,
            max_total_duration_secs: max_total,
            min_total_duration_secs: min_total,
            elevenlabs_model: env_default("ELEVENLABS_MODEL", "eleven_turbo_v2_5"),
            background_music: env::var("BACKGROUND_MUSIC")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
            manim_max_retries: env_parse("MANIM_MAX_RETRIES", 2_u32)?,
            tts_concurrency: env_parse("TTS_CONCURRENCY", 4_usize)?,
            premium_examples: env_default("PREMIUM_EXAMPLES", "1").trim() == "1",
            log_json: env_default("LOG_JSON", "0").trim() == "1",
            enable_antialiasing: env_default("MANIM_ANTIALIASING", "1").trim() == "1",
            enable_typst: env_default("MANIM_TYPST", "1").trim() == "1",
            burn_subtitles: env_default("BURN_SUBTITLES", "1").trim() == "1",
            subtitle_font: env_default("SUBTITLE_FONT", "Arial").trim().to_string(),
            subtitle_font_size: env_parse("SUBTITLE_FONT_SIZE", 20_u32)?,
        })
    }
}

#[derive(Clone)]
struct AppState {
    client: Client,
    config: Arc<Config>,
}

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

const STORAGE: &str = "storage";

fn safe_storage_path(input: &str) -> AppResult<PathBuf> {
    let canonical = std::fs::canonicalize(input)
        .map_err(|e| AppError::BadRequest(format!("path does not exist: {input} ({e})")))?;
    let root = std::fs::canonicalize(STORAGE)
        .map_err(|e| AppError::BadRequest(format!("storage root missing: {e}")))?;
    if !canonical.starts_with(&root) {
        return Err(AppError::BadRequest(format!(
            "path escapes storage/: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn storage_join(sub: &str, name: &str) -> PathBuf {
    Path::new(STORAGE).join(sub).join(name)
}

fn validate_voice_id(id: &str) -> AppResult<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::BadRequest("invalid voice_id".into()));
    }
    Ok(())
}

fn validate_topic(topic: &str) -> AppResult<()> {
    let trimmed = topic.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("topic is empty".into()));
    }
    if trimmed.len() > 2000 {
        return Err(AppError::BadRequest("topic too long (max 2000 chars)".into()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Storyboard
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Scene {
    narration: String,
    visual: String,
    mood: String,
    #[serde(default)]
    palette: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Storyboard {
    title: String,
    style: String,
    scenes: Vec<Scene>,
}

fn strip_json_fence(s: &str) -> String {
    let t = s.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```JSON"))
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    let t = t.trim_start();
    let t = t.strip_suffix("```").unwrap_or(t);
    t.trim().to_string()
}

// ---------------------------------------------------------------------------
// Palette normalization — Gemini does not guarantee 4 hex colors
// ---------------------------------------------------------------------------

const FALLBACK_BG: &str = "#0B1026";
const FALLBACK_INK: &str = "#F4F1DE";
const FALLBACK_ACCENT: &str = "#FFB703";
const FALLBACK_WARN: &str = "#E63946";

fn is_hex_color(s: &str) -> bool {
    let s = s.trim();
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn luminance_hex(s: &str) -> f64 {
    let t = s.trim().trim_start_matches('#');
    if t.len() < 6 {
        return 0.0;
    }
    let r = u8::from_str_radix(&t[0..2], 16).unwrap_or(0) as f64;
    let g = u8::from_str_radix(&t[2..4], 16).unwrap_or(0) as f64;
    let b = u8::from_str_radix(&t[4..6], 16).unwrap_or(0) as f64;
    (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0
}

/// Return exactly 4 valid hex colors: [BG, INK, ACCENT, WARN].
/// Missing or invalid entries are replaced with fallbacks. Also forces BG
/// near-black or near-white so a mid-tone bg cannot slip through.
fn normalize_palette(raw: &[String]) -> [String; 4] {
    let mut out: [Option<String>; 4] = [None, None, None, None];
    for (i, c) in raw.iter().take(4).enumerate() {
        if is_hex_color(c) {
            out[i] = Some(c.trim().to_uppercase());
        }
    }
    let bg = out[0].clone().unwrap_or_else(|| FALLBACK_BG.to_string());
    let bg_lum = luminance_hex(&bg);
    let bg = if bg_lum < 0.15 || bg_lum > 0.85 {
        bg
    } else {
        FALLBACK_BG.to_string()
    };
    let ink = out[1].clone().unwrap_or_else(|| FALLBACK_INK.to_string());
    let accent = out[2].clone().unwrap_or_else(|| FALLBACK_ACCENT.to_string());
    let warn = out[3].clone().unwrap_or_else(|| FALLBACK_WARN.to_string());
    [bg, ink, accent, warn]
}

fn pick_background(board: &Storyboard) -> String {
    let raw = board
        .scenes
        .first()
        .map(|s| s.palette.clone())
        .unwrap_or_default();
    normalize_palette(&raw)[0].clone()
}

// ---------------------------------------------------------------------------
// Subtitles — phrase-weighted chunking + styled ASS burn-in
// ---------------------------------------------------------------------------

const MAX_CUE_CHARS: usize = 42;
const MAX_CUE_LINES: usize = 2;
const MIN_CUE_SECS: f64 = 0.9;

fn chunk_scene_into_cues(text: &str, dur: f64) -> Vec<(f64, f64, String)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || dur <= 0.0 {
        return vec![];
    }

    let weights: Vec<f64> = words
        .iter()
        .map(|w| {
            let base = w.chars().count().max(1) as f64;
            let bonus = if w.ends_with(['.', '!', '?']) {
                3.0
            } else if w.ends_with([',', ';', ':']) {
                1.5
            } else {
                0.0
            };
            base + bonus
        })
        .collect();
    let total_w: f64 = weights.iter().sum();
    let secs_per_w = dur / total_w;

    let target_chars = MAX_CUE_CHARS * MAX_CUE_LINES;
    let mut cues: Vec<(f64, f64, String)> = Vec::new();
    let mut i = 0;
    let mut cursor_t = 0.0_f64;

    while i < words.len() {
        let start_t = cursor_t;
        let mut chars = 0usize;
        let mut j = i;
        let mut cue_w = 0.0_f64;
        while j < words.len() {
            let w_len = words[j].chars().count() + 1;
            if chars + w_len > target_chars && j > i {
                break;
            }
            chars += w_len;
            cue_w += weights[j];
            j += 1;
            if chars >= target_chars / 2 && words[j - 1].ends_with(['.', '!', '?']) {
                break;
            }
        }

        let end_t = start_t + cue_w * secs_per_w;
        let mut cue = words[i..j].join(" ");
        if cue.chars().count() > MAX_CUE_CHARS {
            if let Some(bp) = cue
                .char_indices()
                .take_while(|(idx, _)| *idx < MAX_CUE_CHARS)
                .filter(|(_, c)| *c == ' ')
                .map(|(idx, _)| idx)
                .last()
            {
                cue = format!("{}\n{}", &cue[..bp], cue[bp + 1..].trim());
            }
        }

        cues.push((start_t, end_t.max(start_t + MIN_CUE_SECS), cue));
        cursor_t = end_t;
        i = j;
    }

    if let Some(last) = cues.last_mut() {
        last.1 = last.1.min(dur).max(last.0 + 0.3);
    }
    cues
}

fn build_ass(board: &Storyboard, durations: &[f64], font: &str, font_size: u32) -> String {
    let mut out = String::new();
    out.push_str("[Script Info]\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\nWrapStyle: 2\nScaledBorderAndShadow: yes\n\n");
    out.push_str("[V4+ Styles]\n");
    out.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    out.push_str(&format!(
        "Style: Caption,{font},{font_size},&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,3,1.5,2,120,120,80,1\n\n"
    ));
    out.push_str("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");

    let ass_ts = |s: f64| {
        let cs = (s * 100.0).round() as u64;
        format!(
            "{}:{:02}:{:02}.{:02}",
            cs / 360_000,
            (cs / 6_000) % 60,
            (cs / 100) % 60,
            cs % 100
        )
    };

    let mut t0 = 0.0_f64;
    for (i, scene) in board.scenes.iter().enumerate() {
        let dur = durations.get(i).copied().unwrap_or(0.0);
        for (s, e, text) in chunk_scene_into_cues(scene.narration.trim(), dur) {
            let body = text.replace('\n', "\\N");
            let _ = writeln!(
                out,
                "Dialogue: 0,{},{},Caption,,0,0,0,,{{\\fad(150,150)}}{}",
                ass_ts(t0 + s),
                ass_ts(t0 + e),
                body
            );
        }
        t0 += dur;
    }
    out
}

// ---------------------------------------------------------------------------
// Gemini
// ---------------------------------------------------------------------------

async fn svc_generate_storyboard(state: &AppState, topic: &str) -> AppResult<Storyboard> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        state.config.gemini_model
    );

    let prompt = format!(
        r##"You are a senior creative director for a premium educational video channel
in the style of 3Blue1Brown, Veritasium, and Kurzgesagt.

Produce a storyboard about: {topic}

Return ONLY valid JSON matching this schema:
{{
  "title": "short punchy title",
  "style": "one of: minimalist-geometric, cinematic-noir, synthwave, editorial-clean, cosmic",
  "scenes": [
    {{
      "narration": "1-3 spoken sentences, TTS-ready, no stage directions, no emojis",
      "visual": "a SHOT LIST the animator can literally build. Name concrete manim objects and motion: 'white Dot at ORIGIN -> Create -> ReplacementTransform to Arrow -> camera push 0.7x on arrow tip -> Write label next_to arrow end'. Every scene must name at least one Transform/ReplacementTransform and one camera or highlight move.",
      "mood": "one word",
      "palette": ["#RRGGBB", "#RRGGBB", "#RRGGBB", "#RRGGBB"]
    }}
  ]
}}

Rules:
- 5 to 8 scenes. Each scene is 4-10 seconds of narration.
- EVERY scene MUST have a palette array of EXACTLY FOUR hex colors
  ["#RRGGBB", "#RRGGBB", "#RRGGBB", "#RRGGBB"] — no more, no less.
- The FIRST palette color of scene 1 becomes the video background. It MUST be near-black
  (#0B1026, #111116, #14121A) OR near-white (#FAFAFA, #F4F1DE). Never mid-tone blue/green/red.
- The SECOND palette color is the primary ink (text + main shape) and must contrast the bg.
- The THIRD is the accent (arrows, highlights). The FOURTH is a warning/secondary accent.
- Reuse 2-3 colors across the whole video.
- Ideas build: hook -> setup -> turn -> insight -> payoff.
- VARY compositions: never repeat the same layout twice in a row.
- No LaTeX, no math notation in narration - speak it plainly.
- The `visual` field MUST be a shot list, not a mood.
"##,
    );

    let body = json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "temperature": 0.9
        }
    });

    let response = state
        .client
        .post(&url)
        .header("x-goog-api-key", &state.config.gemini_api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let s = response.status();
        let t = response.text().await.unwrap_or_default();
        return Err(AppError::Gemini(format!("HTTP {s}: {t}")));
    }

    let data: Value = response.json().await?;
    let raw = data["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| AppError::Gemini("no text in storyboard response".into()))?;

    let cleaned = strip_json_fence(raw);
    let board: Storyboard = serde_json::from_str(&cleaned)
        .map_err(|e| AppError::Gemini(format!("bad storyboard JSON: {e}\n---\n{cleaned}")))?;

    if board.scenes.is_empty() {
        return Err(AppError::Gemini("storyboard has no scenes".into()));
    }
    Ok(board)
}

// ---------------------------------------------------------------------------
// ElevenLabs
// ---------------------------------------------------------------------------

async fn svc_generate_audio(state: &AppState, script: &str, voice_id: &str) -> AppResult<String> {
    let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{voice_id}");

    let body = json!({
        "text": script,
        "model_id": state.config.elevenlabs_model,
        "voice_settings": {
            "stability": 0.35,
            "similarity_boost": 0.8,
            "style": 0.55,
            "use_speaker_boost": true
        },
        "output_format": "mp3_44100_192"
    });

    let response = state
        .client
        .post(&url)
        .header("xi-api-key", &state.config.elevenlabs_api_key)
        .header("Content-Type", "application/json")
        .header("Accept", "audio/mpeg")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let s = response.status();
        let t = response.text().await.unwrap_or_default();
        return Err(AppError::ElevenLabs(format!("HTTP {s}: {t}")));
    }

    let ct = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !ct.starts_with("audio/") {
        let t = response.text().await.unwrap_or_default();
        return Err(AppError::ElevenLabs(format!("unexpected content-type '{ct}': {t}")));
    }

    let bytes = response.bytes().await?;
    if bytes.is_empty() {
        return Err(AppError::ElevenLabs("empty audio body".into()));
    }

    let rel_path = storage_join("audio", &format!("{}.mp3", uuid::Uuid::new_v4()));
    tokio::fs::write(&rel_path, &bytes).await?;
    Ok(rel_path.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Premium Manim prompt — with opacity layering and salience guidance
// ---------------------------------------------------------------------------

const PREMIUM_EXAMPLES: &str = r##"
# =========================================================================
# REFERENCE: a "premium" Manim scene. Pattern-match the idioms below.
# Camera animation is ALWAYS self.camera.frame.*, never self.frame.*
# Layout uses alignment helpers, never magic LEFT*x + UP*y coordinates.
# =========================================================================
from manim import *

class PremiumReference(MovingCameraScene):
    def construct(self):
        BG, INK, ACCENT, WARN = "#0B1026", "#F4F1DE", "#FFB703", "#E63946"
        self.camera.background_color = BG

        # --- TITLE region (top, y = +3.2, small) ---
        title = Text("The Idea", color=INK, font_size=40).to_edge(UP, buff=0.6)
        underline = Line(title.get_left(), title.get_right(), color=ACCENT, stroke_width=2)
        underline.next_to(title, DOWN, buff=0.15)
        self.play(FadeIn(title, shift=UP*0.2), Create(underline))

        # --- STAGE region (ORIGIN, max radius 2.5) ---
        node = Circle(radius=0.9, color=ACCENT, stroke_width=4).move_to(ORIGIN)
        label = Text("core", color=INK).move_to(node.get_center())
        self.play(GrowFromCenter(node), FadeIn(label))

        # Satellites arranged symmetrically — NEVER hand-place.
        labels = VGroup(*[Text(t, color=INK, font_size=28) for t in ["a", "b", "c"]])
        labels.arrange(RIGHT, buff=1.2).next_to(node, DOWN, buff=1.4)
        arrows = VGroup(*[
            Arrow(node.get_bottom(), l.get_top(), color=WARN, buff=0.1, stroke_width=3)
            for l in labels
        ])
        self.play(LaggedStart(*[GrowArrow(a) for a in arrows], lag_ratio=0.15))
        self.play(LaggedStart(*[FadeIn(l, shift=UP*0.1) for l in labels], lag_ratio=0.15))

        # Camera push on the node — Cairo / MovingCameraScene idiom.
        self.camera.frame.save_state()
        self.play(self.camera.frame.animate.scale(0.55).move_to(node.get_center()), run_time=1.2)
        self.play(Indicate(node, color=WARN, scale_factor=1.15), run_time=1.0)
        self.play(self.camera.frame.animate.restore(), run_time=1.0)

        # Closing composition: pull back, keep the concept centered.
        self.play(self.camera.frame.animate.scale(1.25), run_time=1.4)
"##;

fn build_manim_prompt(
    board: &Storyboard,
    scene_durations: &[f64],
    previous_error: Option<&str>,
    premium: bool,
    typst: bool,
) -> AppResult<String> {
    let total: f64 = scene_durations.iter().sum();
    let target = total.max(1.0);

    let scenes_json = serde_json::to_string_pretty(&board.scenes)
        .map_err(|e| AppError::DeepSeek(format!("serialize storyboard: {e}")))?;
    let durations_json = serde_json::to_string(scene_durations).unwrap_or_default();

    let raw_palette = board
        .scenes
        .first()
        .map(|s| s.palette.clone())
        .unwrap_or_default();
    let palette = normalize_palette(&raw_palette);
    let (bg_c, ink_c, accent_c, warn_c) = (&palette[0], &palette[1], &palette[2], &palette[3]);

    let retry_note = match previous_error {
        Some(err) => format!(
            "\n\nIMPORTANT: A previous attempt failed with this Manim error. Fix it:\n---\n{}\n---\n",
            err.chars().take(3000).collect::<String>()
        ),
        None => String::new(),
    };

    let premium_block = if premium { PREMIUM_EXAMPLES } else { "" };

    let typst_rule = if typst {
        r#"4c. For MATH SYMBOLS OR FORMULAS, use `Typst(r"$...$")` instead of Text.
    Do NOT use MathTex, Tex, or any LaTeX object.
"#
    } else {
        ""
    };

    Ok(format!(
        r##"You are a world-class Manim animator (3Blue1Brown tier).
Write ONE Python file that renders a cinematic explanation in the style "{style}".

{premium_block}

Hard requirements:

1. Define exactly one scene class: `class StoryAnimation(MovingCameraScene):`
2. Each scene i has target duration T_i from the durations array. Wrap every scene in:
              self.next_section(f"scene_{{i}}")
       _t0 = self.renderer.time
       ... your animations for scene i ...
       _rem = T_i - (self.renderer.time - _t0)
       if _rem > 0.05:
           self.wait(_rem)
   If _rem <= 0.05 the animations already filled the slot — do NOT call wait().
   `self.wait(0)` raises "duration of 0 <= 0 seconds", so the `if _rem > 0.05` guard
   is MANDATORY. Never write `self.wait(max(0.0, ...))`.
3. Colors: the FIRST line inside construct MUST be exactly:
       BG, INK, ACCENT, WARN = "{bg_c}", "{ink_c}", "{accent_c}", "{warn_c}"
   Do not change these values. Do not add or remove names. Four values, four names.
   The NEXT line MUST be: `self.camera.background_color = BG`.
4. NO Tex, MathTex, or any LaTeX. Allowed mobjects: Text, VGroup, Circle, Square,
   Rectangle, Line, Arrow, Dot, Polygon, RegularPolygon, RoundedRectangle, Arc,
   Ellipse, Triangle, Annulus, SurroundingRectangle, Brace, NumberLine, Axes,
   DecimalNumber, ArcBetweenPoints, DashedLine, ValueTracker, Typst,
   Graph, Matrix, ComplexPlane, ParametricFunction, VectorField, StreamLines,
   BarChart, Table, Dodecahedron.
   You are NOT limited to primitives — use the full Manim toolkit where it serves the explanation.
4b. NEVER call `brace.get_text(...)` or `brace.get_tex(...)`. Use
    `label = Text("x").next_to(brace, DOWN, buff=0.1)` and `FadeIn(label)`.
{typst_rule}
5. TECHNIQUE DENSITY CONTRACT — prefer, where natural:
   (a) >=3 Transform or ReplacementTransform chains,
   (b) >=1 of MoveAlongPath / ValueTracker+always_redraw / Axes.plot,
   (c) >=1 camera save_state + scale/move_to + restore using `self.camera.frame`,
   (d) >=1 SurroundingRectangle + Indicate pairing,
   (e) >=1 Brace / NumberLine / Axes,
   (f) >=1 rate_func=there_and_back or overshoot.
   These are signals of craft. They are not mandatory — deviate when a different
   technique serves the explanation better.
6. `self.wait(...)` is only valid in TWO places:
   (a) the guarded trailing padding wait in rule 2, and
   (b) NOWHERE ELSE inside a scene. Between `self.play(...)` calls, animate instead.
   Never write self.wait(0) or self.wait(max(0.0, ...)).
7. Group text into VGroups with arrange(DOWN, buff=0.3); animate the group.
8. LAST frame must be composed (transform into closing title, or pull back),
   not FadeOut+Write.
9. Imports: `from manim import *`
10. Return ONLY one ```python fenced block.

CAMERA API — MANDATORY:

11. The renderer is Cairo. The ONLY valid camera animation surface is
    `self.camera.frame`. Use it exactly like this:
        self.camera.frame.save_state()
        self.play(self.camera.frame.animate.scale(0.6).move_to(focus.get_center()))
        self.play(self.camera.frame.animate.restore())
    To zoom in: scale(<1). To zoom out: scale(>1). To pan: `.move_to(point)`.
    Every `save_state()` MUST have a matching `restore()` (or `Restore(self.camera.frame)`)
    within the same scene. Do not leave the camera zoomed at the end of a scene.
12. NEVER reference `self.frame` — that attribute does NOT exist on a Scene
    and will crash the render with:
        AttributeError: 'StoryAnimation' object has no attribute 'frame'
    If you typed `self.frame.` anywhere, rewrite it as `self.camera.frame.`.

MOBJECT RULES:

13. Line/Arrow/DashedLine/Vector/ArcBetweenPoints take EXACTLY TWO POSITIONAL args.
    WRONG: Line(LEFT*3, UP*2, RIGHT*2, DOWN)
    RIGHT: Line(LEFT*3 + UP*2, RIGHT*2 + DOWN)
14. Circle/Square/Rectangle/Dot/Triangle/Polygon take ONE positional arg or none.
15. Text("...") takes a string; position with .move_to(RIGHT*x + UP*y).

VISUAL SALIENCE — GUIDANCE, NOT LAW:

16. Assign every mobject an implicit salience tier:
    - Primary focus (what the narration is about): full opacity, accent color or bright ink.
    - Contextual support (labels, secondary shapes, connecting lines): reduced opacity
      via `.set_opacity(0.4)` or a desaturated color.
    - Structural scaffolding (axes, grids, bounding boxes, reference lines):
      `.set_opacity(0.15)` so they recede.
    A viewer's eye should have one obvious target at every moment. If you introduce
    a second element of equal visual weight, differentiate them (size, color, motion)
    or stage them sequentially. This is a default, not a prohibition — complex
    multi-element compositions are fine when the explanation calls for them.

CAPTIONS:

17. DO NOT draw the narration text on screen. Captions are burned in by ffmpeg.
    Text mobjects are for labels, symbols, and short titles ONLY (<= 4 words).
    The bottom-center strip (y < -2.5) must be left EMPTY so captions are readable.

LAYOUT & ALIGNMENT — MANDATORY:

18. Build every scene on a 16:9 safe area of width 12, height 7, centered at ORIGIN.
    Never place a mobject whose bounding box exceeds x ∈ [-7, 7] or y ∈ [-3.8, 3.8].
19. Reserve fixed regions and REUSE them across scenes:
      TITLE  : top,    y = +3.2, x = 0, scale <= 0.6
      STAGE  : center, ORIGIN, radius <= 2.5
      CAPTION: bottom, y = -3.0, x = 0  (leave empty — ffmpeg burns captions there)
    NEVER place a mobject with center y < -2.5.
20. Align with helpers, never with magic numbers:
      label.next_to(obj, UP, buff=0.25)
      VGroup(a, b, c).arrange(RIGHT, buff=0.4).move_to(ORIGIN)
      group.arrange(DOWN, aligned_edge=LEFT, buff=0.3)
21. Every `self.play` must have a reason: transform the PREVIOUS mobject into the
    new one. Never FadeOut then FadeIn the same concept — use ReplacementTransform.
22. Timing: every `self.play(...)` call gets `run_time=` between 0.6 and 2.0.
    Vary it — do not default to 1.0 for everything.
23. The final 2 seconds show ONE composed closing frame: pull the camera back
    (`self.camera.frame.animate.scale(1.4)`), keep the main concept centered,
    add the video title as a small Text at TITLE region.

{retry_note}

Style: {style}
Title: {title}
Palette — BG={bg_c}  INK={ink_c}  ACCENT={accent_c}  WARN={warn_c}

Scenes:
{scenes_json}

Durations (seconds): {durations_json}
Total target: {target:.2}s

Produce the file now."##,
        style = board.style,
        title = board.title,
        bg_c = bg_c,
        ink_c = ink_c,
        accent_c = accent_c,
        warn_c = warn_c,
        scenes_json = scenes_json,
        durations_json = durations_json,
        target = target,
        retry_note = retry_note,
        premium_block = premium_block,
        typst_rule = typst_rule,
    ))
}

// ---------------------------------------------------------------------------
// Code extraction / sanitization
// ---------------------------------------------------------------------------

fn extract_python(content: &str) -> Option<String> {
    for fence in ["```python", "```py", "```"] {
        if let Some(start) = content.find(fence) {
            let after = &content[start + fence.len()..];
            let after = after.strip_prefix('\n').unwrap_or(after);
            if let Some(end) = after.find("```") {
                let code = after[..end].trim();
                if !code.is_empty() {
                    return Some(code.to_string());
                }
            }
        }
    }
    None
}

fn sanitize_manim_code(code: String) -> String {
    use regex::Regex;
    let re_mathtex = Regex::new(r"\bMathTex\b").unwrap();
    let re_tex = Regex::new(r"\bTex\b").unwrap();
    let re_scene_base = Regex::new(r"class\s+StoryAnimation\s*\(\s*Scene\s*\)").unwrap();

    let code = re_mathtex.replace_all(&code, "Text").into_owned();
    let code = re_tex.replace_all(&code, "Text").into_owned();
    re_scene_base
        .replace_all(&code, "class StoryAnimation(MovingCameraScene)")
        .into_owned()
}

fn rewrite_brace_get_text(code: String) -> String {
    use regex::Regex;
    let re = Regex::new(
        r#"(?P<brace>[A-Za-z_][A-Za-z0-9_]*)\s*\.\s*get_(?:text|tex)\s*\(\s*"(?P<label>[^"\\]*)"\s*(?:,\s*(?P<kw>(?:color|font_size)\s*=\s*(?:"[^"]*"|[A-Za-z_#][A-Za-z0-9_"]*)))?\s*\)"#,
    )
    .unwrap();

    re.replace_all(&code, |caps: &regex::Captures| {
        let brace = &caps["brace"];
        let label = &caps["label"];
        let kw = caps.name("kw").map(|m| m.as_str()).unwrap_or("");
        if kw.is_empty() {
            format!(r#"Text("{label}").next_to({brace}, DOWN, buff=0.1)"#)
        } else {
            format!(r#"Text("{label}", {kw}).next_to({brace}, DOWN, buff=0.1)"#)
        }
    })
    .into_owned()
}

fn force_camera_frame_idiom(code: String) -> String {
    use regex::Regex;
    let re = Regex::new(r"\bself\.frame\b").unwrap();
    re.replace_all(&code, "self.camera.frame").into_owned()
}

fn guard_zero_waits(code: String) -> String {
    use regex::Regex;

    let re_max = Regex::new(r"self\.wait\(\s*max\(\s*0\.0\s*,\s*(?P<expr>[^()]*(?:\([^()]*\)[^()]*)*)\)\s*\)").unwrap();
    let code = re_max
        .replace_all(&code, |caps: &regex::Captures| {
            let expr = caps["expr"].trim();
            format!("_w = float({expr})\n        if _w > 0.05:\n            self.wait(_w)")
        })
        .into_owned();

    let re_zero = Regex::new(r"(?m)^[ \t]*self\.wait\(\s*0(?:\.0+)?\s*\)[ \t]*(?:#.*)?\n").unwrap();
    re_zero.replace_all(&code, "").into_owned()
}

fn ensure_background(mut code: String, bg: &str) -> String {
    if code.contains("background_color") {
        return code;
    }
    let needle = "def construct(self):";
    if let Some(pos) = code.find(needle) {
        let line_start = code[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let indent: String = code[line_start..pos]
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        let body_indent = format!("{indent}    ");
        let inject = format!("\n{body_indent}self.camera.background_color = \"{bg}\"");
        code.insert_str(pos + needle.len(), &inject);
    }
    code
}

fn is_moving_camera_scene(code: &str) -> bool {
    let re = regex::Regex::new(r"class\s+StoryAnimation\s*\(\s*MovingCameraScene\s*\)").unwrap();
    re.is_match(code)
}

fn clean_manim_error(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        let t = line.trim_start_matches(['|', '+', '-']).trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("Traceback (most recent call last)") {
            continue;
        }
        if line.trim_start().starts_with("C:\\") || line.contains("site-packages\\manim") {
            continue;
        }
        if t.starts_with('^') || t.starts_with('~') {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if let Some(pos) = out.rfind("Error:") {
        let tail: String = out[pos..].lines().next().unwrap_or("").to_string();
        out = format!("{tail}\n\n---\n{out}");
    }
    out.chars().take(2500).collect()
}

async fn svc_request_manim_code(
    state: &AppState,
    prompt: &str,
    bg: &str,
) -> AppResult<String> {
    let body = json!({
        "model": "deepseek-chat",
        "messages": [
            {"role": "system", "content": "You are a world-class Manim developer. Output only valid, runnable Python. Never use LaTeX. Camera animation MUST use self.camera.frame (Cairo/MovingCameraScene). NEVER write self.frame. Line/Arrow/DashedLine/Vector/ArcBetweenPoints take exactly two positional arguments. Never call brace.get_text or brace.get_tex — use Text(\"...\").next_to(brace, ...). Never write self.wait(0) or self.wait(max(0.0, ...)). Align with .next_to/.arrange/.to_edge — never hand-place with LEFT*x+UP*y. Never draw caption text; leave the bottom strip empty. Return only one ```python fenced block."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.6
    });

    let response = state
        .client
        .post("https://api.deepseek.com/chat/completions")
        .header("Authorization", format!("Bearer {}", state.config.deepseek_api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let s = response.status();
        let t = response.text().await.unwrap_or_default();
        return Err(AppError::DeepSeek(format!("HTTP {s}: {t}")));
    }

    let data: Value = response.json().await?;
    let content = data["choices"][0]["message"]["content"].as_str().unwrap_or("");

    let code = extract_python(content)
        .ok_or_else(|| AppError::DeepSeek("no python code block found".into()))?;

    let code = sanitize_manim_code(code);
    let code = rewrite_brace_get_text(code);
    let code = force_camera_frame_idiom(code);
    let code = guard_zero_waits(code);
    let code = ensure_background(code, bg);

    if !is_moving_camera_scene(&code) {
        return Err(AppError::DeepSeek(
            "generated code must define `class StoryAnimation(MovingCameraScene):`".into(),
        ));
    }

    Ok(code)
}

// ---------------------------------------------------------------------------
// Render — Cairo only
// ---------------------------------------------------------------------------

async fn svc_generate_manim_with_render(
    state: &AppState,
    board: &Storyboard,
    scene_durations: &[f64],
    run_id: &str,
) -> AppResult<String> {
    let mut last_err: Option<String> = None;
    let attempts = state.config.manim_max_retries.saturating_add(1);
    let bg = pick_background(board);

    for attempt in 0..attempts {
        let prompt = build_manim_prompt(
            board,
            scene_durations,
            last_err.as_deref(),
            state.config.premium_examples,
            state.config.enable_typst,
        )?;
        let code = svc_request_manim_code(state, &prompt, &bg).await?;

        let rel_path = storage_join("scripts", &format!("{}.py", uuid::Uuid::new_v4()));
        tokio::fs::write(&rel_path, code.as_bytes()).await?;

        tracing::info!(
            run_id = %run_id, attempt = attempt + 1, path = %rel_path.display(),
            premium = state.config.premium_examples,
            typst = state.config.enable_typst,
            "manim code written"
        );

        match svc_render(state, &rel_path.to_string_lossy(), run_id).await {
            Ok(video_path) => return Ok(video_path),
            Err(e) => {
                tracing::warn!(
                    run_id = %run_id, attempt = attempt + 1, error = ?e,
                    "manim render failed, retrying"
                );
                let _ = tokio::fs::remove_file(&rel_path).await;
                last_err = Some(clean_manim_error(&e.to_string()));
            }
        }
    }

    Err(AppError::Render(format!(
        "manim failed after {} attempt(s). Last error: {}",
        attempts,
        last_err.unwrap_or_else(|| "unknown".into())
    )))
}

async fn svc_render(state: &AppState, script_path: &str, run_id: &str) -> AppResult<String> {
    let canonical = safe_storage_path(script_path)?;
    if canonical.extension().and_then(|e| e.to_str()) != Some("py") {
        return Err(AppError::BadRequest("script_path must end in .py".into()));
    }

    let source = tokio::fs::read_to_string(&canonical).await?;
    if regex::Regex::new(r"\bself\.frame\b").unwrap().is_match(&source) {
        return Err(AppError::Render(
            "generated script uses `self.frame` (no such attribute). \
             Must use `self.camera.frame` with MovingCameraScene + Cairo."
                .into(),
        ));
    }
    if regex::Regex::new(r"(?m)^[ \t]*self\.wait\(\s*max\(\s*0\.0").unwrap().is_match(&source)
        || regex::Regex::new(r"(?m)^[ \t]*self\.wait\(\s*0(?:\.0+)?\s*\)").unwrap().is_match(&source)
    {
        return Err(AppError::Render(
            "generated script contains self.wait(0) or self.wait(max(0.0, ...)); \
             Manim raises 'duration of 0 <= 0 seconds'. Use the guarded if-block."
                .into(),
        ));
    }

    let stem = canonical
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::BadRequest("invalid script filename".into()))?
        .to_string();

    let work_dir = canonical.parent()
        .ok_or_else(|| AppError::BadRequest("script has no parent directory".into()))?;
    let file_name = canonical.file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| AppError::BadRequest("invalid filename".into()))?;

    tokio::fs::create_dir_all("storage/output").await?;
    let media_dir = std::fs::canonicalize("storage/output")
        .map_err(|e| AppError::Render(format!("cannot resolve output dir: {e}")))?;
    let media_dir_arg = media_dir.to_string_lossy().replace('\\', "/");

    let quality_flag = state.config.manim_quality.flag().to_string();
    let res = state.config.manim_resolution.clone();
    let fps = state.config.manim_fps.to_string();

    let mut cmd = Command::new(&state.config.manim_bin);
    cmd.current_dir(work_dir)
        .arg(file_name)
        .arg("StoryAnimation")
        .arg(&quality_flag)
        .arg("--resolution").arg(&res)
        .arg("--fps").arg(&fps)
        .arg("-o").arg(format!("{stem}.mp4"))
        .arg("--media_dir").arg(&media_dir_arg)
        .arg("--renderer=cairo");

    if state.config.manim_disable_caching {
        cmd.arg("--disable_caching");
    }

    tracing::info!(
        run_id = %run_id,
        bin = %state.config.manim_bin,
        quality = quality_flag.as_str(),
        resolution = %res,
        fps = fps.as_str(),
        "invoking manim"
    );

    let timeout = Duration::from_secs(state.config.manim_timeout_secs);
    let output = match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(r) => r.map_err(|e| AppError::Render(format!("failed to spawn manim: {e}")))?,
        Err(_) => {
            return Err(AppError::Render(format!(
                "manim exceeded {timeout:?} timeout for {}", canonical.display()
            )))
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        tracing::error!(
            run_id = %run_id,
            stderr = %stderr,
            "manim render failed (full stderr)"
        );
        return Err(AppError::Render(format!("manim failed: {stderr}")));
    }

    // NOTE: Manim v0.22.0 changed output path resolution. The candidates below
    // work for v0.21.x. If you upgrade to v0.22.0+, read
    // scene.renderer.file_writer.final_file_path instead of constructing paths.
    // Pin your Manim version in the install script until you have verified
    // the new output surface.
    let mut candidates: Vec<PathBuf> = Vec::new();
    let res_folder = {
        let parts: Vec<&str> = res.split(',').collect();
        let h = parts.get(1).copied().unwrap_or("1080").trim();
        format!("{h}p{fps}")
    };
    candidates.push(
        media_dir.join("videos").join(&stem).join(&res_folder).join(format!("{stem}.mp4")),
    );
    let videos_root = media_dir.join("videos").join(&stem);
    if let Ok(rd) = std::fs::read_dir(&videos_root) {
        for entry in rd.flatten() {
            let p = entry.path().join(format!("{stem}.mp4"));
            if p.exists() {
                candidates.push(p);
            }
        }
    }

    let video_path = candidates.iter().find(|p| p.exists())
        .ok_or_else(|| AppError::Render("manim reported success but output not found".into()))?;

    Ok(video_path.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Combine (with symmetric duration guard + PCM intermediate)
// ---------------------------------------------------------------------------

async fn svc_combine(
    state: &AppState,
    video_path: &str,
    audio_path: &str,
    subtitle_path: Option<&str>,
) -> AppResult<String> {
    let video = safe_storage_path(video_path)?;
    let audio = safe_storage_path(audio_path)?;

    let audio_dur = svc_probe_duration(state, &audio.to_string_lossy()).await?;
    if !(audio_dur.is_finite() && audio_dur > 0.0) {
        return Err(AppError::Ffmpeg(format!("invalid audio duration: {audio_dur}")));
    }

    let video_dur = svc_probe_duration(state, &video.to_string_lossy()).await?;

    // SYMMETRIC GUARD: if video is shorter than audio, Manim sections underran.
    // If video is longer than audio, the LLM inserted extra waits that pushed
    // past the scene's slot. Both desync the remaining scenes from their audio.
    if video_dur + 0.05 < audio_dur {
        return Err(AppError::Render(format!(
            "video ({video_dur:.2}s) shorter than audio ({audio_dur:.2}s) — \
             Manim sections underran; the trailing padding wait did not fill its slot"
        )));
    }
    if video_dur > audio_dur + 0.5 {
        return Err(AppError::Render(format!(
            "video ({video_dur:.2}s) is {:.2}s longer than audio ({audio_dur:.2}s) — \
             a scene overshot its slot (likely an extra self.wait inserted mid-scene). \
             Trim excess or regenerate with tighter timing.",
            video_dur - audio_dur
        )));
    }

    let dur = format!("{audio_dur:.3}");
    let out = storage_join("output", &format!("final_{}.mp4", uuid::Uuid::new_v4()));

    let has_music = state.config.background_music.as_ref().map(|p| p.exists()).unwrap_or(false);
    let has_subs = subtitle_path.is_some() && state.config.burn_subtitles;

    let mut cmd = Command::new(&state.config.ffmpeg_bin);
    cmd.arg("-y").arg("-i").arg(&video).arg("-i").arg(&audio);

    let mut vf_parts: Vec<String> = vec![
        "eq=contrast=1.05:saturation=1.08".into(),
        "vignette=PI/5".into(),
        "tpad=stop_mode=clone:stop_duration=0.5".into(),
        format!("trim=duration={dur}"),
        "setpts=PTS-STARTPTS".into(),
    ];

    if has_subs {
        let subs = safe_storage_path(subtitle_path.unwrap())?;
        let subs_str = subs.to_string_lossy().replace('\\', "/");
        let escaped = if cfg!(windows) {
            subs_str.replacen(":", "\\:", 1)
        } else {
            subs_str
        };
        vf_parts.push(format!("subtitles='{}'", escaped.replace('\'', "'\\''")));
    }

    let video_chain = vf_parts.join(",");

    let filter: String;
    if has_music {
        let music = state.config.background_music.as_ref().unwrap();
        cmd.arg("-stream_loop").arg("-1").arg("-i").arg(music);

        filter = format!(
            "[0:v]{}[v];\
             [1:a]loudnorm=I=-16:TP=-1.5:LRA=11[voice];\
             [2:a]volume=0.18,atrim=duration={dur},asetpts=PTS-STARTPTS[music];\
             [music][voice]sidechaincompress=threshold=0.05:ratio=8:attack=5:release=250[ducked];\
             [voice][ducked]amix=inputs=2:duration=first:dropout_transition=2[a]",
            video_chain, dur = dur
        );
    } else {
        filter = format!(
            "[0:v]{}[v];[1:a]loudnorm=I=-16:TP=-1.5:LRA=11[a]",
            video_chain
        );
    }

    cmd.arg("-filter_complex").arg(&filter)
        .arg("-map").arg("[v]")
        .arg("-map").arg("[a]")
        .arg("-c:v").arg("libx264")
        .arg("-preset").arg("slow")
        .arg("-crf").arg("18")
        .arg("-pix_fmt").arg("yuv420p")
        .arg("-movflags").arg("+faststart")
        .arg("-c:a").arg("aac")
        .arg("-b:a").arg("192k")
        .arg("-t").arg(&dur)
        .arg(&out);

    let output = cmd.output().await
        .map_err(|e| AppError::Ffmpeg(format!("failed to spawn ffmpeg: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Ffmpeg(format!("ffmpeg failed: {stderr}")));
    }
    if !out.exists() {
        return Err(AppError::Ffmpeg("ffmpeg reported success but output missing".into()));
    }

    Ok(out.to_string_lossy().into_owned())
}

async fn svc_probe_duration(state: &AppState, audio_path: &str) -> AppResult<f64> {
    let canonical = safe_storage_path(audio_path)?;

    let output = Command::new(&state.config.ffprobe_bin)
        .args(["-v", "error", "-show_entries", "format=duration",
               "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(&canonical)
        .output()
        .await
        .map_err(|e| AppError::Ffmpeg(format!("failed to spawn ffprobe: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Ffmpeg(format!("ffprobe failed: {stderr}")));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let dur = text.trim().parse::<f64>()
        .map_err(|e| AppError::Ffmpeg(format!("could not parse ffprobe duration: {e}")))?;
    if !(dur.is_finite() && dur > 0.0) {
        return Err(AppError::Ffmpeg(format!("ffprobe returned invalid duration: {dur}")));
    }
    Ok(dur)
}

async fn concat_audio(state: &AppState, clips: &[PathBuf], out: &Path) -> AppResult<()> {
    if clips.is_empty() {
        return Err(AppError::Ffmpeg("no audio clips to concat".into()));
    }

    for c in clips {
        let d = svc_probe_duration(state, &c.to_string_lossy()).await?;
        if d <= 0.0 {
            return Err(AppError::Ffmpeg(format!(
                "clip has zero duration: {}", c.display()
            )));
        }
    }

    let list = storage_join("audio", &format!("list_{}.txt", uuid::Uuid::new_v4()));

    let mut content = String::new();
    for c in clips {
        let abs = std::fs::canonicalize(c)
            .map_err(|e| AppError::Ffmpeg(format!("clip does not exist: {} ({e})", c.display())))?;
        let escaped = abs.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
        content.push_str(&format!("file '{escaped}'\n"));
    }
    tokio::fs::write(&list, content).await?;

    let list_abs = std::fs::canonicalize(&list)
        .map_err(|e| AppError::Ffmpeg(format!("concat list missing: {e}")))?;

    // PCM intermediate: avoid double lossy encode. ElevenLabs gives MP3;
    // decode to PCM during concat, then encode to AAC exactly once in combine.
    let output = Command::new(&state.config.ffmpeg_bin)
        .arg("-y")
        .arg("-f").arg("concat")
        .arg("-safe").arg("0")
        .arg("-i").arg(&list_abs)
        .arg("-ar").arg("44100")
        .arg("-ac").arg("1")
        .arg("-c:a").arg("pcm_s16le")
        .arg(out)
        .output()
        .await
        .map_err(|e| AppError::Ffmpeg(format!("concat spawn: {e}")))?;

    let _ = tokio::fs::remove_file(&list).await;

    if !output.status.success() {
        return Err(AppError::Ffmpeg(format!(
            "concat failed: {}", String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP types + handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct StoryboardRequest { topic: String }
#[derive(Serialize)]
struct StoryboardResponse { storyboard: Storyboard }

#[derive(Deserialize)]
struct ScriptRequest { topic: String }
#[derive(Serialize)]
struct ScriptResponse { script: String }

#[derive(Deserialize)]
struct AudioRequest { script: String, voice_id: String }
#[derive(Serialize)]
struct AudioResponse { file_path: String }

#[derive(Deserialize)]
struct ManimRequest {
    topic: String,
    #[serde(default = "default_duration")]
    audio_duration: f64,
}
fn default_duration() -> f64 { 30.0 }
#[derive(Serialize)]
struct ManimResponse { file_path: String }

#[derive(Deserialize)]
struct RenderRequest { script_path: String }
#[derive(Serialize)]
struct RenderResponse { video_path: String }

#[derive(Deserialize)]
struct CombineRequest {
    video_path: String,
    audio_path: String,
    #[serde(default)]
    subtitle_path: Option<String>,
}
#[derive(Serialize)]
struct CombineResponse { final_video_path: String }

#[derive(Deserialize)]
struct MasterRequest { topic: String, voice_id: String }
#[derive(Serialize)]
struct MasterResponse {
    final_video_path: String,
    run_id: String,
    subtitle_path: Option<String>,
}

async fn handle_healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

async fn handle_readyz(State(state): State<AppState>) -> impl IntoResponse {
    let mut checks: Vec<(&'static str, bool, String)> = Vec::new();

    match tokio::fs::create_dir_all("storage/output").await {
        Ok(_) => checks.push(("storage", true, "writable".into())),
        Err(e) => checks.push(("storage", false, e.to_string())),
    }

    // Manim version check — warn if v0.22.0+ is detected without explicit pinning
    match Command::new(&state.config.manim_bin).arg("--version").output().await {
        Ok(o) if o.status.success() => {
            let v = (String::from_utf8_lossy(&o.stdout).trim().to_string()
                + String::from_utf8_lossy(&o.stderr).trim()).trim().to_string();
            let warn = if v.contains("0.22") {
                "WARNING: Manim v0.22.0 changed output paths; svc_render candidate resolution may fail. Pin to 0.21.x or update candidates."
            } else {
                ""
            };
            checks.push(("manim", true, format!("bin={} version={v} {warn}", state.config.manim_bin)));
        }
        Ok(o) => checks.push((
            "manim",
            false,
            format!(
                "bin={} stderr={}",
                state.config.manim_bin,
                String::from_utf8_lossy(&o.stderr)
            ),
        )),
        Err(e) => checks.push((
            "manim",
            false,
            format!("bin={} error={e}", state.config.manim_bin),
        )),
    }

    match Command::new(&state.config.ffmpeg_bin).arg("-version").output().await {
        Ok(o) if o.status.success() => checks.push(("ffmpeg", true, "ok".into())),
        Ok(o) => checks.push(("ffmpeg", false, String::from_utf8_lossy(&o.stderr).into_owned())),
        Err(e) => checks.push(("ffmpeg", false, e.to_string())),
    }

    // Font availability — libass silently substitutes or renders nothing
    // if the subtitle font is missing. fc-match is cheap and catches it early.
    match Command::new("fc-match").arg(&state.config.subtitle_font).output().await {
        Ok(o) if o.status.success() => {
            let matched = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let requested = &state.config.subtitle_font;
            let ok = matched.to_lowercase().contains(&requested.to_lowercase());
            checks.push((
                "subtitle_font",
                ok,
                if ok {
                    format!("font={requested} matched={matched}")
                } else {
                    format!(
                        "font={requested} resolved to {matched} — install the font \
                         or set SUBTITLE_FONT to an available family"
                    )
                },
            ));
        }
        Ok(o) => checks.push((
            "subtitle_font",
            false,
            format!("fc-match failed: {}", String::from_utf8_lossy(&o.stderr)),
        )),
        Err(e) => checks.push((
            "subtitle_font",
            false,
            format!("fc-match not found ({e}) — install fontconfig to enable font checks"),
        )),
    }

    let ready = checks.iter().all(|(_, ok, _)| *ok);
    let status = if ready { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    let payload: Vec<Value> = checks.into_iter()
        .map(|(n, ok, d)| json!({"name": n, "ok": ok, "detail": d}))
        .collect();
    (status, Json(json!({"ready": ready, "checks": payload})))
}

async fn handle_storyboard(
    State(state): State<AppState>,
    Json(payload): Json<StoryboardRequest>,
) -> AppResult<Json<StoryboardResponse>> {
    validate_topic(&payload.topic)?;
    let storyboard = svc_generate_storyboard(&state, &payload.topic).await?;
    Ok(Json(StoryboardResponse { storyboard }))
}

async fn handle_script(
    State(state): State<AppState>,
    Json(payload): Json<ScriptRequest>,
) -> AppResult<Json<ScriptResponse>> {
    validate_topic(&payload.topic)?;
    let board = svc_generate_storyboard(&state, &payload.topic).await?;
    let script = board.scenes.iter()
        .map(|s| s.narration.trim().to_string())
        .collect::<Vec<_>>().join(" ");
    Ok(Json(ScriptResponse { script }))
}

async fn handle_audio(
    State(state): State<AppState>,
    Json(payload): Json<AudioRequest>,
) -> AppResult<Json<AudioResponse>> {
    if payload.script.trim().is_empty() {
        return Err(AppError::BadRequest("script is empty".into()));
    }
    validate_voice_id(&payload.voice_id)?;
    let file_path = svc_generate_audio(&state, &payload.script, &payload.voice_id).await?;
    Ok(Json(AudioResponse { file_path }))
}

async fn handle_manim(
    State(state): State<AppState>,
    Json(payload): Json<ManimRequest>,
) -> AppResult<Json<ManimResponse>> {
    validate_topic(&payload.topic)?;
    if !(1.0..=3600.0).contains(&payload.audio_duration) {
        return Err(AppError::BadRequest("audio_duration out of range".into()));
    }

    let board = svc_generate_storyboard(&state, &payload.topic).await?;
    let durations = vec![payload.audio_duration; board.scenes.len()];

    let bg = pick_background(&board);
    let prompt = build_manim_prompt(
        &board, &durations, None,
        state.config.premium_examples, state.config.enable_typst,
    )?;
    let code = svc_request_manim_code(&state, &prompt, &bg).await?;

    let rel_path = storage_join("scripts", &format!("{}.py", uuid::Uuid::new_v4()));
    tokio::fs::write(&rel_path, code.as_bytes()).await?;
    Ok(Json(ManimResponse { file_path: rel_path.to_string_lossy().into_owned() }))
}

async fn handle_render(
    State(state): State<AppState>,
    Json(payload): Json<RenderRequest>,
) -> AppResult<Json<RenderResponse>> {
    let run_id = uuid::Uuid::new_v4().to_string();
    let video_path = svc_render(&state, &payload.script_path, &run_id).await?;
    Ok(Json(RenderResponse { video_path }))
}

async fn handle_combine(
    State(state): State<AppState>,
    Json(payload): Json<CombineRequest>,
) -> AppResult<Json<CombineResponse>> {
    let final_video_path = svc_combine(
        &state, &payload.video_path, &payload.audio_path,
        payload.subtitle_path.as_deref(),
    ).await?;
    Ok(Json(CombineResponse { final_video_path }))
}

async fn handle_generate(
    State(state): State<AppState>,
    Json(payload): Json<MasterRequest>,
) -> AppResult<Json<MasterResponse>> {
    validate_topic(&payload.topic)?;
    validate_voice_id(&payload.voice_id)?;

    let run_id = uuid::Uuid::new_v4().to_string();
    tracing::info!(run_id = %run_id, topic = %payload.topic, "pipeline: start");

    let board = svc_generate_storyboard(&state, &payload.topic).await?;
    tracing::info!(
        run_id = %run_id, scenes = board.scenes.len(),
        style = %board.style, title = %board.title,
        "pipeline: storyboard ready"
    );

    let conc = state.config.tts_concurrency.max(1);
    let mut per_scene: Vec<(usize, PathBuf, f64)> = stream::iter(
        board.scenes.iter().cloned().enumerate().map(|(i, scene)| {
            let state = state.clone();
            let voice_id = payload.voice_id.clone();
            async move {
                let p = svc_generate_audio(&state, &scene.narration, &voice_id).await?;
                let d = svc_probe_duration(&state, &p).await?;
                Ok::<_, AppError>((i, PathBuf::from(p), d))
            }
        }),
    )
    .buffer_unordered(conc)
    .try_collect::<Vec<_>>()
    .await?;
    per_scene.sort_by_key(|(i, _, _)| *i);

    let mut clips: Vec<PathBuf> = Vec::with_capacity(per_scene.len());
    let mut durations: Vec<f64> = Vec::with_capacity(per_scene.len());
    for (i, path, d) in per_scene {
        tracing::info!(run_id = %run_id, scene = i, duration_secs = d, "pipeline: scene audio ready");
        clips.push(path);
        durations.push(d);
    }

    let total: f64 = durations.iter().sum();
    if !total.is_finite() || total < state.config.min_total_duration_secs {
        return Err(AppError::BadRequest(format!(
            "total audio too short: {total:.2}s (min {})",
            state.config.min_total_duration_secs
        )));
    }
    if total > state.config.max_total_duration_secs {
        return Err(AppError::BadRequest(format!(
            "total audio too long: {total:.2}s (max {})",
            state.config.max_total_duration_secs
        )));
    }

    let subtitle_rel = if state.config.burn_subtitles {
        let ass = build_ass(
            &board,
            &durations,
            &state.config.subtitle_font,
            state.config.subtitle_font_size,
        );
        let p = storage_join("output", &format!("subs_{}.ass", uuid::Uuid::new_v4()));
        tokio::fs::write(&p, ass.as_bytes()).await?;
        tracing::info!(run_id = %run_id, path = %p.display(), "pipeline: subtitles written");
        Some(p.to_string_lossy().into_owned())
    } else {
        None
    };

    tracing::info!(run_id = %run_id, "pipeline: concat audio (PCM intermediate)");
    let full_audio = storage_join("audio", &format!("full_{}.wav", uuid::Uuid::new_v4()));
    concat_audio(&state, &clips, &full_audio).await?;

    tracing::info!(run_id = %run_id, "pipeline: manim code + render");
    let video_path = svc_generate_manim_with_render(&state, &board, &durations, &run_id).await?;

    tracing::info!(run_id = %run_id, "pipeline: combine");
    let final_path = svc_combine(
        &state, &video_path,
        full_audio.to_str().ok_or_else(|| AppError::BadRequest("audio path non-UTF8".into()))?,
        subtitle_rel.as_deref(),
    ).await?;

    tracing::info!(run_id = %run_id, final_path = %final_path, "pipeline: complete");
    Ok(Json(MasterResponse {
        final_video_path: final_path,
        run_id,
        subtitle_path: subtitle_rel,
    }))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => { eprintln!("configuration error: {e}"); std::process::exit(1); }
    };

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".into());
    if config.log_json {
        tracing_subscriber::fmt().with_env_filter(filter).json().init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    for dir in ["storage/audio", "storage/scripts", "storage/output"] {
        tokio::fs::create_dir_all(dir).await?;
    }

    tracing::info!(
        renderer = "cairo",
        quality = config.manim_quality.flag(),
        resolution = %config.manim_resolution,
        fps = config.manim_fps,
        manim_bin = %config.manim_bin,
        antialiasing = config.enable_antialiasing,
        typst = config.enable_typst,
        subtitles = config.burn_subtitles,
        "visual quality settings"
    );

    let state = AppState {
        client: Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(300))
            .build()?,
        config: Arc::new(config),
    };

    let app = Router::new()
        .route("/healthz", get(handle_healthz))
        .route("/readyz", get(handle_readyz))
        .route("/api/storyboard", post(handle_storyboard))
        .route("/api/script", post(handle_script))
        .route("/api/audio", post(handle_audio))
        .route("/api/manim", post(handle_manim))
        .route("/api/render", post(handle_render))
        .route("/api/combine", post(handle_combine))
        .route("/api/generate", post(handle_generate))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(state);

    let addr = "0.0.0.0:3000";
    tracing::info!(addr = %addr, "listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}