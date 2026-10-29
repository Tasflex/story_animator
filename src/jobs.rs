//! In-memory job queue for the HTTP API. A 60s video takes minutes, so the
//! API accepts a job, returns its id immediately, and runs it in the
//! background with at most MAX_CONCURRENT_JOBS at once.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::{RwLock, Semaphore};

use crate::pipeline::{self, AppState, Progress, RunResult, VideoRequest};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: String,
    pub state: JobState,
    pub stage: String,
    pub progress: f32,
    pub message: String,
    pub topic: Option<String>,
    pub created_at: u64,
    pub finished_at: Option<u64>,
    pub error: Option<String>,
    pub result: Option<RunResult>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Clone)]
pub struct Jobs {
    inner: Arc<RwLock<HashMap<String, Job>>>,
    slots: Arc<Semaphore>,
}

impl Jobs {
    pub fn new(max_concurrent: usize) -> Self {
        Jobs { inner: Arc::default(), slots: Arc::new(Semaphore::new(max_concurrent.max(1))) }
    }

    pub async fn get(&self, id: &str) -> Option<Job> {
        self.inner.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<Job> {
        let mut v: Vec<Job> = self.inner.read().await.values().cloned().collect();
        v.sort_by_key(|j| std::cmp::Reverse(j.created_at));
        v
    }

    pub async fn submit(&self, state: AppState, req: VideoRequest) -> Job {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let job = Job {
            id: id.clone(),
            state: JobState::Queued,
            stage: "queued".into(),
            progress: 0.0,
            message: "waiting for a free render slot".into(),
            topic: req.topic.clone().or_else(|| req.storyboard.as_ref().map(|s| s.title.clone())),
            created_at: now(),
            finished_at: None,
            error: None,
            result: None,
        };
        self.inner.write().await.insert(id.clone(), job.clone());

        let jobs = self.clone();
        tokio::spawn(async move {
            let _permit = jobs.slots.clone().acquire_owned().await.expect("semaphore open");
            jobs.update(&id, |j| {
                j.state = JobState::Running;
                j.message = "starting".into();
            })
            .await;
            let j2 = jobs.clone();
            let id2 = id.clone();
            let progress: pipeline::ProgressFn = Arc::new(move |p: Progress| {
                let j3 = j2.clone();
                let id3 = id2.clone();
                tokio::spawn(async move {
                    j3.update(&id3, |j| {
                        j.stage = p.stage.to_string();
                        j.progress = j.progress.max(p.progress);
                        j.message = p.message;
                    })
                    .await;
                });
            });
            let res = pipeline::run(&state, &id, req, progress).await;
            jobs.update(&id, |j| {
                j.finished_at = Some(now());
                match res {
                    Ok(r) => {
                        j.state = JobState::Done;
                        j.stage = "done".into();
                        j.progress = 1.0;
                        j.message = format!("finished in {:.0}s", r.elapsed_secs);
                        j.result = Some(r);
                    }
                    Err(e) => {
                        tracing::error!(job = %j.id, error = %e, "job failed");
                        j.state = JobState::Failed;
                        j.error = Some(e.to_string());
                        j.message = "failed".into();
                    }
                }
            })
            .await;
        });
        job
    }

    async fn update(&self, id: &str, f: impl FnOnce(&mut Job)) {
        if let Some(j) = self.inner.write().await.get_mut(id) {
            f(j);
        }
    }
}
