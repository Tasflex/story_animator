//! HTTP API. See docs/API.md.

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::doctor;
use crate::error::{AppError, AppResult};
use crate::jobs::Jobs;
use crate::llm::{self, StoryRequest};
use crate::pipeline::{AppState, VideoRequest, validate_run_id};

#[derive(Clone)]
struct Ctx {
    app: AppState,
    jobs: Jobs,
}

pub fn router(app: AppState) -> Router {
    let jobs = Jobs::new(app.cfg.max_concurrent_jobs);
    Router::new()
        .route("/healthz", get(|| async { Json(json!({ "status": "ok" })) }))
        .route("/readyz", get(readyz))
        .route("/api/catalog", get(catalog))
        .route("/api/storyboard", post(storyboard))
        .route("/api/videos", post(create_video).get(list_videos))
        .route("/api/videos/:id", get(get_video))
        .route("/api/videos/:id/video", get(download_video))
        .route("/api/videos/:id/storyboard", get(get_storyboard))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(Ctx { app, jobs })
}

async fn readyz(State(c): State<Ctx>) -> impl IntoResponse {
    let report = doctor::run(&c.app.cfg).await;
    let ok = report.iter().all(|r| r.ok || !r.required);
    let status = if ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(json!({ "ready": ok, "checks": report })))
}

async fn catalog(State(c): State<Ctx>) -> Json<Value> {
    Json(json!({
        "layouts": c.app.catalog.layouts,
        "icons": c.app.catalog.icons,
        "backgrounds": c.app.catalog.backgrounds,
        "themes": c.app.catalog.themes.keys().collect::<Vec<_>>(),
        "cameras": c.app.catalog.cameras,
        "transitions": c.app.catalog.transitions,
    }))
}

#[derive(Deserialize)]
struct StoryboardReq {
    topic: String,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    audience: Option<String>,
    #[serde(default)]
    aspect: Option<String>,
    #[serde(default)]
    style: Option<String>,
}

async fn storyboard(State(c): State<Ctx>, Json(r): Json<StoryboardReq>) -> AppResult<Json<Value>> {
    if r.topic.trim().is_empty() {
        return Err(AppError::BadRequest("topic is empty".into()));
    }
    let portrait = r.aspect.as_deref().map(crate::config::Aspect::parse).transpose()?
        == Some(crate::config::Aspect::Portrait);
    let style = r.style.as_deref().map(crate::config::VisualStyle::parse).transpose()?.unwrap_or(c.app.cfg.style);
    let sb = llm::write_storyboard(
        &c.app.client,
        &c.app.cfg,
        &c.app.catalog,
        &StoryRequest {
            topic: &r.topic,
            theme: r.theme.as_deref(),
            audience: r.audience.as_deref(),
            portrait,
            style,
        },
    )
    .await?;
    Ok(Json(json!({ "storyboard": sb, "words": sb.total_words() })))
}

async fn create_video(State(c): State<Ctx>, Json(req): Json<VideoRequest>) -> AppResult<impl IntoResponse> {
    if req.storyboard.is_none() && req.topic.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err(AppError::BadRequest("provide `topic` or `storyboard`".into()));
    }
    if let Some(v) = &req.voice_id {
        crate::tts::validate_voice_id(v)?;
    }
    let job = c.jobs.submit(c.app.clone(), req).await;
    let url = format!("/api/videos/{}", job.id);
    Ok((StatusCode::ACCEPTED, Json(json!({ "id": job.id, "status_url": url, "job": job }))))
}

async fn list_videos(State(c): State<Ctx>) -> Json<Value> {
    Json(json!({ "jobs": c.jobs.list().await }))
}

async fn get_video(State(c): State<Ctx>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    validate_run_id(&id)?;
    let job = c.jobs.get(&id).await.ok_or_else(|| AppError::NotFound(format!("job {id}")))?;
    Ok(Json(json!(job)))
}

async fn download_video(State(c): State<Ctx>, Path(id): Path<String>) -> AppResult<Response> {
    validate_run_id(&id)?;
    let path = crate::pipeline::run_dir(&c.app.cfg, &id).join("final.mp4");
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| AppError::NotFound(format!("video for {id} (not finished?)")))?;
    let len = file.metadata().await?.len();
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(file));
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::CONTENT_LENGTH, len)
        .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"{id}.mp4\""))
        .body(body)
        .expect("valid response"))
}

async fn get_storyboard(State(c): State<Ctx>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    validate_run_id(&id)?;
    let path = crate::pipeline::run_dir(&c.app.cfg, &id).join("storyboard.json");
    let bytes = tokio::fs::read(&path).await.map_err(|_| AppError::NotFound(format!("storyboard for {id}")))?;
    Ok(Json(serde_json::from_slice(&bytes)?))
}
