use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
        tracing::error!(error = %self, "request failed");
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

type AppResult<T> = Result<T, AppError>;

// ---------------------------------------------------------------------------
// Config + state
// ---------------------------------------------------------------------------

struct Config {
    gemini_api_key: String,
    elevenlabs_api_key: String,
    deepseek_api_key: String,
    manim_bin: String,
    ffmpeg_bin: String,
    ffprobe_bin: String,
    // Quality / cinematic controls
    manim_quality: String,          // ql | qm | qh | qk
    manim_resolution: String,       // e.g. "1920,1080"
    manim_fps: u32,
    elevenlabs_model: String,
    background_music: Option<PathBuf>,
    // Retry budget for LLM -> Manim code failure recovery
    manim_max_retries: u32,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        dotenv::dotenv().ok();

        let required = |name: &str| {
            env::var(name).map_err(|_| format!("missing required env var: {name}"))
        };
        let optional = |name: &str, default: &str| {
            env::var(name).unwrap_or_else(|_| default.to_string())
        };

        Ok(Config {
            gemini_api_key: required("GEMINI_API_KEY")?,
            elevenlabs_api_key: required("ELEVENLABS_API_KEY")?,
            deepseek_api_key: required("DEEPSEEK_API_KEY")?,
            manim_bin: optional("MANIM_BIN", "manim"),
            ffmpeg_bin: optional("FFMPEG_BIN", "ffmpeg"),
            ffprobe_bin: optional("FFPROBE_BIN", "ffprobe"),
            manim_quality: optional("MANIM_QUALITY", "qk"),
            manim_resolution: optional("MANIM_RESOLUTION", "1920,1080"),
            manim_fps: optional("MANIM_FPS", "60").parse().unwrap_or(60),
            elevenlabs_model: optional("ELEVENLABS_MODEL", "eleven_turbo_v2_5"),
            background_music: env::var("BACKGROUND_MUSIC").ok().map(PathBuf::from),
            manim_max_retries: optional("MANIM_MAX_RETRIES", "1").parse().unwrap_or(1),
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
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) || id.len() > 64 {
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
// Storyboard model
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
// Services
// ---------------------------------------------------------------------------

async fn svc_generate_storyboard(state: &AppState, topic: &str) -> AppResult<Storyboard> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.5-flash:generateContent?key={}",
        state.config.gemini_api_key
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
      "visual": "precise visual description an animator can build in Manim: shapes, motion, arrows, transforms, groupings",
      "mood": "one word",
      "palette": ["#RRGGBB", "#RRGGBB", "#RRGGBB"]
    }}
  ]
}}

Rules:
- 5 to 8 scenes. Each scene is 4-10 seconds of narration.
- Vary visuals: never repeat the same composition twice in a row.
- Use a coherent palette across the video (reuse 2-3 colors).
- Ideas should build: hook -> setup -> turn -> insight -> payoff.
- No LaTeX, no math notation in narration — speak it plainly.
"##,
    );

    let body = json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "temperature": 0.9
        }
    });

    let response = state.client.post(&url).json(&body).send().await?;
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

async fn svc_generate_audio(
    state: &AppState,
    script: &str,
    voice_id: &str,
) -> AppResult<String> {
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

    let bytes = response.bytes().await?;
    if bytes.is_empty() {
        return Err(AppError::ElevenLabs("empty audio body".into()));
    }

    let rel_path = storage_join("audio", &format!("{}.mp3", uuid::Uuid::new_v4()));
    tokio::fs::write(&rel_path, &bytes).await?;
    Ok(rel_path.to_string_lossy().into_owned())
}

