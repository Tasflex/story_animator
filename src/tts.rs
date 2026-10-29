//! Narration: ElevenLabs text-to-speech *with timestamps*, plus an offline
//! "silent" provider that estimates word timings (for free previews).
//!
//! Word timings are what make the video orchestrated: they drive when each
//! visual appears, the character lip-flap, and the word-by-word captions.

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::Config;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Word {
    pub w: String,
    /// start / end in seconds
    pub s: f64,
    pub e: f64,
}

const API: &str = "https://api.elevenlabs.io/v1";

fn key(cfg: &Config) -> AppResult<&str> {
    cfg.elevenlabs_api_key
        .as_deref()
        .ok_or_else(|| AppError::Config("ELEVENLABS_API_KEY is not set (or use TTS_PROVIDER=silent)".into()))
}

/// Pick the voice: explicit request > ELEVENLABS_VOICE_ID > first premade voice on the account.
pub async fn resolve_voice(client: &Client, cfg: &Config, requested: Option<&str>) -> AppResult<String> {
    if let Some(v) = requested.or(cfg.elevenlabs_voice_id.as_deref()) {
        validate_voice_id(v)?;
        return Ok(v.to_string());
    }
    let resp = client.get(format!("{API}/voices")).header("xi-api-key", key(cfg)?).send().await?;
    let status = resp.status();
    let data: Value = resp.json().await?;
    if !status.is_success() {
        return Err(AppError::Tts(format!("list voices HTTP {status}: {data}")));
    }
    let voices = data["voices"].as_array().cloned().unwrap_or_default();
    let pick = voices
        .iter()
        .find(|v| v["category"].as_str() == Some("premade"))
        .or_else(|| voices.first())
        .and_then(|v| v["voice_id"].as_str())
        .ok_or_else(|| AppError::Tts("no voices available on this ElevenLabs account".into()))?;
    tracing::info!(voice_id = pick, "no voice configured; using account's first premade voice");
    Ok(pick.to_string())
}

pub fn validate_voice_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(AppError::BadRequest("invalid voice_id".into()));
    }
    Ok(())
}

