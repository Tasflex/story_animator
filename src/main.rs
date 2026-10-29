//! Story Animator — topic in, a five-minute 2D animated explainer (16:9) and
//! a vertical short (9:16) out.
//!
//! Architecture (see docs/ARCHITECTURE.md):
//!   niche      what the channel covers; topic ideas and the fit check
//!   llm        writes a chaptered JSON storyboard from the engine's vocabulary
//!   tts        narrates each scene with word-level timestamps (audio tags for emotion)
//!   timeline   fits scenes into exactly TARGET_DURATION_SECS
//!   render     renders scenes in parallel with the Python engine (engine/)
//!   media      crossfades, mixes audio, burns captions, cuts the short with ffmpeg

mod catalog;
mod config;
mod custom;
mod doctor;
mod error;
mod jobs;
mod llm;
mod media;
mod niche;
mod pipeline;
mod render;
mod server;
mod storyboard;
mod subtitles;
mod timeline;
mod tts;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use crate::catalog::Catalog;
use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::pipeline::{AppState, Progress, VideoRequest};
use crate::storyboard::Storyboard;

#[derive(Parser)]
#[command(name = "story_animator", version, about = "Topic in: a five-minute animated explainer plus a vertical short out.")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a video from a topic (or from a storyboard file).
    Generate(GenerateArgs),
    /// Only write the storyboard (JSON) so you can review/edit it first.
    Storyboard {
        /// What the video is about.
        topic: String,
        /// Where to write the JSON (default: stdout).
        #[arg(long, short)]
        out: Option<PathBuf>,
        #[arg(long)]
        theme: Option<String>,
        #[arg(long)]
        audience: Option<String>,
        #[arg(long)]
        portrait: bool,
        /// diagram (3Blue1Brown-like, default) | infographic (flat icons)
        #[arg(long)]
        style: Option<String>,
        /// Write it even if the topic doesn't fit the channel niche.
        #[arg(long)]
        force: bool,
    },
    /// Suggest video topics that fit the channel niche.
    Topics {
        /// How many ideas.
        #[arg(long, short, default_value_t = 10)]
        count: usize,
        /// Narrow to an area, e.g. "bridges and tunnels".
        #[arg(long)]
        area: Option<String>,
    },
    /// Fill the background-music library (assets/music) with ElevenLabs Music,
    /// once; every video then reuses a track from it.
    Music {
        /// Tracks to have in the library.
        #[arg(long, short, default_value_t = 3)]
        count: usize,
    },
    /// Re-run an earlier run, reusing everything that hasn't changed
    /// (edit storage/runs/<id>/storyboard.json first to tweak it).
    Resume {
        run_id: String,
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// Start the HTTP API server.
    Serve {
        #[arg(long)]
        addr: Option<String>,
    },
    /// Check that Python, Manim, ffmpeg, fonts and API keys are ready.
    Doctor,
    /// Print the visual vocabulary (layouts, icons, themes...) as JSON.
    Catalog,
}

#[derive(Args)]
struct GenerateArgs {
    /// What the video is about.
    topic: Option<String>,
    /// Render a storyboard JSON file instead of asking the LLM.
    #[arg(long, short)]
    storyboard: Option<PathBuf>,
    /// ElevenLabs voice id (default: ELEVENLABS_VOICE_ID or the account's first voice).
    #[arg(long)]
    voice: Option<String>,
    /// landscape (16:9) or portrait (9:16).
    #[arg(long)]
    aspect: Option<String>,
    /// draft | standard | high | ultra
    #[arg(long, short)]
    quality: Option<String>,
    /// Force a colour theme (see `catalog`).
    #[arg(long)]
    theme: Option<String>,
    /// Who the video is for, e.g. "curious 10-year-olds".
    #[arg(long)]
    audience: Option<String>,
    /// Silent narration with estimated timings — free, for previews.
    #[arg(long)]
    silent: bool,
    /// Disable background music for this video.
    #[arg(long)]
    no_music: bool,
    /// diagram (3Blue1Brown-like, default) | infographic (flat icons and characters).
    #[arg(long)]
    style: Option<String>,
    /// Make it even if the topic doesn't fit the channel niche.
    #[arg(long)]
    force: bool,
    /// Copy the finished video here (the short goes next to it as <name>_short.mp4).
    #[arg(long, short)]
    out: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(2);
        }
    };
    init_tracing(&cfg);
    if let Err(e) = dispatch(cli, cfg).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn init_tracing(cfg: &Config) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,story_animator=info,tower_http=warn".into());
    if cfg.log_json {
        tracing_subscriber::fmt().with_env_filter(filter).json().init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).with_target(false).init();
    }
}

