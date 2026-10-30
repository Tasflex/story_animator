//! What earlier videos taught us: `storage/LESSONS.md`.
//!
//! After every video the self-review (review.rs) adds what it learned, and
//! feedback from the Telegram bot is added as the owner's lessons. An LLM
//! merges each addition into one short, de-duplicated list, so the file stays
//! readable instead of growing forever. The writer puts the list into every
//! storyboard prompt and the packager into every title/thumbnail prompt, so
//! each video is planned against everything the earlier ones got wrong.
//!
//! Next to the rules ("what went wrong, don't repeat it") the file keeps a
//! short "What works" list: strengths of the well-scoring videos, so the
//! writer repeats what landed and not only avoids what failed. The scorecard
//! at the bottom has one row per video; reviews and auto-improvement
//! (improve.rs) read it to judge each new video against the best one so far.
//!
//! The file is plain Markdown and safe to edit by hand.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::json;

use crate::config::Config;
use crate::error::AppResult;
use crate::llm;
use crate::storyboard::json_body;

const MAX_LESSONS: usize = 30;
const MAX_STRENGTHS: usize = 12;
/// Marks a lesson that came from the channel owner; the merge keeps these verbatim.
pub const OWNER: &str = "(owner)";
const LESSONS_HEADING: &str = "## Lessons";
const STRENGTHS_HEADING: &str = "## What works";
const SCORECARD_HEADING: &str = "## Scorecard";

pub fn path(cfg: &Config) -> PathBuf {
    cfg.storage.join("LESSONS.md")
}

struct Doc {
    lessons: Vec<String>,
    strengths: Vec<String>,
    scorecard: Vec<String>,
}

async fn read(cfg: &Config) -> Doc {
    let text = tokio::fs::read_to_string(path(cfg)).await.unwrap_or_default();
    let mut doc = Doc { lessons: Vec::new(), strengths: Vec::new(), scorecard: Vec::new() };
    let mut section = "";
    for line in text.lines() {
        if line.starts_with("## ") {
            section = line.trim();
            continue;
        }
        let item = line.trim().strip_prefix("- ").map(str::trim).filter(|l| !l.is_empty());
        if section == LESSONS_HEADING {
            if let Some(l) = item {
                doc.lessons.push(l.to_string());
            }
        } else if section == STRENGTHS_HEADING {
            if let Some(l) = item {
                doc.strengths.push(l.to_string());
            }
        } else if section == SCORECARD_HEADING && line.starts_with('|') && !line.starts_with("|---") && !line.starts_with("| Date") {
            doc.scorecard.push(line.to_string());
        }
    }
    doc
}

async fn write(cfg: &Config, doc: &Doc) -> AppResult<()> {
    let mut s = String::from(
        "# Lessons from earlier videos\n\n\
         Maintained by the self-review after every video (`story_animator review`), by auto-improvement \
         (`story_animator improve`) and by your feedback in the Telegram bot. The writer reads the Lessons and \
         What works lists before planning every video, and the packager before every title and thumbnail. \
         Lessons marked (owner) came from you and are always kept. The scorecard is how each new video is judged \
         against the best so far. Edit freely.\n\n",
    );
    for (heading, items) in [(LESSONS_HEADING, &doc.lessons), (STRENGTHS_HEADING, &doc.strengths)] {
        s.push_str(heading);
        s.push_str("\n\n");
        for l in items {
            s.push_str(&format!("- {l}\n"));
        }
        s.push('\n');
    }
    s.push_str(SCORECARD_HEADING);
    s.push_str("\n\n| Date | Run | Title | Score | Biggest problem |\n|---|---|---|---|---|\n");
    for row in &doc.scorecard {
        s.push_str(row);
        s.push('\n');
    }
    if let Some(dir) = path(cfg).parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    tokio::fs::write(path(cfg), s).await?;
    Ok(())
}