fn build_manim_prompt(
    board: &Storyboard,
    scene_durations: &[f64],
    previous_error: Option<&str>,
) -> AppResult<String> {
    let total: f64 = scene_durations.iter().sum();
    let target = (total * 0.97).max(1.0);

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
            "\n\nIMPORTANT: A previous attempt failed with this Manim error. Fix it and do not \
             repeat the same mistake:\n---\n{}\n---\n",
            err.chars().take(2000).collect::<String>()
        ),
        None => String::new(),
    };

    Ok(format!(
        r##"You are a world-class Manim animator (3Blue1Brown tier).
Write ONE Python file that renders a cinematic explanation in the style "{style}".

Hard requirements:
1. Define exactly one scene class: `class StoryAnimation(MovingCameraScene):`
2. Target total runtime ~ {target:.2} seconds. Split the video into scenes with
   `self.next_section("scene_i")` and match the per-scene timing array below.
3. Use ONLY these color names: WHITE, BLACK, GRAY, RED, GREEN, BLUE, YELLOW,
   ORANGE, PURPLE, PINK, TEAL, GOLD, MAROON, DARK_BLUE. Any custom color MUST be
   a hex string like "#FFB703".
4. NO Tex, MathTex, or any LaTeX. Use ONLY: Text, VGroup, Circle, Square,
   Rectangle, Line, Arrow, Dot, Polygon, RegularPolygon, RoundedRectangle, Arc,
   Ellipse, Triangle, Annulus, SurroundingRectangle, Brace, NumberLine, Axes.
5. Every scene must have motion: Create, Write, FadeIn, Transform,
   ReplacementTransform, MoveAlongPath, Rotate, ScaleInPlace, ApplyWave,
   LaggedStart, etc. NO static slides.
6. Set `self.camera.background_color = "{bg}"` in `construct`.
7. Use `self.camera.frame.save_state()` and
   `self.camera.frame.animate.scale(...)` for cinematic zooms. This requires the
   `MovingCameraScene` base class — do not subclass plain `Scene`.
8. Group related text into `VGroup`s with `arrange(DOWN, buff=0.3)` and animate
   the group.
9. Add a title card at the start and a fade-out at the end.
10. Imports must be only: `from manim import *`.
11. Return ONLY the Python code inside a single ```python fenced block.
    No prose, no markdown outside the fence.{retry_note}

Storyboard style: {style}
Title: {title}
Palette (use these across the video): {palette:?}

Scenes (JSON, in order):
{scenes_json}

Per-scene durations in seconds (must match order): {durations_json}
Total target: {target:.2}s

Produce the file now."##,
        style = board.style,
        title = board.title,
        bg = palette.first().cloned().unwrap_or_else(|| "#0B1026".into()),
        palette = palette,
        scenes_json = scenes_json,
        durations_json = durations_json,
        target = target,
        retry_note = retry_note,
    ))
}

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

fn sanitize_manim_code(mut code: String) -> String {
    // Strip LaTeX mobjects that sometimes slip through.
    if code.contains("MathTex") {
        code = code.replace("MathTex", "Text");
    }
    if code.contains("Tex(") {
        code = code.replace("Tex(", "Text(");
    }

    // Upgrade bare Scene base → MovingCameraScene so `self.camera.frame` works.
    if code.contains("class StoryAnimation(Scene)") {
        code = code.replace(
            "class StoryAnimation(Scene)",
            "class StoryAnimation(MovingCameraScene)",
        );
    }

    // Ensure a background color is set.
    if !code.contains("background_color") {
        code = code.replacen(
            "class StoryAnimation(MovingCameraScene):",
            "class StoryAnimation(MovingCameraScene):\n        self.camera.background_color = \"#0B1026\"",
            1,
        );
    }
    code
}

async fn svc_request_manim_code(
    state: &AppState,
    prompt: &str,
) -> AppResult<String> {
    let body = json!({
        "model": "deepseek-chat",
        "messages": [
            {"role": "system", "content": "You are a world-class Manim developer. Output only valid, runnable Python. Never use LaTeX. Never use non-standard color constants."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.6
    });

    let response = state
        .client
        .post("https://api.deepseek.com/chat/completions")
        .header(
            "Authorization",
            format!("Bearer {}", state.config.deepseek_api_key),
        )
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
    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");

    let code = extract_python(content)
        .ok_or_else(|| AppError::DeepSeek("no python code block found".into()))?;

    let code = sanitize_manim_code(code);

        if !code.contains("class StoryAnimation(MovingCameraScene)") {
        return Err(AppError::DeepSeek(
            "generated code must define `class StoryAnimation(MovingCameraScene):`".into(),
        ));
    }

    if !code.contains("class StoryAnimation") {
        return Err(AppError::DeepSeek(
            "generated code does not define class StoryAnimation".into(),
        ));   
    }

    Ok(code)
}

/// Generate Manim code, then attempt a single render. If render fails and the
/// retry budget allows, re-ask the LLM with the error message.
async fn svc_generate_manim_with_render(
    state: &AppState,
    board: &Storyboard,
    scene_durations: &[f64],
) -> AppResult<String> {
    let mut last_err: Option<String> = None;
    let attempts = state.config.manim_max_retries.saturating_add(1);

    for attempt in 0..attempts {
        let prompt = build_manim_prompt(board, scene_durations, last_err.as_deref())?;
        let code = svc_request_manim_code(state, &prompt).await?;

        let rel_path = storage_join("scripts", &format!("{}.py", uuid::Uuid::new_v4()));
        tokio::fs::write(&rel_path, code.as_bytes()).await?;

        tracing::info!(attempt = attempt + 1, path = %rel_path.display(), "manim code written");

        match svc_render(state, &rel_path.to_string_lossy()).await {
            Ok(video_path) => return Ok(video_path),
            Err(e) => {
                tracing::warn!(attempt = attempt + 1, error = %e, "manim render failed, retrying");
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

async fn svc_render(state: &AppState, script_path: &str) -> AppResult<String> {
    let canonical = safe_storage_path(script_path)?;
    if canonical.extension().and_then(|e| e.to_str()) != Some("py") {
        return Err(AppError::BadRequest("script_path must end in .py".into()));
    }

    let stem = canonical
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::BadRequest("invalid script filename".into()))?
        .to_string();

    let work_dir = canonical
        .parent()
        .ok_or_else(|| AppError::BadRequest("script has no parent directory".into()))?;
    let file_name = canonical
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| AppError::BadRequest("invalid filename".into()))?;

    let media_dir = std::fs::canonicalize("storage/output")
        .map_err(|e| AppError::Render(format!("cannot resolve output dir: {e}")))?;
    let media_dir_arg = media_dir.to_string_lossy().replace('\\', "/");

    let quality_flag = format!("-{}", state.config.manim_quality);
    let res = state.config.manim_resolution.clone();
    let fps = state.config.manim_fps.to_string();

    let output = Command::new(&state.config.manim_bin)
        .current_dir(work_dir)
        .arg(file_name)
        .arg("StoryAnimation")
        .arg(&quality_flag)
        .arg("--resolution")
        .arg(&res)
        .arg("--fps")
        .arg(&fps)
        .arg("-o")
        .arg(format!("{stem}.mp4"))
        .arg("--media_dir")
        .arg(&media_dir_arg)
        .arg("--disable_caching")
        .output()
        .await
        .map_err(|e| AppError::Render(format!("failed to spawn manim: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Render(format!("manim failed: {stderr}")));
    }

    // Manim's output folder name varies by preset ("1920p1080", "1080p60", etc).
    let res_folder = res.replace(',', "p");
    let candidates = [
        media_dir
            .join("videos")
            .join(&stem)
            .join(&res_folder)
            .join(format!("{stem}.mp4")),
        media_dir
            .join("videos")
            .join(&stem)
            .join("1080p60")
            .join(format!("{stem}.mp4")),
        media_dir
            .join("videos")
            .join(&stem)
            .join("2160p60")
            .join(format!("{stem}.mp4")),
        media_dir
            .join("videos")
            .join(&stem)
            .join("480p15")
            .join(format!("{stem}.mp4")),
    ];

    let video_path = candidates
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| AppError::Render("manim reported success but output not found".into()))?;

    Ok(video_path.to_string_lossy().into_owned())
}

async fn svc_combine(
    state: &AppState,
    video_path: &str,
    audio_path: &str,
) -> AppResult<String> {
    let video = safe_storage_path(video_path)?;
    let audio = safe_storage_path(audio_path)?;
    let out = storage_join("output", &format!("final_{}.mp4", uuid::Uuid::new_v4()));

    let has_music = state
        .config
        .background_music
        .as_ref()
        .map(|p| p.exists())
        .unwrap_or(false);

    let mut cmd = Command::new(&state.config.ffmpeg_bin);
    cmd.arg("-y").arg("-i").arg(&video).arg("-i").arg(&audio);

    let filter: String;
    if has_music {
        let music = state.config.background_music.as_ref().unwrap();
        cmd.arg("-i").arg(music);
        filter = concat!(
            "[0:v]tpad=stop_mode=clone:stop_duration=999,",
            "eq=contrast=1.05:saturation=1.08,vignette=PI/5[v];",
            "[2:a]volume=0.18,aloop=loop=-1:size=2e9[music];",
            "[1:a]loudnorm=I=-16:TP=-1.5:LRA=11[voice];",
            "[music][voice]sidechaincompress=threshold=0.05:ratio=8:attack=5:release=250[ducked];",
            "[voice][ducked]amix=inputs=2:duration=first:dropout_transition=2[a]"
        )
        .to_string();
    } else {
        filter = concat!(
            "[0:v]tpad=stop_mode=clone:stop_duration=999,",
            "eq=contrast=1.05:saturation=1.08,vignette=PI/5[v];",
            "[1:a]loudnorm=I=-16:TP=-1.5:LRA=11[a]"
        )
        .to_string();
    }

    cmd.arg("-filter_complex")
        .arg(&filter)
        .arg("-map")
        .arg("[v]")
        .arg("-map")
        .arg("[a]")
        .arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg("slow")
        .arg("-crf")
        .arg("18")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-movflags")
        .arg("+faststart")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("192k")
        .arg("-shortest")
        .arg(&out);

    let output = cmd
        .output()
        .await
        .map_err(|e| AppError::Ffmpeg(format!("failed to spawn ffmpeg: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Ffmpeg(format!("ffmpeg failed: {stderr}")));
    }
    if !out.exists() {
        return Err(AppError::Ffmpeg(
            "ffmpeg reported success but output missing".into(),
        ));
    }

    Ok(out.to_string_lossy().into_owned())
}

async fn svc_probe_duration(state: &AppState, audio_path: &str) -> AppResult<f64> {
    let canonical = safe_storage_path(audio_path)?;

    let output = Command::new(&state.config.ffprobe_bin)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(&canonical)
        .output()
        .await
        .map_err(|e| AppError::Ffmpeg(format!("failed to spawn ffprobe: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Ffmpeg(format!("ffprobe failed: {stderr}")));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse::<f64>()
        .map_err(|e| AppError::Ffmpeg(format!("could not parse ffprobe duration: {e}")))
}

async fn concat_audio(state: &AppState, clips: &[PathBuf], out: &Path) -> AppResult<()> {
    if clips.is_empty() {
        return Err(AppError::Ffmpeg("no audio clips to concat".into()));
    }

    let list = storage_join("audio", &format!("list_{}.txt", uuid::Uuid::new_v4()));

    // Use absolute paths, forward-slashed, and single-quote-escaped so ffmpeg's
    // concat demuxer can find every clip regardless of cwd.
    let mut content = String::new();
    for c in clips {
        let abs = std::fs::canonicalize(c).map_err(|e| {
            AppError::Ffmpeg(format!("clip does not exist: {} ({e})", c.display()))
        })?;
        // ffmpeg concat demuxer expects forward slashes and escaped quotes.
        let escaped = abs
            .to_string_lossy()
            .replace('\\', "/")
            .replace('\'', "'\\''");
        content.push_str(&format!("file '{escaped}'\n"));
    }
    tokio::fs::write(&list, content).await?;

    // Resolve the list file to an absolute path *before* spawning ffmpeg.
    let list_abs = std::fs::canonicalize(&list)
        .map_err(|e| AppError::Ffmpeg(format!("concat list missing: {e}")))?;

    let output = Command::new(&state.config.ffmpeg_bin)
        .arg("-y")
        .arg("-f").arg("concat")
        .arg("-safe").arg("0")
        .arg("-i").arg(&list_abs)
        .arg("-c").arg("copy")
        .arg(out)
        .output()
        .await
        .map_err(|e| AppError::Ffmpeg(format!("concat spawn: {e}")))?;

    let _ = tokio::fs::remove_file(&list).await;

    if !output.status.success() {
        return Err(AppError::Ffmpeg(format!(
            "concat failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct StoryboardRequest {
    topic: String,
}

#[derive(Serialize)]
struct StoryboardResponse {
    storyboard: Storyboard,
}

#[derive(Deserialize)]
struct ScriptRequest {
    topic: String,
}

#[derive(Serialize)]
struct ScriptResponse {
    script: String,
}

#[derive(Deserialize)]
struct AudioRequest {
    script: String,
    voice_id: String,
}

#[derive(Serialize)]
struct AudioResponse {
    file_path: String,
}

#[derive(Deserialize)]
struct ManimRequest {
    script: String,
    #[serde(default = "default_duration")]
    audio_duration: f64,
}

fn default_duration() -> f64 {
    30.0
}

#[derive(Serialize)]
struct ManimResponse {
    file_path: String,
}

#[derive(Deserialize)]
struct RenderRequest {
    script_path: String,
}

#[derive(Serialize)]
struct RenderResponse {
    video_path: String,
}

#[derive(Deserialize)]
struct CombineRequest {
    video_path: String,
    audio_path: String,
}

#[derive(Serialize)]
struct CombineResponse {
    final_video_path: String,
}

#[derive(Deserialize)]
struct MasterRequest {
    topic: String,
    voice_id: String,
}

#[derive(Serialize)]
struct MasterResponse {
    final_video_path: String,
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

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
    let script = board
        .scenes
        .iter()
        .map(|s| s.narration.trim().to_string())
        .collect::<Vec<_>>()
        .join(" ");
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
    if payload.script.trim().is_empty() {
        return Err(AppError::BadRequest("script is empty".into()));
    }
    if !(1.0..=3600.0).contains(&payload.audio_duration) {
        return Err(AppError::BadRequest("audio_duration out of range".into()));
    }

    // Single-scene fallback: build a minimal storyboard from the raw script,
    // then reuse the same directed Manim prompt path.
    let board = Storyboard {
        title: "Untitled".into(),
        style: "minimalist-geometric".into(),
        scenes: vec![Scene {
            narration: payload.script.clone(),
            visual: "Interpret the narration visually with shapes, arrows, and \
                     transforms. Vary compositions; avoid static slides."
                .into(),
            mood: "curious".into(),
            palette: vec!["#0B1026".into(), "#FFB703".into(), "#FB8500".into()],
        }],
    };

    let code = {
        let prompt = build_manim_prompt(&board, &[payload.audio_duration], None)?;
        svc_request_manim_code(&state, &prompt).await?
    };

    let rel_path = storage_join("scripts", &format!("{}.py", uuid::Uuid::new_v4()));
    tokio::fs::write(&rel_path, code.as_bytes()).await?;
    Ok(Json(ManimResponse {
        file_path: rel_path.to_string_lossy().into_owned(),
    }))
}

async fn handle_render(
    State(state): State<AppState>,
    Json(payload): Json<RenderRequest>,
) -> AppResult<Json<RenderResponse>> {
    let video_path = svc_render(&state, &payload.script_path).await?;
    Ok(Json(RenderResponse { video_path }))
}

async fn handle_combine(
    State(state): State<AppState>,
    Json(payload): Json<CombineRequest>,
) -> AppResult<Json<CombineResponse>> {
    let final_video_path = svc_combine(&state, &payload.video_path, &payload.audio_path).await?;
    Ok(Json(CombineResponse { final_video_path }))
}

async fn handle_generate(
    State(state): State<AppState>,
    Json(payload): Json<MasterRequest>,
) -> AppResult<Json<MasterResponse>> {
    validate_topic(&payload.topic)?;
    validate_voice_id(&payload.voice_id)?;

    tracing::info!(topic = %payload.topic, "pipeline: storyboard");
    let board = svc_generate_storyboard(&state, &payload.topic).await?;
    tracing::info!(
        scenes = board.scenes.len(),
        style = %board.style,
        title = %board.title,
        "pipeline: storyboard ready"
    );

    tracing::info!("pipeline: per-scene audio");
    let mut clips: Vec<PathBuf> = Vec::with_capacity(board.scenes.len());
    let mut durations: Vec<f64> = Vec::with_capacity(board.scenes.len());
    for (i, scene) in board.scenes.iter().enumerate() {
        let p = svc_generate_audio(&state, &scene.narration, &payload.voice_id).await?;
        let d = svc_probe_duration(&state, &p).await?;
        tracing::info!(scene = i, duration_secs = d, "pipeline: scene audio ready");
        clips.push(PathBuf::from(p));
        durations.push(d);
    }

    tracing::info!("pipeline: concat audio");
    let full_audio = storage_join("audio", &format!("full_{}.mp3", uuid::Uuid::new_v4()));
    concat_audio(&state, &clips, &full_audio).await?;

    tracing::info!("pipeline: manim code + render");
    let video_path = svc_generate_manim_with_render(&state, &board, &durations).await?;

    tracing::info!("pipeline: combine");
    let final_path = svc_combine(
        &state,
        &video_path,
        full_audio.to_str().ok_or_else(|| {
            AppError::BadRequest("audio path contains non-UTF8 characters".into())
        })?,
    )
    .await?;

    tracing::info!(final_path = %final_path, "pipeline: complete");
    Ok(Json(MasterResponse {
        final_video_path: final_path,
    }))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(1);
        }
    };

    for dir in ["storage/audio", "storage/scripts", "storage/output"] {
        tokio::fs::create_dir_all(dir).await?;
    }

    let state = AppState {
        client: Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()?,
        config: Arc::new(config),
    };

    let app = Router::new()
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
    tracing::info!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}