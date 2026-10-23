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
                 '-V' is not a valid Manim quality flag and produces \
                 \"Error: No such option '-V'\"."
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

        // Cairo only. Reject stale opengl configs so the cinematic camera
        // API (self.camera.frame.*) never runs under a renderer that lacks it.
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
            gemini_model: env_default("GEMINI_MODEL", "gemini-2.5-flash"),
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
// Subtitles
// ---------------------------------------------------------------------------

fn srt_timestamp(secs: f64) -> String {
    let total_ms = (secs * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_s = total_ms / 1000;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

fn build_srt(board: &Storyboard, durations: &[f64]) -> String {
    let mut out = String::new();
    let mut t0 = 0.0_f64;
    let mut idx = 1;

    for (i, scene) in board.scenes.iter().enumerate() {
        let dur = durations.get(i).copied().unwrap_or(0.0);
        if dur <= 0.0 {
            continue;
        }
        let text = scene.narration.trim();
        if text.is_empty() {
            t0 += dur;
            continue;
        }

        let cues: Vec<String> = if text.len() > 120 {
            let mid = text.len() / 2;
            let split_at = text[..mid]
                .rfind(|c: char| c == '.' || c == '?' || c == '!')
                .map(|p| p + 1)
                .unwrap_or(mid);
            vec![
                text[..split_at].trim().to_string(),
                text[split_at..].trim().to_string(),
            ]
        } else {
            vec![text.to_string()]
        };

        let n = cues.len();
        for (j, cue) in cues.iter().enumerate() {
            if cue.is_empty() {
                continue;
            }
            let cue_start = t0 + dur * (j as f64) / (n as f64);
            let cue_end = t0 + dur * ((j + 1) as f64) / (n as f64);
            let _ = writeln!(
                out,
                "{}\n{} --> {}\n{}\n",
                idx,
                srt_timestamp(cue_start),
                srt_timestamp(cue_end),
                cue
            );
            idx += 1;
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
      "palette": ["#RRGGBB", "#RRGGBB", "#RRGGBB"]
    }}
  ]
}}