/// The lessons and what works, as a prompt section, or "" when there are none yet.
pub async fn prompt_block(cfg: &Config) -> String {
    let doc = read(cfg).await;
    let mut s = String::new();
    if !doc.lessons.is_empty() {
        let list: String = doc.lessons.iter().map(|l| format!("- {l}\n")).collect();
        s.push_str(&format!(
            "## Lessons from earlier videos (apply every one; (owner) lessons are the channel owner's orders)\n{list}"
        ));
    }
    if !doc.strengths.is_empty() {
        let list: String = doc.strengths.iter().map(|l| format!("- {l}\n")).collect();
        s.push_str(&format!("## What worked in the best videos so far (keep doing it)\n{list}"));
    }
    s
}

/// One scorecard row, parsed.
#[derive(Debug, Clone)]
pub struct Scored {
    pub run: String,
    pub title: String,
    pub score: f64,
    pub problem: String,
}

fn parse_row(row: &str) -> Option<Scored> {
    let cells: Vec<&str> = row.trim().trim_matches('|').split('|').map(str::trim).collect();
    if cells.len() < 5 {
        return None;
    }
    Some(Scored {
        run: cells[1].to_string(),
        title: cells[2].to_string(),
        score: cells[3].parse().ok()?,
        problem: cells[4].to_string(),
    })
}

fn run_key(run_id: &str) -> &str {
    &run_id[..run_id.len().min(8)]
}

/// Every other reviewed video, oldest first.
async fn others(cfg: &Config, run_id: &str) -> Vec<Scored> {
    read(cfg).await.scorecard.iter().filter_map(|r| parse_row(r)).filter(|s| s.run != run_key(run_id)).collect()
}

/// The best-scoring other video: the bar a new video has to clear.
pub async fn best_other(cfg: &Config, run_id: &str) -> Option<Scored> {
    others(cfg, run_id).await.into_iter().max_by(|a, b| a.score.total_cmp(&b.score))
}

/// The scorecard as a prompt section for the reviewer: the best video and the most recent ones.
pub async fn history_block(cfg: &Config, run_id: &str) -> String {
    let rows = others(cfg, run_id).await;
    if rows.is_empty() {
        return "## Earlier videos\nNone yet: this is the first reviewed video.".into();
    }
    let line = |s: &Scored| format!("- {:.1}/10 \"{}\" (biggest problem: {})", s.score, s.title, s.problem);
    let best = rows.iter().max_by(|a, b| a.score.total_cmp(&b.score)).map(line).unwrap_or_default();
    let recent: Vec<String> = rows.iter().rev().take(6).map(line).collect();
    format!("## Earlier videos\nBest so far:\n{best}\nMost recent:\n{}", recent.join("\n"))
}

/// Merge new lessons into the list (LLM-consolidated; plain append if the LLM is unavailable).
pub async fn add(client: &reqwest::Client, cfg: &Config, new: &[String]) -> AppResult<()> {
    let new: Vec<String> = new.iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    if new.is_empty() {
        return Ok(());
    }
    let mut doc = read(cfg).await;
    doc.lessons = match merge(client, cfg, &doc.lessons, &new).await {
        Ok(merged) => merged,
        Err(e) => {
            tracing::warn!(error = %e, "lesson merge failed; appending");
            let mut all = doc.lessons.clone();
            all.extend(new.into_iter().filter(|n| !doc.lessons.contains(n)));
            all
        }
    };
    write(cfg, &doc).await
}

/// Merge what a well-scoring video did right into "What works".
pub async fn add_strengths(client: &reqwest::Client, cfg: &Config, new: &[String]) -> AppResult<()> {
    let new: Vec<String> = new.iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    if new.is_empty() {
        return Ok(());
    }
    let mut doc = read(cfg).await;
    let rules = format!(
        "You keep the \"what works\" list of an animated YouTube explainer channel: what its best videos did \
         well, so later videos repeat it. Merge the new items into the current list and return JSON \
         {{\"lessons\": [\"...\"]}}.\n\
         - At most {MAX_STRENGTHS} items, most valuable first.\n\
         - Each one is a concrete, repeatable technique (what to do, and in a few words why it works), at most \
           25 words, general rather than about one topic.\n\
         - Merge duplicates and near-duplicates into one sharper item."
    );
    let mut merged = match merge_with(client, cfg, &rules, &doc.strengths, &new).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(error = %e, "strength merge failed; appending");
            let mut all = doc.strengths.clone();
            all.extend(new.into_iter().filter(|n| !doc.strengths.contains(n)));
            all
        }
    };
    merged.truncate(MAX_STRENGTHS);
    doc.strengths = merged;
    write(cfg, &doc).await
}

