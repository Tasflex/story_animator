//! Parallel scene rendering via the Python engine.
//!
//! Each scene is an independent process, so one bad scene can't take down
//! the video: if a render fails, the scene is re-rendered once with the
//! always-safe `title` layout before giving up.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::stream::{self, StreamExt, TryStreamExt};
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::sync::Semaphore;

use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::media::run_cmd;

pub struct SceneRender {
    pub index: usize,
    pub spec: Value,
    pub spec_path: PathBuf,
    pub out: PathBuf,
}

/// Render all scenes; `on_done(done, total)` is called as each finishes.
pub async fn render_all(
    cfg: &Config,
    jobs: Vec<SceneRender>,
    on_done: Arc<dyn Fn(usize, usize) + Send + Sync>,
) -> AppResult<()> {
    let total = jobs.len();
    let sem = Arc::new(Semaphore::new(cfg.render_concurrency));
    let done = Arc::new(AtomicUsize::new(0));
    stream::iter(jobs.into_iter().map(|job| {
        let sem = sem.clone();
        let done = done.clone();
        let on_done = on_done.clone();
        async move {
            let _permit = sem.acquire().await.expect("semaphore open");
            render_one(cfg, &job).await?;
            let d = done.fetch_add(1, Ordering::SeqCst) + 1;
            on_done(d, total);
            Ok::<_, AppError>(())
        }
    }))
    .buffer_unordered(total.max(1))
    .try_collect::<Vec<_>>()
    .await?;
    Ok(())
}

async fn render_one(cfg: &Config, job: &SceneRender) -> AppResult<()> {
    let spec_text = serde_json::to_string_pretty(&job.spec)?;
    // Resume support: skip if this exact spec was already rendered.
    if job.out.exists()
        && let Ok(prev) = tokio::fs::read_to_string(&job.spec_path).await
            && prev == spec_text {
                tracing::info!(scene = job.index, "scene unchanged, reusing render");
                return Ok(());
            }
    tokio::fs::write(&job.spec_path, &spec_text).await?;
    let started = std::time::Instant::now();
    match engine_render(cfg, &job.spec_path, &job.out).await {
        Ok(()) => {
            tracing::info!(scene = job.index, secs = started.elapsed().as_secs_f32(), "scene rendered");
            Ok(())
        }
        Err(e) => {
            tracing::warn!(scene = job.index, error = %e, "scene render failed; retrying with safe layout");
            let fallback = fallback_spec(&job.spec);
            let fb_path = job.spec_path.with_extension("fallback.json");
            tokio::fs::write(&fb_path, serde_json::to_string_pretty(&fallback)?).await?;
            engine_render(cfg, &fb_path, &job.out)
                .await
                .map_err(|e2| AppError::Render(format!("scene {} failed twice: {e} / fallback: {e2}", job.index)))?;
            // Note the fallback so a resume doesn't think the original spec rendered.
            let _ = tokio::fs::write(&job.spec_path, "fallback").await;
            Ok(())
        }
    }
}

async fn engine_render(cfg: &Config, spec: &std::path::Path, out: &std::path::Path) -> AppResult<()> {
    let _ = tokio::fs::remove_file(out).await;
    run_cmd(
        Command::new(&cfg.python_bin)
            .args(["-m", "story_engine", "render", "--spec"])
            .arg(spec)
            .arg("--out")
            .arg(out)
            .current_dir(cfg.engine_dir())
            .env("PYTHONIOENCODING", "utf-8"),
        cfg.render_timeout_secs,
    )
    .await?;
    if !out.exists() {
        return Err(AppError::Render(format!("engine reported success but {} is missing", out.display())));
    }
    Ok(())
}

/// A spec whose scene uses its own `content.fallback` layout if it declared
/// one (and it isn't the layout that just failed), else a plain title card.
pub fn fallback_spec(spec: &Value) -> Value {
    let mut s = spec.clone();
    let scene = &spec["scene"];
    let fb = &scene["content"]["fallback"];
    if let Some(layout) = fb["layout"].as_str().filter(|l| Some(*l) != scene["layout"].as_str() && *l != "custom") {
        s["scene"]["layout"] = json!(layout);
        s["scene"]["content"] = if fb["content"].is_object() { fb["content"].clone() } else { json!({}) };
        if let Some(obj) = s.as_object_mut() {
            obj.remove("custom_code");
        }
        return s;
    }
    let heading = scene["title"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| scene["narration"].as_str().unwrap_or("").split_whitespace().take(5).collect::<Vec<_>>().join(" "));    s["scene"]["layout"] = json!("title");
    s["scene"]["title"] = Value::Null;
    s["scene"]["content"] = json!({ "heading": heading });
    if let Some(obj) = s.as_object_mut() {
        obj.remove("custom_code");
    }
    s
}