async fn state(cfg: Config) -> AppResult<AppState> {
    tokio::fs::create_dir_all(cfg.runs_dir()).await?;
    let catalog = Catalog::load(&cfg).await?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(300))
        .build()?;
    Ok(AppState { client, cfg: Arc::new(cfg), catalog: Arc::new(catalog) })
}

fn cli_progress() -> pipeline::ProgressFn {
    // tracing already logs every progress event; nothing extra to print.
    Arc::new(|_p: Progress| {})
}

async fn finish(result: pipeline::RunResult, out: Option<PathBuf>) -> AppResult<()> {
    let mut path = result.video.clone();
    let mut short = result.short.clone();
    if let Some(o) = out {
        if let Some(parent) = o.parent().filter(|p| !p.as_os_str().is_empty()) {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&result.video, &o).await?;
        if let Some(s) = &result.short {
            let stem = o.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "video".into());
            let so = o.with_file_name(format!("{stem}_short.mp4"));
            tokio::fs::copy(s, &so).await?;
            short = Some(so);
        }
        path = o;
    }
    let short_line = match (&short, result.short_duration) {
        (Some(p), Some(d)) => format!("\n  short:    {}  ({d:.1}s, 1080x1920)", p.display()),
        _ => String::new(),
    };
    let yt_line = result.youtube.as_ref().map(|p| format!("\n  youtube:  {}", p.display())).unwrap_or_default();
    println!(
        "\n  \"{}\"\n  video:    {}\n  length:   {:.2}s  ({} scenes, {}x{} @ {}fps, speech {:.2}x){short_line}{yt_line}\n  run:      {}  (resume: story_animator resume {})\n  took:     {:.0}s",
        result.title,
        path.display(),
        result.duration,
        result.scenes,
        result.width,
        result.height,
        result.fps,
        result.tempo,
        result.run_dir.display(),
        result.run_id,
        result.elapsed_secs
    );
    Ok(())
}

/// Briefs for the music library: unobtrusive beds that suit engineering explainers.
const MUSIC_BRIEFS: [(&str, &str); 4] = [
    ("wonder", "wonder-filled documentary underscore, soft felt piano and warm strings, gentle pulse, 85 bpm, curious and hopeful"),
    ("circuit", "modern minimal electronic underscore, soft synth arpeggios, light glitchy percussion, 100 bpm, sleek technology feel"),
    ("scale", "airy cinematic ambient underscore, wide pads, subtle plucked motifs, slow build, 75 bpm, awe at enormous structures"),
    ("workshop", "playful inventive underscore, pizzicato strings, marimba and light claps, 95 bpm, clever engineering tricks"),
];

