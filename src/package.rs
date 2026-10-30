//! Packaging a finished run for YouTube: title, description, tags and thumbnail,
//! written against docs/PACKAGING.md and the lessons file.
//!
//! The LLM sees the contact sheet (every scene's frame) and picks the
//! thumbnail: which frame, the crop around its subject, the words, the pointer
//! and the backdrop. The engine renders it; the LLM then looks at the result,
//! scores it against the rules and, if it falls short, revises the spec once.
//!
//! Writes `<run>/thumbnail.png`, `<run>/package.json` and the final `<run>/youtube.txt`.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::process::Command;

use crate::error::{AppError, AppResult};
use crate::pipeline::{self, AppState};
use crate::review;
use crate::storyboard::{Storyboard, json_body};
use crate::timeline::Timeline;
use crate::{lessons, llm, media};

pub const CHANNEL_LINE: &str = "New to Blueprinted? We explain how the world's machines and megastructures actually \
                                work, with animated diagrams. Subscribe for the next one.";
const BACKDROPS: [&str; 3] = ["datacenter", "ocean", "rays"];
const MAX_TAG_CHARS: usize = 480;
/// Attempts are repeated until the thumbnail scores this (1-10) with no rule broken.
const GOOD_THUMBNAIL: f64 = 7.0;
const MAX_ROUNDS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thumb {
    pub scene: usize,
    #[serde(default = "default_at")]
    pub at: f64,
    pub lines: Vec<String>,
    #[serde(default)]
    pub highlight: Option<String>,
    pub crop: [f64; 4],
    #[serde(default = "default_art")]
    pub art: String,
    #[serde(default)]
    pub pointer: Option<[f64; 2]>,
    #[serde(default)]
    pub backdrop: Option<String>,
}

fn default_at() -> f64 {
    0.75
}
fn default_art() -> String {
    "right".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub title: String,
    /// First two lines of the description (shown above "...more").
    pub hook: String,
    /// What the viewer will see, one short paragraph.
    pub what_you_see: String,
    #[serde(default)]
    pub hashtags: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub thumbnail: Thumb,
    #[serde(default)]
    pub thumbnail_score: Option<f64>,
    #[serde(default)]
    pub thumbnail_problems: Vec<String>,
}

#[derive(Deserialize)]
struct Check {
    score: f64,
    #[serde(default)]
    problems: Vec<String>,
}

/// PACKAGING.md rules that code can check without looking at the image. (Whether the
/// thumbnail restates the title is judged by the vision check: sharing the subject's
/// name is fine, repeating the claim is not, and only meaning tells them apart.)
fn rule_problems(p: &Package, script: &str) -> Vec<String> {
    let thumb_text = p.thumbnail.lines.join(" ");
    let mut out = Vec::new();
    // Every number must come from the script (digits there; commas ignored).
    let digits = |s: &str| -> Vec<String> {
        s.replace(',', "")
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .map(|n| n.trim_matches('.').to_string())
            .filter(|n| !n.is_empty())
            .collect()
    };
    let known = digits(script);
    let text = format!("{} {} {} {}", p.title, p.hook, p.what_you_see, thumb_text);
    for n in digits(&text) {
        if !known.contains(&n) {
            out.push(format!("the number {n} isn't in the script; use only numbers the video states"));
        }
    }
    let n = thumb_text.split_whitespace().count();
    if n > 5 {
        out.push(format!("the thumbnail has {n} words; use 2-4"));
    }
    if p.title.chars().count() > 60 {
        out.push(format!("the title is {} characters; keep it under 55", p.title.chars().count()));
    }
    let t = p.title.to_lowercase();
    if t.starts_with("how ") && (t.ends_with(" work") || t.ends_with(" works")) {
        out.push("the title is a plain \"How X Works\"; make it a surprising claim or a hidden thing".into());
    }
    out
}