/// Record a video's score. A video reviewed again (after a revision or an
/// auto-improvement) replaces its earlier row instead of adding a second one.
pub async fn score(cfg: &Config, run_id: &str, title: &str, score: f64, problem: &str) -> AppResult<()> {
    let mut doc = read(cfg).await;
    let clean = |s: &str| s.replace('|', "/").replace('\n', " ");
    let key = run_key(run_id);
    let row = format!("| {} | {key} | {} | {score:.1} | {} |", today(), clean(title), clean(problem));
    match doc.scorecard.iter().position(|r| parse_row(r).is_some_and(|s| s.run == key)) {
        Some(i) => doc.scorecard[i] = row,
        None => doc.scorecard.push(row),
    }
    write(cfg, &doc).await
}

#[derive(Deserialize)]
struct Merged {
    lessons: Vec<String>,
}

async fn merge(client: &reqwest::Client, cfg: &Config, old: &[String], new: &[String]) -> AppResult<Vec<String>> {
    let system = format!(
        "You keep the lessons list of an animated YouTube explainer channel. Merge the new lessons into the \
         current list and return JSON {{\"lessons\": [\"...\"]}}.\n\
         - At most {MAX_LESSONS} lessons, most important first.\n\
         - Each one is a concrete, imperative rule for the NEXT video (what to do or avoid, and why in a few \
           words), at most 30 words. No lessons that only apply to one topic.\n\
         - Merge duplicates and near-duplicates into one sharper lesson. When a new lesson contradicts an old \
           one, the new one wins.\n\
         - Lessons starting with \"{OWNER}\" come from the channel owner: keep every one of them, word for word, \
           with the \"{OWNER}\" mark, and drop any other lesson that contradicts them."
    );
    let mut out = merge_with(client, cfg, &system, old, new).await?;
    // The owner's lessons survive even if the model drops them.
    for l in old.iter().chain(new).filter(|l| l.starts_with(OWNER)) {
        if !out.contains(l) {
            out.insert(0, l.clone());
        }
    }
    out.truncate(MAX_LESSONS.max(out.iter().filter(|l| l.starts_with(OWNER)).count()));
    Ok(out)
}

/// One LLM merge of `new` into `old` under `rules`; returns the merged list.
async fn merge_with(client: &reqwest::Client, cfg: &Config, rules: &str, old: &[String], new: &[String]) -> AppResult<Vec<String>> {
    if !cfg.has_llm() {
        return Err(crate::error::AppError::Config("no LLM key".into()));
    }
    let user = json!({ "current": old, "new": new }).to_string();
    let raw = llm::complete(client, cfg, rules, &user, true).await?;
    let merged: Merged = json_body(&raw)
        .and_then(|b| serde_json::from_str(b).map_err(|e| e.to_string()))
        .map_err(crate::error::AppError::Llm)?;
    Ok(merged.lessons.into_iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}

/// Today's date (UTC) as YYYY-MM-DD, without a date crate.
fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scorecard_rows_parse() {
        let s = parse_row("| 2026-10-30 | ab12cd34 | How Locks Work | 7.5 | scene 3 sparse |").unwrap();
        assert_eq!((s.run.as_str(), s.score, s.problem.as_str()), ("ab12cd34", 7.5, "scene 3 sparse"));
        assert!(parse_row("| Date | Run | Title | Score | Biggest problem |").is_none());
    }
}