async fn dispatch(cli: Cli, cfg: Config) -> AppResult<()> {
    match cli.cmd {
        Cmd::Doctor => {
            let report = doctor::run(&cfg).await;
            let mut fatal = false;
            for c in &report {
                let mark = if c.ok { "ok  " } else if c.required { "FAIL" } else { "warn" };
                fatal |= !c.ok && c.required;
                println!("[{mark}] {:<8} {}", c.name, c.detail);
            }
            if fatal {
                return Err(AppError::Config("environment not ready".into()));
            }
            Ok(())
        }
        Cmd::Catalog => {
            let st = state(cfg).await?;
            println!("{}", serde_json::to_string_pretty(&*st.catalog.layouts.keys().collect::<Vec<_>>())?);
            println!("engine: v{}", st.catalog.version);
            println!("icons: {}", st.catalog.icons.join(", "));
            println!("themes: {}", st.catalog.themes.keys().cloned().collect::<Vec<_>>().join(", "));
            println!("backgrounds: {}", st.catalog.backgrounds.keys().cloned().collect::<Vec<_>>().join(", "));
            Ok(())
        }
        Cmd::Topics { count, area } => {
            let client = reqwest::Client::builder().timeout(Duration::from_secs(300)).build()?;
            let ideas = niche::suggest_topics(&client, &cfg, count, area.as_deref()).await?;
            for (i, t) in ideas.iter().enumerate() {
                println!("\n{:>2}. {}\n    hook: {}\n    why:  {}", i + 1, t.topic, t.hook, t.why);
                for w in &t.wow {
                    println!("    wow:  {w}");
                }
            }
            println!("\nMake one:  story_animator generate \"<topic>\"");
            Ok(())
        }
        Cmd::Music { count } => {
            let lib = match &cfg.music {
                config::MusicSource::Library(p) => p.clone(),
                _ => cfg.root.join("assets").join("music"),
            };
            tokio::fs::create_dir_all(&lib).await?;
            let client = reqwest::Client::builder().timeout(Duration::from_secs(600)).build()?;
            let secs = cfg.target_duration + 4.0;
            for (k, (name, brief)) in MUSIC_BRIEFS.iter().take(count.clamp(1, MUSIC_BRIEFS.len())).enumerate() {
                let out = lib.join(format!("{:02}_{name}.mp3", k + 1));
                if out.exists() {
                    println!("have     {}", out.display());
                    continue;
                }
                println!("making   {} ({secs:.0}s) ...", out.display());
                tts::generate_music(&client, &cfg, brief, secs, &out).await?;
            }
            println!("music library: {} track(s) in {}", pipeline::music_tracks(&lib).await.len(), lib.display());
            Ok(())
        }
        Cmd::Storyboard { topic, out, theme, audience, portrait, style, force } => {
            let style = style.as_deref().map(config::VisualStyle::parse).transpose()?.unwrap_or(cfg.style);
            let st = state(cfg).await?;
            let sb = llm::write_storyboard(
                &st.client,
                &st.cfg,
                &st.catalog,
                &llm::StoryRequest {
                    topic: &topic,
                    theme: theme.as_deref(),
                    audience: audience.as_deref(),
                    portrait,
                    style,
                    word_scale: 1.0,
                    force,
                },
            )
            .await?;
            let text = serde_json::to_string_pretty(&sb)?;
            match out {
                Some(p) => {
                    tokio::fs::write(&p, text).await?;
                    eprintln!("wrote {} ({} scenes, {} words)", p.display(), sb.scenes.len(), sb.total_words());
                }
                None => println!("{text}"),
            }
            Ok(())
        }
        Cmd::Generate(a) => {
            let storyboard = match &a.storyboard {
                Some(p) => {
                    let text = tokio::fs::read_to_string(p).await?;
                    Some(
                        serde_json::from_str::<Storyboard>(&text)
                            .map_err(|e| AppError::Storyboard(format!("{}: {e}", p.display())))?,
                    )
                }
                None => None,
            };
            if storyboard.is_none() && a.topic.is_none() {
                return Err(AppError::BadRequest("give a topic, or --storyboard FILE".into()));
            }
            let req = VideoRequest {
                topic: a.topic,
                storyboard,
                voice_id: a.voice,
                aspect: a.aspect,
                quality: a.quality,
                theme: a.theme,
                audience: a.audience,
                silent: a.silent.then_some(true),
                music: a.no_music.then_some(false),
                style: a.style,
                force: a.force.then_some(true),
            };
            let st = state(cfg).await?;
            let run_id = uuid::Uuid::new_v4().simple().to_string();
            let result = pipeline::run(&st, &run_id, req, cli_progress()).await?;
            finish(result, a.out).await
        }
        Cmd::Resume { run_id, out } => {
            pipeline::validate_run_id(&run_id)?;
            let st = state(cfg).await?;
            let dir = pipeline::run_dir(&st.cfg, &run_id);
            let req: VideoRequest = serde_json::from_slice(
                &tokio::fs::read(dir.join("request.json"))
                    .await
                    .map_err(|_| AppError::NotFound(format!("run {run_id} in {}", dir.display())))?,
            )?;
            let result = pipeline::run(&st, &run_id, req, cli_progress()).await?;
            finish(result, out).await
        }
        Cmd::Serve { addr } => {
            let addr = addr.unwrap_or_else(|| cfg.server_addr.clone());
            let st = state(cfg).await?;
            let app = server::router(st);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            tracing::info!(%addr, "listening — POST /api/videos to make a video");
            axum::serve(listener, app).await?;
            Ok(())
        }
    }
}