pub async fn run(state: &AppState, run_id: &str, notes: Option<&str>) -> AppResult<Package> {
    pipeline::validate_run_id(run_id)?;
    let cfg = &state.cfg;
    let dir = pipeline::run_dir(cfg, run_id);
    let board: Storyboard = read(&dir.join("storyboard.json")).await?;
    let timeline: Timeline = read(&dir.join("timeline.json")).await?;
    let previous: Option<Package> = read(&dir.join("package.json")).await.ok();
    let rules = tokio::fs::read_to_string(cfg.root.join("docs").join("PACKAGING.md")).await.unwrap_or_default();
    let lessons = lessons::prompt_block(cfg).await;
    let sheet = review::contact_sheet(state, &dir).await;

    let system = format!(
        r##"You package videos for the YouTube channel "Blueprinted". Follow these rules exactly:

{rules}

{lessons}

## The image
A contact sheet: one frame from every scene of the video, numbered in the top-left corner. Frames come
from the clean render (no captions). Each frame is 16:9; the scene's own heading sits in its top ~15%.

## Output
ONE JSON object:
{{"title": "<= 55 characters",
  "hook": "the first two lines of the description",
  "what_you_see": "one short paragraph: the 3-4 mechanisms the video shows",
  "hashtags": ["#engineering", "#topic", "#howitworks"],
  "tags": ["10-20 tags, exact topic phrase first, 'blueprinted' last"],
  "thumbnail": {{
    "scene": scene number whose frame holds the subject,
    "at": 0.75 (0-1 through the scene; earlier only to catch the moment before a reveal),
    "lines": ["1-3 lines, 2-4 words in total, never restating the title's claim"],
    "highlight": "the one line drawn biggest; must equal one of lines",
    "crop": [x0, y0, x1, y1] fractions of that frame around the subject only (y0 >= 0.15 to drop the heading),
    "art": "right": the subject big on the right, in front of the backdrop's light (best for a mechanism);
           "bottom": a wide diagram under the text; "inset": ONE small object (a phone, an icon) in the
           bottom-left corner with a glowing beam into the backdrop's light (needs a backdrop),
    "pointer": [x, y] fractions inside the crop to ring and point at, or null,
    "backdrop": one of {backdrops:?} or null. Prefer one (rule 0), and it must fit the topic's world:
                "datacenter" (server-rack corridor) ONLY for computers, data and the internet; "ocean" (deep water,
                light shafts) for the sea, ships, canals, dams and anything underwater; "rays" (a burst of light)
                for everything else. Crop tightly: the subject must be big and readable on a phone.
  }}}}
Only use numbers the script states. Never promise what the video doesn't show."##,
        backdrops = BACKDROPS,
    );
    // The whole storyboard (not the clipped outline) is what numbers are checked against.
    let script = serde_json::to_string(&board)?;
    let mut user = format!(
        "## Script (scene: [layout] chapter title: narration | content)\n{}\n\nShort title already used in the \
         short: {}\n",
        review::script_outline(&board),
        board.short.as_ref().map(|s| s.title.as_str()).unwrap_or("-")
    );
    if let Some(p) = &previous {
        user.push_str(&format!("\n## Current packaging (keep what the notes don't change)\n{}\n", serde_json::to_string_pretty(p)?));
    }
    if let Some(n) = notes.filter(|n| !n.trim().is_empty()) {
        user.push_str(&format!("\n## The channel owner asks for these changes (they override the rules)\n{n}\n"));
    }
    let check_system = format!(
        "You judge YouTube thumbnails for the channel \"Blueprinted\" against these rules:\n\n{rules}\n\n\
         The image is the rendered thumbnail; the video's title and topic are in the message. Return ONE JSON \
         object: {{\"score\": 1-10, \"problems\": [\"specific, fixable problems\"]}}. Judge it as a viewer \
         scrolling on a phone: does it stop the scroll, does the backdrop fit the topic's world, is the subject \
         clear, is the text readable, does it raise a question the title doesn't answer? If the thumbnail text \
         restates the title's claim or gives away the answer (e.g. title \"...Without Pumps\" + thumbnail \"NO \
         PUMPS?\"), that is a problem; naming the same subject (title \"The Internet Is Hiding...\" + thumbnail \
         \"THE WHOLE INTERNET?\") is fine."
    );

    // Write, check the rules in code, render, look at it; feed every problem back
    // to the packager (it knows the topic) and keep the best-scoring attempt.
    let mut best: Option<Package> = None;
    let mut feedback = String::new();
    for round in 0..MAX_ROUNDS {
        let prompt = format!("{user}{feedback}\nWrite the JSON now.");
        let raw = match &sheet {
            Some(png) => llm::complete_vision(&state.client, cfg, &system, &prompt, png, true).await?,
            None => llm::complete(&state.client, cfg, &system, &prompt, true).await?,
        };
        let mut pack: Package = match parse(&raw) {
            Ok(p) => p,
            Err(e) => {
                feedback = format!("\n## Your last answer was not valid JSON ({e}); answer with the JSON object only.\n");
                continue;
            }
        };
        tidy(&mut pack, board.scenes.len());
        let mut problems = rule_problems(&pack, &script);
        if problems.is_empty() || round + 1 == MAX_ROUNDS {
            render_thumbnail(state, &dir, &pack.thumbnail).await?;
            let png = tokio::fs::read(dir.join("thumbnail.png")).await?;
            let check_user = format!("Topic: {}\nTitle: {}", board.title, pack.title);
            match llm::complete_vision(&state.client, cfg, &check_system, &check_user, &png, true)
                .await
                .and_then(|raw| parse::<Check>(&raw))
            {
                Ok(c) => {
                    pack.thumbnail_score = Some(c.score.clamp(1.0, 10.0) - problems.len() as f64);
                    problems.extend(c.problems);
                }
                Err(e) => tracing::warn!(error = %e, "thumbnail check failed"),
            }
        }
        pack.thumbnail_problems = problems.clone();
        tracing::info!(round, score = ?pack.thumbnail_score, problems = problems.len(), "packaging attempt");
        let score = pack.thumbnail_score.unwrap_or(0.0);
        if best.as_ref().is_none_or(|b| score > b.thumbnail_score.unwrap_or(0.0)) {
            best = Some(pack.clone());
        }
        if score >= GOOD_THUMBNAIL && problems.is_empty() {
            break;
        }
        feedback = format!(
            "\n## Your previous attempt\n{}\n## Its problems: fix every one. Change whatever it takes, the title \
             included: if the title and thumbnail tell the same claim, give one of them a different part of the \
             hook (e.g. the title says what happens, the thumbnail asks how). Don't repeat the same idea in new words.\n{}\n",
            serde_json::to_string(&pack)?,
            problems.iter().map(|p| format!("- {p}")).collect::<Vec<_>>().join("\n")
        );
    }
    let pack = best.ok_or_else(|| AppError::Llm("package: no usable packaging".into()))?;
    // The last render may have been a worse attempt; make thumbnail.png the one we keep.
    render_thumbnail(state, &dir, &pack.thumbnail).await?;

    tokio::fs::write(dir.join("package.json"), serde_json::to_vec_pretty(&pack)?).await?;
    tokio::fs::write(dir.join("youtube.txt"), youtube_text(&pack, &board, &timeline)).await?;
    Ok(pack)
}

async fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> AppResult<T> {
    let bytes = tokio::fs::read(path).await.map_err(|_| AppError::NotFound(path.display().to_string()))?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn parse<T: for<'de> Deserialize<'de>>(raw: &str) -> AppResult<T> {
    json_body(raw)
        .and_then(|b| serde_json::from_str(b).map_err(|e| e.to_string()))
        .map_err(|e| AppError::Llm(format!("package: {e}")))
}

/// Keep the LLM's choices inside what the engine and YouTube accept.
fn tidy(p: &mut Package, scenes: usize) {
    let t = &mut p.thumbnail;
    t.scene = t.scene.min(scenes.saturating_sub(1));
    t.at = t.at.clamp(0.05, 0.98);
    let [mut x0, mut y0, mut x1, mut y1] = t.crop.map(|v| v.clamp(0.0, 1.0));
    if x1 - x0 < 0.1 {
        (x0, x1) = (0.1, 0.9);
    }
    if y1 - y0 < 0.1 {
        (y0, y1) = (0.18, 0.92);
    }
    t.crop = [x0, y0, x1, y1];
    t.lines.retain(|l| !l.trim().is_empty());
    t.lines.truncate(3);
    if t.lines.is_empty() {
        t.lines.push(p.title.to_uppercase());
    }
    if t.highlight.as_ref().is_none_or(|h| !t.lines.contains(h)) {
        t.highlight = t.lines.last().cloned();
    }
    if !matches!(t.art.as_str(), "right" | "bottom" | "inset") {
        t.art = "right".into();
    }
    t.backdrop = t.backdrop.take().filter(|b| BACKDROPS.contains(&b.as_str()));
    // The corner subject beams into a backdrop's light; with no backdrop there's nothing to beam into.
    if t.backdrop.is_none() && t.art == "inset" {
        t.art = "right".into();
    }
    t.pointer = t.pointer.map(|[x, y]| [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)]);
    // Tags: under YouTube's 500-character limit, dropping from the end.
    let mut total = 0;
    p.tags.retain(|tag| {
        total += tag.chars().count() + 2;
        total <= MAX_TAG_CHARS
    });
    p.hashtags.truncate(3);
}