Rules:
- 5 to 8 scenes. Each scene is 4-10 seconds of narration.
- The FIRST palette color of scene 1 becomes the video background. It MUST be near-black
  (#0B1026, #111116, #14121A) OR near-white (#FAFAFA, #F4F1DE). Never mid-tone blue/green/red.
- The SECOND palette color is the primary ink (text + main shape) and must contrast the bg.
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
// Premium Manim prompt
// ---------------------------------------------------------------------------

const PREMIUM_EXAMPLES: &str = r##"
# =========================================================================
# REFERENCE: a "premium" Manim scene. Pattern-match the idioms below.
# Camera animation is ALWAYS self.camera.frame.*, never self.frame.*
# =========================================================================
from manim import *

class PremiumReference(MovingCameraScene):
    def construct(self):
        BG, INK, ACCENT, WARN = "#0B1026", "#F4F1DE", "#FFB703", "#E63946"
        self.camera.background_color = BG

        dot   = Dot(ORIGIN, color=INK)
        ring  = Circle(radius=0.6, color=ACCENT, stroke_width=3)
        arrow = Arrow(ORIGIN, RIGHT*2 + UP*1.5, color=ACCENT, buff=0)
        self.play(FadeIn(dot, scale=0.5))
        self.play(ReplacementTransform(dot, ring))
        self.play(ReplacementTransform(ring, arrow))

        path = ArcBetweenPoints(arrow.get_end(), arrow.get_end() + LEFT*3, angle=-PI/2)
        orb  = Dot(arrow.get_end(), color=WARN)
        self.play(MoveAlongPath(orb, path), run_time=2.0, rate_func=smooth)

        t = ValueTracker(0.0)
        live = always_redraw(
            lambda: DecimalNumber(t.get_value(), num_decimal_places=1)
            .set_color(ACCENT).to_corner(UR)
        )
        self.add(live)
        self.play(t.animate.set_value(42.0), run_time=2.0, rate_func=smooth)

        # CINEMATIC CAMERA — Cairo / MovingCameraScene idiom
        self.camera.frame.save_state()
        self.play(self.camera.frame.animate.scale(0.6).move_to(live.get_center()))

        box = SurroundingRectangle(live, color=ACCENT, buff=0.15)
        self.play(Create(box))
        self.play(Indicate(live, color=ACCENT, scale_factor=1.2))

        self.play(self.camera.frame.animate.restore(), run_time=1.0,
                  rate_func=there_and_back)

        line = Line(LEFT*3, RIGHT*3, color=INK)
        brace = Brace(line, DOWN, color=ACCENT)
        label = Text("x", color=ACCENT).next_to(brace, DOWN, buff=0.1)
        self.play(Create(line), GrowFromCenter(brace), FadeIn(label))
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
    let palette = board
        .scenes
        .first()
        .map(|s| s.palette.clone())
        .unwrap_or_default();

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
2. Target total runtime ~ {target:.2} seconds. Use `self.next_section("scene_i")` per scene.
   Capture `section_start = self.renderer.time` right after each `next_section`, then
   finish each section with:
   `self.wait(max(0.0, TARGET_I - (self.renderer.time - section_start)))`
3. Colors: declare `BG, INK, ACCENT, WARN = "<bg>", "<ink>", "<accent>", "<warn>"` at top of construct.
   `self.camera.background_color = BG` as first executable line.
4. NO Tex, MathTex, or any LaTeX. Allowed mobjects: Text, VGroup, Circle, Square,
   Rectangle, Line, Arrow, Dot, Polygon, RegularPolygon, RoundedRectangle, Arc,
   Ellipse, Triangle, Annulus, SurroundingRectangle, Brace, NumberLine, Axes,
   DecimalNumber, ArcBetweenPoints, DashedLine, ValueTracker, Typst.
4b. NEVER call `brace.get_text(...)` or `brace.get_tex(...)`. Use
    `label = Text("x").next_to(brace, DOWN, buff=0.1)` and `FadeIn(label)`.
{typst_rule}
5. TECHNIQUE DENSITY CONTRACT — graded. Must contain:
   (a) >=3 Transform or ReplacementTransform chains,
   (b) >=1 of MoveAlongPath / ValueTracker+always_redraw / Axes.plot,
   (c) >=1 camera save_state + scale/move_to + restore using `self.camera.frame`,
   (d) >=1 SurroundingRectangle + Indicate pairing,
   (e) >=1 Brace / NumberLine / Axes,
   (f) >=1 rate_func=there_and_back or overshoot.
6. No `self.wait(x)` with x > 0.5 except the padding wait at the end of a section.
7. Group text into VGroups with arrange(DOWN, buff=0.3); animate the group.
8. LAST frame must be composed (transform into closing title, or pull back),
   not FadeOut+Write.
9. Imports: `from manim import *`
10. Return ONLY one ```python fenced block.

CAMERA API — MANDATORY (this is the #1 source of runtime crashes):

11. The renderer is Cairo. The ONLY valid camera animation surface is
    `self.camera.frame`. Use it exactly like this:
        self.camera.frame.save_state()
        self.play(self.camera.frame.animate.scale(0.6).move_to(focus.get_center()))
        self.play(self.camera.frame.animate.restore())
    To zoom in: scale(<1). To zoom out: scale(>1). To pan: `.move_to(point)`.
    Combined: `self.play(self.camera.frame.animate.scale(0.7).move_to(target))`.
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

{retry_note}

Style: {style}
Title: {title}
Palette (BG=palette[0], INK=palette[1], ACCENT=palette[2]): {palette:?}

Scenes:
{scenes_json}

Durations (seconds): {durations_json}
Total target: {target:.2}s

Produce the file now."##,
        style = board.style,
        title = board.title,
        palette = palette,
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

/// Deterministic fix for:
///   AttributeError: 'StoryAnimation' object has no attribute 'frame'
///
/// `self.frame` does not exist on a Scene in Manim CE. The only valid
/// camera surface for `MovingCameraScene` under Cairo is `self.camera.frame`.
/// `self.camera.frame` does not contain `self.frame` as a substring, so this
/// replacement is safe.
fn force_camera_frame_idiom(code: String) -> String {
    use regex::Regex;
    let re = Regex::new(r"\bself\.frame\b").unwrap();
    re.replace_all(&code, "self.camera.frame").into_owned()
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

fn pick_background(board: &Storyboard) -> String {
    board
        .scenes
        .first()
        .and_then(|s| s.palette.first())
        .cloned()
        .unwrap_or_else(|| "#0B1026".into())
}

async fn svc_request_manim_code(
    state: &AppState,
    prompt: &str,
    bg: &str,
) -> AppResult<String> {
    let body = json!({
        "model": "deepseek-chat",
        "messages": [
            {"role": "system", "content": "You are a world-class Manim developer. Output only valid, runnable Python. Never use LaTeX. Camera animation MUST use self.camera.frame (Cairo/MovingCameraScene). NEVER write self.frame. Line/Arrow/DashedLine/Vector/ArcBetweenPoints take exactly two positional arguments. Never call brace.get_text or brace.get_tex — use Text(\"...\").next_to(brace, ...). Prefer transform chains, path animation, camera moves, and semantic highlights. Return only one ```python fenced block."},
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
                last_err = Some(e.to_string());
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

    // Fast-fail if `self.frame` slipped through the sanitizer. This runs
    // BEFORE spawning manim so we don't waste 60s of render time on a script
    // that cannot possibly execute.
    let source = tokio::fs::read_to_string(&canonical).await?;
    if regex::Regex::new(r"\bself\.frame\b").unwrap().is_match(&source) {
        return Err(AppError::Render(
            "generated script uses `self.frame` (no such attribute). \
             Must use `self.camera.frame` with MovingCameraScene + Cairo."
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

    // NOTE: `-aaVector` is a ManimGL-only flag. Manim CE (the `manim` on PATH
    // here) rejects it, and because clap walks clustered short flags
    // character-by-character it surfaces as the confusing
    //   "Error: No such option '-V'."
    // Cairo antialiasing is already on by default and controlled by
    // --quality (-qh renders AA'd at 1080p60). Do not pass any -aa* flags.

    if state.config.manim_disable_caching {
        cmd.arg("--disable_caching");
    }

    // Log the exact argv one time so a future `-V`-class bug is a 5-second
    // diagnosis, not a mystery. This prints BEFORE we spawn, so it fires even
    // when manim immediately rejects the flags.
    tracing::info!(
        run_id = %run_id,
        bin = %state.config.manim_bin,
        quality = quality_flag.as_str(),
        resolution = %res,
        fps = fps.as_str(),
        antialiasing = state.config.enable_antialiasing,
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
// Combine (with subtitle burn-in)
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
        let style = format!(
            "FontName={},FontSize={},PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,Outline=2,MarginV=40",
            state.config.subtitle_font, state.config.subtitle_font_size
        );
        vf_parts.push(format!(
            "subtitles='{}':force_style='{}'",
            escaped.replace('\'', "'\\''"),
            style
        ));
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

    let output = Command::new(&state.config.ffmpeg_bin)
        .arg("-y")
        .arg("-f").arg("concat")
        .arg("-safe").arg("0")
        .arg("-i").arg(&list_abs)
        .arg("-ar").arg("44100")
        .arg("-ac").arg("1")
        .arg("-c:a").arg("libmp3lame")
        .arg("-b:a").arg("192k")
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

    match Command::new(&state.config.manim_bin).arg("--version").output().await {
        Ok(o) if o.status.success() => {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string()
                + String::from_utf8_lossy(&o.stderr).trim();
            checks.push(("manim", true, format!("bin={} version={v}", state.config.manim_bin)));
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
        let srt = build_srt(&board, &durations);
        if srt.trim().is_empty() {
            None
        } else {
            let p = storage_join("output", &format!("subs_{}.srt", uuid::Uuid::new_v4()));
            tokio::fs::write(&p, srt.as_bytes()).await?;
            tracing::info!(run_id = %run_id, path = %p.display(), "pipeline: subtitles written");
            Some(p.to_string_lossy().into_owned())
        }
    } else {
        None
    };

    tracing::info!(run_id = %run_id, "pipeline: concat audio");
    let full_audio = storage_join("audio", &format!("full_{}.mp3", uuid::Uuid::new_v4()));
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