//! Configuration, read once from the environment (and `.env`).
//!
//! Every knob has a sensible default; only the API keys for the providers
//! you actually use are required. See docs/CONFIGURATION.md.

use std::env;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aspect {
    Landscape,
    Portrait,
}

impl Aspect {
    pub fn parse(s: &str) -> AppResult<Self> {
        match s.trim().to_lowercase().as_str() {
            "landscape" | "16:9" | "wide" => Ok(Aspect::Landscape),
            "portrait" | "9:16" | "vertical" | "shorts" => Ok(Aspect::Portrait),
            o => Err(AppError::BadRequest(format!("aspect must be landscape|portrait, got {o:?}"))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    /// 540p / 15fps — fast previews
    Draft,
    /// 720p / 30fps
    Standard,
    /// 1080p / 30fps
    High,
    /// 1080p / 60fps
    Ultra,
}

impl Quality {
    pub fn parse(s: &str) -> AppResult<Self> {
        match s.trim().to_lowercase().as_str() {
            "draft" | "preview" => Ok(Quality::Draft),
            "standard" | "720p" => Ok(Quality::Standard),
            "high" | "1080p" => Ok(Quality::High),
            "ultra" | "1080p60" => Ok(Quality::Ultra),
            o => Err(AppError::BadRequest(format!("quality must be draft|standard|high|ultra, got {o:?}"))),
        }
    }

    /// (short side px, long side px, fps)
    pub fn dims(self) -> (u32, u32, u32) {
        match self {
            Quality::Draft => (540, 960, 15),
            Quality::Standard => (720, 1280, 30),
            Quality::High => (1080, 1920, 30),
            Quality::Ultra => (1080, 1920, 60),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LlmProvider {
    Auto,
    Gemini,
    DeepSeek,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtsProvider {
    ElevenLabs,
    Silent,
}

/// Overall visual language the storyboard writer aims for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualStyle {
    /// 3Blue1Brown-style: serif, equations, graphs, vectors, custom diagrams.
    Diagram,
    /// Flat cards, icons, characters, bold sans-serif.
    Infographic,
}

impl VisualStyle {
    pub fn parse(s: &str) -> AppResult<Self> {
        match s.trim().to_lowercase().as_str() {
            "diagram" | "3b1b" | "blueprint" => Ok(VisualStyle::Diagram),
            "infographic" | "flat" => Ok(VisualStyle::Infographic),
            o => Err(AppError::BadRequest(format!("style must be diagram|infographic, got {o:?}"))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MusicSource {
    None,
    File(PathBuf),
    /// Generate a track per video with ElevenLabs Music.
    ElevenLabs,
    /// Pick a track per video from a folder (assets/music by default);
    /// `story_animator music` fills it once.
    Library(PathBuf),
}

#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    pub storage: PathBuf,

    pub llm_provider: LlmProvider,
    pub gemini_api_key: Option<String>,
    pub gemini_model: String,
    pub deepseek_api_key: Option<String>,
    pub deepseek_model: String,

    pub tts_provider: TtsProvider,
    pub elevenlabs_api_key: Option<String>,
    pub elevenlabs_model: String,
    pub elevenlabs_voice_id: Option<String>,
    pub tts_concurrency: usize,

    pub music: MusicSource,
    pub music_volume: f64,

    pub target_duration: f64,
    /// Also cut a vertical short from every landscape video.
    pub shorts: bool,
    /// Upper bound for the short's length (YouTube Shorts: under 60s).
    pub short_max_secs: f64,
    /// Topics scoring below this niche fit (1-10) are refused unless forced.
    pub min_topic_fit: u32,
    pub transition_secs: f64,
    pub max_tempo: f64,
    pub aspect: Aspect,
    pub quality: Quality,

    pub python_bin: String,
    pub render_concurrency: usize,
    pub render_timeout_secs: u64,
    pub ffmpeg_bin: String,
    pub ffprobe_bin: String,

    pub burn_subtitles: bool,

    pub style: VisualStyle,
    /// Allow LLM-written one-off scenes (`custom` layout).
    pub custom_scenes: bool,
    /// Code-writing attempts per custom scene before using its fallback.
    pub custom_attempts: u32,
    pub subtitle_font: String,

    pub server_addr: String,
    pub max_concurrent_jobs: usize,
    pub log_json: bool,
}

/// Read a variable, treating empty strings as unset and stripping inline
/// `# comments` (older .env files in this project used them).
fn var(name: &str) -> Option<String> {
    let raw = env::var(name).ok()?;
    let v = match raw.find(" #").or_else(|| raw.find("\t#")) {
        Some(i) => &raw[..i],
        None => raw.as_str(),
    };
    let v = v.trim().trim_matches('"').trim();
    (!v.is_empty()).then(|| v.to_string())
}

fn var_or(name: &str, default: &str) -> String {
    var(name).unwrap_or_else(|| default.to_string())
}

fn parse<T: std::str::FromStr>(name: &str, default: T) -> AppResult<T>
where
    T::Err: std::fmt::Display,
{
    match var(name) {
        None => Ok(default),
        Some(v) => v.parse::<T>().map_err(|e| AppError::Config(format!("invalid {name}={v:?}: {e}"))),
    }
}

fn flag(name: &str, default: bool) -> bool {
    match var(name).as_deref().map(str::to_lowercase).as_deref() {
        Some("1" | "true" | "yes" | "on") => true,
        Some("0" | "false" | "no" | "off") => false,
        _ => default,
    }
}

fn default_python(root: &Path) -> String {
    for rel in ["venv/Scripts/python.exe", "venv/bin/python", ".venv/Scripts/python.exe", ".venv/bin/python"] {
        let p = root.join(rel);
        if p.exists() {
            return p.to_string_lossy().into_owned();
        }
    }
    if cfg!(windows) { "python".into() } else { "python3".into() }
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        let _ = dotenvy::dotenv();
        let root = env::current_dir()?;
        let storage = root.join(var_or("STORAGE_DIR", "storage"));

        let llm_provider = match var_or("LLM_PROVIDER", "auto").to_lowercase().as_str() {
            "auto" => LlmProvider::Auto,
            "gemini" => LlmProvider::Gemini,
            "deepseek" => LlmProvider::DeepSeek,
            o => return Err(AppError::Config(format!("LLM_PROVIDER must be auto|gemini|deepseek, got {o:?}"))),
        };
        let tts_provider = match var_or("TTS_PROVIDER", "elevenlabs").to_lowercase().as_str() {
            "elevenlabs" => TtsProvider::ElevenLabs,
            "silent" | "none" => TtsProvider::Silent,
            o => return Err(AppError::Config(format!("TTS_PROVIDER must be elevenlabs|silent, got {o:?}"))),
        };
        let music = match var("BACKGROUND_MUSIC") {
            None => MusicSource::Library(root.join("assets").join("music")),
            Some(v) if v.eq_ignore_ascii_case("none") => MusicSource::None,
            Some(v) if v.eq_ignore_ascii_case("elevenlabs") || v.eq_ignore_ascii_case("generate") => {
                MusicSource::ElevenLabs
            }
            Some(v) if v.eq_ignore_ascii_case("library") => MusicSource::Library(root.join("assets").join("music")),
            Some(v) if root.join(&v).is_dir() => MusicSource::Library(root.join(&v)),
            Some(v) => {
                let p = root.join(&v);
                if !p.exists() {
                    return Err(AppError::Config(format!("BACKGROUND_MUSIC file not found: {}", p.display())));
                }
                MusicSource::File(p)
            }
        };

        let cfg = Config {
            python_bin: var("PYTHON_BIN").unwrap_or_else(|| default_python(&root)),
            root,
            storage,
            llm_provider,
            gemini_api_key: var("GEMINI_API_KEY"),
            gemini_model: var_or("GEMINI_MODEL", "gemini-2.5-flash"),
            deepseek_api_key: var("DEEPSEEK_API_KEY"),
            deepseek_model: var_or("DEEPSEEK_MODEL", "deepseek-chat"),
            tts_provider,
            elevenlabs_api_key: var("ELEVENLABS_API_KEY"),
            elevenlabs_model: var_or("ELEVENLABS_MODEL", "eleven_v4"),
            elevenlabs_voice_id: var("ELEVENLABS_VOICE_ID"),
            tts_concurrency: parse("TTS_CONCURRENCY", 3usize)?.max(1),
            music,
            music_volume: parse("MUSIC_VOLUME", 0.07f64)?,
            target_duration: parse("TARGET_DURATION_SECS", 300.0f64)?,
            shorts: flag("SHORTS", true),
            short_max_secs: parse("SHORT_MAX_SECS", 59.0f64)?,
            min_topic_fit: parse("MIN_TOPIC_FIT", 6u32)?.min(10),
            transition_secs: parse("TRANSITION_SECS", 0.5f64)?,
            max_tempo: parse("MAX_SPEECH_TEMPO", 1.15f64)?,
            aspect: Aspect::parse(&var_or("ASPECT", "landscape"))?,
            quality: Quality::parse(&var_or("RENDER_QUALITY", "high"))?,
            render_concurrency: parse(
                "RENDER_CONCURRENCY",
                std::thread::available_parallelism().map(|n| (n.get() / 2).clamp(1, 4)).unwrap_or(2),
            )?
            .max(1),
            render_timeout_secs: parse("RENDER_TIMEOUT_SECS", 900u64)?,
            ffmpeg_bin: var_or("FFMPEG_BIN", "ffmpeg"),
            ffprobe_bin: var_or("FFPROBE_BIN", "ffprobe"),
            burn_subtitles: flag("BURN_SUBTITLES", true),
            style: VisualStyle::parse(&var_or("VISUAL_STYLE", "diagram"))?,
            custom_scenes: flag("CUSTOM_SCENES", true),
            custom_attempts: parse("CUSTOM_ATTEMPTS", 3u32)?.clamp(1, 6),
            subtitle_font: var_or("SUBTITLE_FONT", "Poppins"),
            server_addr: var_or("SERVER_ADDR", "0.0.0.0:3000"),
            max_concurrent_jobs: parse("MAX_CONCURRENT_JOBS", 1usize)?.max(1),
            log_json: flag("LOG_JSON", false),
        };
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> AppResult<()> {
        if !(15.0..=600.0).contains(&self.target_duration) {
            return Err(AppError::Config("TARGET_DURATION_SECS must be between 15 and 600".into()));
        }
        if !(0.0..=1.5).contains(&self.transition_secs) {
            return Err(AppError::Config("TRANSITION_SECS must be between 0 and 1.5".into()));
        }
        if !(1.0..=1.3).contains(&self.max_tempo) {
            return Err(AppError::Config("MAX_SPEECH_TEMPO must be between 1.0 and 1.3".into()));
        }
        if !(15.0..=60.0).contains(&self.short_max_secs) {
            return Err(AppError::Config("SHORT_MAX_SECS must be between 15 and 60".into()));
        }
        if !(0.0..=1.0).contains(&self.music_volume) {
            return Err(AppError::Config("MUSIC_VOLUME must be between 0 and 1".into()));
        }
        Ok(())
    }

    /// Whether the TTS model understands inline audio tags like `[excited]`.
    pub fn tts_supports_tags(&self) -> bool {
        let m = self.elevenlabs_model.as_str();
        m.starts_with("eleven_v3") || m.starts_with("eleven_v4") || m.starts_with("eleven_v5")
    }

    pub fn has_llm(&self) -> bool {
        self.gemini_api_key.is_some() || self.deepseek_api_key.is_some()
    }

    pub fn engine_dir(&self) -> PathBuf {
        self.root.join("engine")
    }

    pub fn fonts_dir(&self) -> PathBuf {
        self.root.join("assets").join("fonts")
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.storage.join("runs")
    }
}