async fn render_thumbnail(state: &AppState, dir: &Path, t: &Thumb) -> AppResult<()> {
    let f = |v: f64| format!("{v:.3}");
    let mut cmd = Command::new(&state.cfg.python_bin);
    cmd.args(["-m", "story_engine", "thumbnail", "--run"])
        .arg(dir)
        .args(["--scene", &t.scene.to_string(), "--at", &f(t.at), "--lines", &t.lines.join("|"), "--art", &t.art])
        .args(["--crop", &t.crop.map(f).join(",")]);
    if let Some(h) = &t.highlight {
        cmd.args(["--highlight", h]);
    }
    if let Some([x, y]) = t.pointer {
        cmd.args(["--pointer", &format!("{},{}", f(x), f(y))]);
    }
    if let Some(b) = &t.backdrop {
        cmd.args(["--backdrop", b]);
    }
    media::run_cmd(cmd.current_dir(state.cfg.engine_dir()).env("PYTHONIOENCODING", "utf-8"), 300).await?;
    Ok(())
}

fn youtube_text(p: &Package, board: &Storyboard, tl: &Timeline) -> String {
    let mut s = format!("{}\n\n{}\n\n{}\n\n", p.title.trim(), p.hook.trim(), p.what_you_see.trim());
    let chapters = pipeline::chapter_lines(board, tl);
    if !chapters.is_empty() {
        s.push_str(&format!("Chapters\n{}\n\n", chapters.join("\n")));
    }
    s.push_str(&format!("{CHANNEL_LINE}\n\n{}\n\nTags: {}\n", p.hashtags.join(" "), p.tags.join(", ")));
    if let Some(sh) = board.short.as_ref().filter(|sh| !sh.title.is_empty()) {
        s.push_str(&format!("\nShort title: {}\n", sh.title));
    }
    s
}

/// JSON for the bot and the CLI.
pub fn summary(p: &Package) -> serde_json::Value {
    json!({
        "title": p.title,
        "thumbnail_score": p.thumbnail_score,
        "thumbnail_problems": p.thumbnail_problems,
        "tags": p.tags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(title: &str, lines: &[&str]) -> Package {
        Package {
            title: title.into(),
            hook: String::new(),
            what_you_see: String::new(),
            hashtags: vec![],
            tags: vec![],
            thumbnail: Thumb {
                scene: 0,
                at: 0.75,
                lines: lines.iter().map(|l| l.to_string()).collect(),
                highlight: None,
                crop: [0.1, 0.2, 0.9, 0.9],
                art: "right".into(),
                pointer: None,
                backdrop: None,
            },
            thumbnail_score: None,
            thumbnail_problems: vec![],
        }
    }

    #[test]
    fn rules_code_can_check() {
        let script = r#"{"scenes":[{"narration":"eighty five feet","content":{"value":85,"suffix":" ft"}}]}"#;
        let good = pack("The Computer You Use Every Day but Never See", &["NO SCREEN.", "NO SLEEP."]);
        assert!(rule_problems(&good, script).is_empty(), "{:?}", rule_problems(&good, script));
        let wordy = pack("How Elevators Work", &["THE CABLE SNAPS AND THEN", "WHAT?"]);
        let problems = rule_problems(&wordy, script);
        assert!(problems.iter().any(|e| e.contains("6 words")), "{problems:?}");
        assert!(problems.iter().any(|e| e.contains("How X Works")), "{problems:?}");
        let mut invented = pack("Ships Climb 85 Feet", &["UP HERE?"]);
        invented.hook = "Lifting 200,000-ton ships".into();
        let problems = rule_problems(&invented, script);
        assert!(problems.iter().any(|e| e.contains("200000")), "{problems:?}");
        assert!(!problems.iter().any(|e| e.contains(" 85 ")), "{problems:?}");
    }

    #[test]
    fn inset_needs_a_known_backdrop() {
        let mut p = pack("T", &["A B"]);
        p.thumbnail.art = "inset".into();
        p.thumbnail.backdrop = Some("ocean".into());
        tidy(&mut p, 10);
        assert_eq!(p.thumbnail.art, "inset");
        p.thumbnail.backdrop = Some("volcano".into());
        tidy(&mut p, 10);
        assert_eq!(p.thumbnail.art, "right");
        assert!(p.thumbnail.backdrop.is_none());
    }
}