/// Synthesize one scene. `prev`/`next` give ElevenLabs the surrounding text so
/// intonation flows naturally across scene boundaries.
pub async fn synthesize(
    client: &Client,
    cfg: &Config,
    voice_id: &str,
    text: &str,
    prev: Option<&str>,
    next: Option<&str>,
    out: &Path,
) -> AppResult<Vec<Word>> {
    let url = format!("{API}/text-to-speech/{voice_id}/with-timestamps?output_format=mp3_44100_128");
    // Audio-tag models (v3/v4) need room to act: lower stability, more style.
    let (stability, style) = if cfg.tts_supports_tags() { (0.4, 0.35) } else { (0.45, 0.25) };
    let mut body = json!({
        "text": text,
        "model_id": cfg.elevenlabs_model,
        "voice_settings": {
            "stability": stability,
            "similarity_boost": 0.8,
            "style": style,
            "use_speaker_boost": true,
            "speed": 1.0
        }
    });
    if let Some(p) = prev {
        body["previous_text"] = json!(p);
    }
    if let Some(n) = next {
        body["next_text"] = json!(n);
    }

    let mut backoff = Duration::from_millis(800);
    for attempt in 1..=5 {
        let resp = client.post(&url).header("xi-api-key", key(cfg)?).json(&body).send().await;
        let resp = match resp {
            Ok(r) => r,
            Err(e) if attempt < 5 => {
                tracing::warn!(attempt, error = %e, "elevenlabs network error, retrying");
                tokio::time::sleep(backoff).await;
                backoff *= 2;
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let status = resp.status();
        if (status.as_u16() == 429 || status.is_server_error()) && attempt < 5 {
            tracing::warn!(attempt, %status, "elevenlabs busy, backing off");
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(10));
            continue;
        }
        let text_body = resp.text().await?;
        if !status.is_success() {
            return Err(AppError::Tts(format!("HTTP {status}: {}", text_body.chars().take(500).collect::<String>())));
        }
        let data: Value = serde_json::from_str(&text_body)?;
        let audio = data["audio_base64"]
            .as_str()
            .ok_or_else(|| AppError::Tts("response missing audio_base64".into()))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(audio)
            .map_err(|e| AppError::Tts(format!("bad base64 audio: {e}")))?;
        if bytes.is_empty() {
            return Err(AppError::Tts("empty audio".into()));
        }
        tokio::fs::write(out, &bytes).await?;
        let align = if data["alignment"].is_object() { &data["alignment"] } else { &data["normalized_alignment"] };
        let words = words_from_alignment(align);
        if words.is_empty() {
            return Err(AppError::Tts("response had no alignment data".into()));
        }
        return Ok(words);
    }
    unreachable!("loop returns on final attempt")
}

/// Group character timings into words. Audio tags (`[excited]`) appear in the
/// alignment like text but are never spoken, so they are dropped.
fn words_from_alignment(a: &Value) -> Vec<Word> {
    let chars = a["characters"].as_array().cloned().unwrap_or_default();
    let starts = a["character_start_times_seconds"].as_array().cloned().unwrap_or_default();
    let ends = a["character_end_times_seconds"].as_array().cloned().unwrap_or_default();
    let mut words = Vec::new();
    let mut cur = String::new();
    let (mut ws, mut we) = (0.0, 0.0);
    let mut in_tag = 0usize;
    for i in 0..chars.len().min(starts.len()).min(ends.len()) {
        let ch = chars[i].as_str().unwrap_or("");
        let (s, e) = (starts[i].as_f64().unwrap_or(0.0), ends[i].as_f64().unwrap_or(0.0));
        if ch == "[" {
            in_tag += 1;
        }
        if in_tag > 0 {
            if ch == "]" {
                in_tag -= 1;
            }
            continue;
        }
        if ch.trim().is_empty() {
            if !cur.is_empty() {
                words.push(Word { w: std::mem::take(&mut cur), s: ws, e: we });
            }
            continue;
        }
        if cur.is_empty() {
            ws = s;
        }
        cur.push_str(ch);
        we = e;
    }
    if !cur.is_empty() {
        words.push(Word { w: cur, s: ws, e: we });
    }
    words
}

/// Offline estimate of word timings at a typical TTS pace (~2.6 words/s).
pub fn estimate_words(text: &str) -> (Vec<Word>, f64) {
    let mut t = 0.1;
    let mut out = Vec::new();
    for w in crate::storyboard::spoken(text).split_whitespace() {
        let letters = w.chars().filter(|c| c.is_alphanumeric()).count() as f64;
        let d = 0.1 + 0.05 * letters;
        out.push(Word { w: w.to_string(), s: t, e: t + d });
        t += d + 0.03;
        if w.ends_with(['.', '!', '?']) {
            t += 0.3;
        } else if w.ends_with([',', ';', ':']) {
            t += 0.12;
        }
    }
    (out, t + 0.15)
}

/// Generate an instrumental bed with ElevenLabs Music (best effort).
pub async fn generate_music(client: &Client, cfg: &Config, prompt: &str, secs: f64, out: &Path) -> AppResult<()> {
    let body = json!({
        "prompt": format!("{prompt}. Instrumental background score for a narrated explainer video; \
                           unobtrusive, steady energy, no vocals, clean ending."),
        "music_length_ms": ((secs + 2.0) * 1000.0).round() as u64,
        "force_instrumental": true
    });
    let resp = client
        .post(format!("{API}/music?output_format=mp3_44100_128"))
        .header("xi-api-key", key(cfg)?)
        .timeout(Duration::from_secs(300))
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        return Err(AppError::Tts(format!("music HTTP {status}: {}", t.chars().take(400).collect::<String>())));
    }
    let bytes = resp.bytes().await?;
    if bytes.len() < 1024 {
        return Err(AppError::Tts("music response too small".into()));
    }
    tokio::fs::write(out, &bytes).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_characters_into_words() {
        let a = json!({
            "characters": ["H","i",","," ","y","o","u","."],
            "character_start_times_seconds": [0.0,0.1,0.2,0.25,0.3,0.4,0.5,0.6],
            "character_end_times_seconds":   [0.1,0.2,0.25,0.3,0.4,0.5,0.6,0.7]
        });
        let w = words_from_alignment(&a);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], Word { w: "Hi,".into(), s: 0.0, e: 0.25 });
        assert_eq!(w[1].w, "you.");
        assert!((w[1].s - 0.3).abs() < 1e-9 && (w[1].e - 0.7).abs() < 1e-9);
    }

    #[test]
    fn drops_audio_tags_from_alignment() {
        let text = "[excited] Hi you";
        let chars: Vec<String> = text.chars().map(|c| c.to_string()).collect();
        let starts: Vec<f64> = (0..chars.len()).map(|i| i as f64 * 0.01).collect();
        let ends: Vec<f64> = starts.iter().map(|s| s + 0.01).collect();
        let w = words_from_alignment(&json!({
            "characters": chars, "character_start_times_seconds": starts, "character_end_times_seconds": ends
        }));
        assert_eq!(w.iter().map(|w| w.w.as_str()).collect::<Vec<_>>(), ["Hi", "you"]);
    }

    #[test]
    fn estimates_are_monotonic() {
        let (w, d) = estimate_words("One two, three. Four");
        assert_eq!(w.len(), 4);
        assert!(w.windows(2).all(|p| p[1].s > p[0].e));
        assert!(d > w[3].e);
    }
}
