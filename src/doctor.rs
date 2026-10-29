//! Environment self-test (`story_animator doctor`, `GET /readyz`).

use serde::Serialize;
use tokio::process::Command;

use crate::config::{Config, MusicSource, TtsProvider};
use crate::media::run_cmd;

#[derive(Debug, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub required: bool,
    pub detail: String,
}

fn check(name: &'static str, ok: bool, required: bool, detail: impl Into<String>) -> Check {
    Check { name, ok, required, detail: detail.into() }
}

pub async fn run(cfg: &Config) -> Vec<Check> {
    let mut out = Vec::new();

    match run_cmd(
        Command::new(&cfg.python_bin).args(["-m", "story_engine", "check"]).current_dir(cfg.engine_dir()),
        120,
    )
    .await
    {
        Ok(s) => out.push(check("engine", s.contains("\"ok\": true"), true, s.trim().to_string())),
        Err(e) => out.push(check("engine", false, true, format!("{} — run: pip install -r engine/requirements.txt ({e})", cfg.python_bin))),
    }

    match run_cmd(Command::new(&cfg.ffmpeg_bin).arg("-hide_banner").arg("-filters"), 30).await {
        Ok(s) => {
            let need = ["xfade", "subtitles", "loudnorm", "sidechaincompress", "amix"];
            let missing: Vec<&str> = need.iter().copied().filter(|f| !s.contains(&format!(" {f} "))).collect();
            out.push(check(
                "ffmpeg",
                missing.is_empty(),
                true,
                if missing.is_empty() { "all required filters present".into() } else { format!("missing filters: {missing:?} (need a full ffmpeg build with libass)") },
            ));
        }
        Err(e) => out.push(check("ffmpeg", false, true, format!("{}: {e}", cfg.ffmpeg_bin))),
    }
    match run_cmd(Command::new(&cfg.ffprobe_bin).arg("-version"), 30).await {
        Ok(_) => out.push(check("ffprobe", true, true, "ok")),
        Err(e) => out.push(check("ffprobe", false, true, e.to_string())),
    }

    let fonts = cfg.fonts_dir();
    let has_font = std::fs::read_dir(&fonts)
        .map(|rd| rd.flatten().any(|e| e.path().extension().is_some_and(|x| x == "ttf")))
        .unwrap_or(false);
    out.push(check("fonts", has_font, false, fonts.display().to_string()));

    out.push(check(
        "llm",
        cfg.has_llm(),
        false,
        match (&cfg.gemini_api_key, &cfg.deepseek_api_key) {
            (Some(_), Some(_)) => format!("gemini ({}) + deepseek fallback", cfg.gemini_model),
            (Some(_), None) => format!("gemini ({})", cfg.gemini_model),
            (None, Some(_)) => "deepseek".into(),
            (None, None) => "no key — only pre-written storyboards can be rendered".into(),
        },
    ));
    let tts_ok = cfg.tts_provider == TtsProvider::Silent || cfg.elevenlabs_api_key.is_some();
    out.push(check(
        "tts",
        tts_ok,
        false,
        match cfg.tts_provider {
            TtsProvider::Silent => "silent (estimated timings)".to_string(),
            TtsProvider::ElevenLabs if tts_ok => format!(
                "elevenlabs ({}), voice {}",
                cfg.elevenlabs_model,
                cfg.elevenlabs_voice_id.as_deref().unwrap_or("auto")
            ),
            TtsProvider::ElevenLabs => "ELEVENLABS_API_KEY missing — use --silent for previews".into(),
        },
    ));
    out.push(check(
        "music",
        true,
        false,
        match &cfg.music {
            MusicSource::None => "none".to_string(),
            MusicSource::File(p) => p.display().to_string(),
            MusicSource::ElevenLabs => "generated per video (ElevenLabs Music)".into(),
        },
    ));
    out
}
