//! Storyboard writing with an LLM (Gemini first, DeepSeek as fallback).
//!
//! The LLM fills in a JSON storyboard using only the vocabulary from the
//! engine catalog; validation checks pacing and a repair round feeds the
//! problems back until it passes.
//!
//! Short videos are written in one call. Long videos (five minutes) are
//! written in two stages so the script stays tight from start to finish:
//!   1. an outline: chapters, the "wow" each one lands, which chapter becomes
//!      the vertical short, and how well the topic fits the channel niche
//!      (off-niche topics are refused unless forced);
//!   2. each chapter's scenes, written in parallel against its own word
//!      budget, then joined with silent chapter cards in between.

use futures::future::try_join_all;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::catalog::Catalog;
use crate::config::{Config, LlmProvider, VisualStyle};
use crate::error::{AppError, AppResult};
use crate::niche;
use crate::storyboard::{
    Budget, LONG_FORM_SECS, MECHANISM, Scene, ShortPick, Storyboard, WORDS_PER_SEC, json_body, normalize_scenes,
    parse_storyboard, process_problems, validate_scenes, word_count,
};

const MAX_ROUNDS: usize = 3;
/// Length of the silent card shown at the start of each chapter.
const CHAPTER_CARD_SECS: f64 = 3.2;
/// LLM-written scenes are the least predictable visuals, so a long video
/// keeps at most this many; extra ones become their fallback layouts.
const MAX_CUSTOM_LONG: usize = 2;

pub struct StoryRequest<'a> {
    pub topic: &'a str,
    pub theme: Option<&'a str>,
    pub audience: Option<&'a str>,
    pub portrait: bool,
    pub style: VisualStyle,
    /// Multiplies the word budget (a draft that read too long/short is rewritten with this).
    pub word_scale: f64,
    /// Make the video even if the topic doesn't fit the channel niche.
    pub force: bool,
}

pub async fn write_storyboard(
    client: &Client,
    cfg: &Config,
    cat: &Catalog,
    req: &StoryRequest<'_>,
) -> AppResult<Storyboard> {
    if !cfg.has_llm() {
        return Err(AppError::Config(
            "no LLM key configured: set GEMINI_API_KEY or DEEPSEEK_API_KEY (or pass a storyboard file)".into(),
        ));
    }
    let budget = Budget::for_duration(cfg.target_duration).scaled(req.word_scale);
    if cfg.target_duration < LONG_FORM_SECS {
        return write_single(client, cfg, cat, req, &budget).await;
    }
    let outline = write_outline(client, cfg, cat, req, &budget).await?;
    tracing::info!(title = %outline.title, chapters = outline.chapters.len(), fit = outline.fit.score, "outline ready");
    let sum_secs: f64 = outline.chapters.iter().map(|c| c.seconds.max(5.0)).sum();
    let chapters = try_join_all(outline.chapters.iter().enumerate().map(|(i, _)| {
        let share = outline.chapters[i].seconds.max(5.0) / sum_secs;
        let words = (budget.target_words as f64 * share).round() as usize;
        write_chapter(client, cfg, cat, req, &outline, i, words)
    }))
    .await?;
    let mut sb = assemble(&outline, chapters);
    sb.normalize(cat);
    if let Some(t) = req.theme.filter(|t| cat.themes.contains_key(*t)) {
        sb.theme = t.to_string();
    }
    for p in sb.validate(&budget) {
        tracing::warn!("storyboard: {p}");
    }
    Ok(sb)
}

// ---------------------------------------------------------------------------
// shared prompt fragments
// ---------------------------------------------------------------------------

fn style_direction(style: VisualStyle, custom: bool, latex: bool) -> String {
    match style {
        VisualStyle::Diagram => format!(
            r#"## Style: diagram explainer (3Blue1Brown-like)
- Use theme "blueprint" (default) or "graphite" (maths, physics, geometry). Backgrounds mostly
  "glow" or "blueprint"; cameras "drift", "push_in" or "static".
{MECHANISM_RULES}
- Supporting layouts: cross_section, scale, fun_fact, stat, graph, vectors, network, dot_grid, equation,
  bar_chart, timeline, quote, title.
{custom_rule}- Maths goes in LaTeX "tex" fields{latex_note}. In JSON, escape backslashes: "\\frac{{1}}{{2}}".
- Function expressions ("expr") are Python maths of x: x**2, sin(x), exp(-x), sqrt(x)."#,
            custom_rule = if custom {
                "- Only if no kit part or layout can show it (geometry being cut and rearranged, a very unusual\n  \
                 motion), use layout \"custom\" with a precise \"brief\" (objects, positions, colours, what moves\n  \
                 on which narration word) and a \"fallback\" {\"layout\": ..., \"content\": ...}. Max 2 per video.\n"
            } else {
                "- Do not use the \"custom\" layout.\n"
            },
            latex_note = if latex { "" } else { " (shown as plain text on this machine)" },
        ),
        VisualStyle::Infographic => format!(
            r#"## Style: flat infographic explainer (Kurzgesagt-like)
- Use any non-diagram theme (midnight, sunset, ocean, forest, ember, neon, paper, candy, mono).
{MECHANISM_RULES}
- Supporting layouts: cross_section, scale, fun_fact, stat, bar_chart, timeline, quote, title; "character"
  for a human moment (counts towards the icon limit).
- Do not use the "custom" layout."#
        ),
    }
}

/// The core visual rule: viewers must SEE how it works, so mechanisms lead and
/// icon cards are capped (validated by `storyboard::validate_scenes`).
const MECHANISM_RULES: &str = r#"- SHOW HOW IT WORKS. The main tool is "process": a working schematic built from kit parts (tanks,
  valves, pumps, gears, pistons, lasers, generators...) joined by pipes, wires, rays or fibres, whose
  steps run the machine on cue words (a valve opens, flow starts, a level rises, a rotor spins, the
  camera moves in on the part being explained). "cross_section" cuts things open layer by layer.
- Build ONE machine per chapter and grow it: the next process scene sets "continue": true, keeps the same
  part ids, may add parts/links, and plays new steps. 2-4 consecutive process scenes per chapter is ideal
  (consecutive process scenes are allowed only with "continue": true).
- Icon-based layouts (hero, icon_row, list, compare, flow, cycle, character) are limited to 1 in 5 scenes.
  Never use "flow" where a process diagram could show the real thing.
- Kit parts are listed below. If the system needs a component the kit lacks, give it a new generic
  snake_case kind (e.g. "lock_gate", "repeater", "turbofan", "heat_exchanger") plus "make": one line
  describing how to draw it. It is built once and reused by future videos, so name it generically
  and prefer existing parts. At most 2 new kinds per video.
- Every process step is tied to a cue word and does something visible. Place parts with "col"/"row"
  (small integers, left to right in the order things happen; fractions like 0.5 are fine).

Example process scene content (a canal lock filling):
{"parts": [{"id": "lake", "kind": "tank", "label": "Upper lake", "width": 1.3, "level": 0.9, "col": 0},
           {"id": "valve", "kind": "valve", "label": "Inlet valve", "col": 1, "row": 0.6},
           {"id": "lock", "kind": "tank", "label": "Lock chamber", "width": 1.6, "level": 0.15, "col": 2}],
 "links": [{"id": "culvert", "from": "lake", "to": "valve", "kind": "pipe", "carries": "water"},
           {"id": "culvert2", "from": "valve", "to": "lock", "kind": "pipe", "carries": "water"}],
 "steps": [{"cue": "valve", "do": [{"open": "valve"}, {"focus": "valve"}]},
           {"cue": "gravity", "do": [{"on": "culvert culvert2"}, {"focus": "all"}, {"note": "No pumps"}]},
           {"cue": "rises", "do": [{"level": ["lock", 0.85]}, {"level": ["lake", 0.8]}]}]}
Next scene: {"continue": true, "parts": [{"id": "ship", "kind": "block", "label": "Ship", "col": 2, "row": -0.6}],
 "steps": [{"cue": "ship", "do": [{"show": "ship"}, {"highlight": "ship"}]}]}"#;

fn voice_direction(cfg: &Config) -> String {
    let mut s = String::from(
        "## Narration\n\
         - Spoken English only: no emojis, stage directions, URLs, or symbols. Write numbers the way they\n  \
         are spoken (\"seventy one percent\", \"nineteen sixty nine\").\n\
         - Conversational, vivid, concrete. Short sentences. Every sentence earns its place.\n\
         - Be factually accurate. Prefer well-established facts; if a number is uncertain, say it\n  \
         generally rather than inventing precision.\n",
    );
    if cfg.tts_supports_tags() {
        s.push_str(
            "- The voice is expressive and follows AUDIO TAGS in square brackets. Start most scenes with one tag\n  \
             that fits the moment, and occasionally add one mid-sentence: [curious], [excited], [amazed],\n  \
             [impressed], [serious], [thoughtful], [whispers], [chuckles], [dramatic pause], [short pause].\n  \
             At most 2 tags per scene. Tags are not spoken and don't count as words. Keep the delivery\n  \
             warm and documentary-like; save [excited]/[amazed] for real wow moments.\n",
        );
    }
    s
}

fn vocabulary(cat: &Catalog, style: VisualStyle, custom: bool, long_form: bool) -> String {
    let mut layout_map = cat.layouts.clone();
    if !custom || style == VisualStyle::Infographic {
        layout_map.remove("custom");
    }
    layout_map.remove("chapter");
    if !long_form {
        layout_map.remove("fun_fact");
    }
    let layouts = serde_json::to_string_pretty(&layout_map).unwrap_or_default();
    let themes = cat.themes.values().map(|t| format!("  - {}: {}", t.name, t.description)).collect::<Vec<_>>().join("\n");
    let backgrounds = cat.backgrounds.iter().map(|(k, v)| format!("  - {k}: {v}")).collect::<Vec<_>>().join("\n");
    let parts = cat.parts.iter().map(|(k, v)| format!("  - {k}: {v}")).collect::<Vec<_>>().join("\n");
    format!(
        r#"## Vocabulary
Layouts (name -> purpose and content schema):
{layouts}

Kit parts for "process" scenes (kind -> what it is, fields, actions):
{parts}

Icons (use these exact names; pick the closest match): {icons}

Themes:
{themes}

Backgrounds:
{backgrounds}

Cameras: {cameras}
Transitions: {transitions}
Character looks: {looks}; poses: {poses}; expressions: {expressions}"#,
        icons = cat.icons.join(", "),
        cameras = cat.cameras.join(", "),
        transitions = cat.transitions.join(", "),
        looks = cat.looks.join(", "),
        poses = cat.poses.join(", "),
        expressions = cat.expressions.join(", "),
    )
}

const SCENE_SCHEMA: &str = r#"{
  "narration": "what the narrator says in this scene ('' for a silent visual beat)",
  "layout": "<layout name>",
  "title": "optional on-screen heading <= 5 words (omit for title/quote/fun_fact layouts)",
  "background": "<background name>",
  "camera": "<camera move>",
  "transition": "<transition INTO this scene>",
  "hold": "seconds, ONLY for a silent visual beat (3-6)",
  "content": { ...fields for the chosen layout... }
}"#;

const VISUAL_RULES: &str = r#"## Visual direction
- Never use the same layout in two consecutive scenes, except process scenes that continue the same
  diagram ("continue": true). Vary backgrounds and camera moves.
- Every visual element should be NAMED in that scene's narration. Put the exact narration word that
  names it into its "cue" field (a single word copied verbatim from the narration, e.g. narration
  "...needs sunlight, water and..." -> cue "sunlight"). The engine reveals each element at the
  moment that word is spoken, so name the first element early in the sentence.
- On-screen text is short (labels <= 3 words). Never put full narration sentences on screen;
  captions are added automatically.
- Keep one theme for the whole video."#;

// ---------------------------------------------------------------------------
// single-call storyboards (short videos)
// ---------------------------------------------------------------------------

async fn write_single(
    client: &Client,
    cfg: &Config,
    cat: &Catalog,
    req: &StoryRequest<'_>,
    budget: &Budget,
) -> AppResult<Storyboard> {
    let system = single_prompt(cfg, cat, budget, req);
    let mut user = user_prompt(req);
    let mut last_problems: Vec<String> = Vec::new();
    let mut best: Option<(usize, Storyboard)> = None;

    for round in 1..=MAX_ROUNDS {
        let raw = complete(client, cfg, &system, &user, true).await?;
        match parse_storyboard(&raw) {
            Ok(mut sb) => {
                sb.normalize(cat);
                if let Some(t) = req.theme.filter(|t| cat.themes.contains_key(*t)) {
                    sb.theme = t.to_string();
                }
                let mut problems = sb.validate(budget);
                problems.extend(process_problems(&sb.scenes, cat));
                tracing::info!(round, words = sb.total_words(), scenes = sb.scenes.len(),
                               problems = problems.len(), "storyboard draft");
                if problems.is_empty() {
                    return Ok(sb);
                }
                if best.as_ref().is_none_or(|(n, _)| problems.len() < *n) {
                    best = Some((problems.len(), sb.clone()));
                }
                user = repair_prompt(&user_prompt(req), &raw, &problems);
                last_problems = problems;
            }
            Err(e) => {
                tracing::warn!(round, error = %e, "storyboard unparseable");
                last_problems = vec![e.clone()];
                user = repair_prompt(&user_prompt(req), &raw, &last_problems);
            }
        }
    }
    // Only fall back to an imperfect draft if its word count is usable:
    // the timeline fitter can absorb small misses, not big ones.
    if let Some((_, sb)) = best {
        let w = sb.total_words();
        if w >= budget.min_words * 85 / 100 && w <= budget.max_words * 110 / 100 && !sb.scenes.is_empty() {
            tracing::warn!(problems = ?last_problems, "using best-effort storyboard");
            return Ok(sb);
        }
    }
    Err(AppError::Llm(format!(
        "could not produce a valid storyboard in {MAX_ROUNDS} rounds: {}",
        last_problems.join("; ")
    )))
}

fn user_prompt(req: &StoryRequest<'_>) -> String {
    let mut s = format!("Topic: {}\n", req.topic.trim());
    if let Some(a) = req.audience {
        s.push_str(&format!("Audience: {a}\n"));
    }
    if let Some(t) = req.theme {
        s.push_str(&format!("Use theme: {t}\n"));
    }
    s.push_str("Write the JSON now.");
    s
}

fn repair_prompt(base: &str, previous: &str, problems: &[String]) -> String {
    format!(
        "{base}\n\nYour previous answer had these problems:\n- {}\n\nPrevious attempt:\n{}\n\n\
         Return the corrected, complete JSON only.",
        problems.join("\n- "),
        previous.chars().take(14000).collect::<String>()
    )
}

fn single_prompt(cfg: &Config, cat: &Catalog, b: &Budget, req: &StoryRequest<'_>) -> String {
    let target = cfg.target_duration;
    format!(
        r#"You are the head writer and director of a premium animated explainer channel
(think Kurzgesagt, TED-Ed, Vox). You write storyboards for EXACTLY {target:.0}-second
2D animated videos. A deterministic animation engine renders your storyboard, so you
may ONLY use the vocabulary below.

## Output
Return ONE JSON object, no prose, no markdown:
{{
  "title": "punchy video title, <= 6 words",
  "theme": "<theme name>",
  "music": "short music brief, e.g. 'curious light pizzicato, warm, 90 bpm, no vocals'",
  "scenes": [ {SCENE_SCHEMA} ]
}}

## Timing — this is what makes the video exactly {target:.0}s
- {min_s}-{max_s} scenes (aim for 7). Total narration {min_w}-{max_w} spoken words, aim for {tw}.
- Each scene 12-30 words. Narration is read at ~150 words/minute.

## Story
- Arc: hook (surprising question/fact) -> context -> 2-4 escalating insights -> payoff -> memorable closer.
- Scene 1 should be a strong hook; use layout "title" or "hero" or "stat".
- The last scene is a short closer (layout "title" or "quote" works well).

{voice}
{VISUAL_RULES}
- Video is {orientation}.

{direction}

{vocab}
"#,
        min_s = b.min_scenes,
        max_s = b.max_scenes,
        min_w = b.min_words,
        max_w = b.max_words,
        tw = b.target_words,
        voice = voice_direction(cfg),
        orientation = if req.portrait { "vertical 9:16 (phone)" } else { "horizontal 16:9" },
        direction = style_direction(req.style, cfg.custom_scenes, cat.latex),
        vocab = vocabulary(cat, req.style, cfg.custom_scenes, false),
    )
}

// ---------------------------------------------------------------------------
// long videos, stage 1: the outline
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Fit {
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterPlan {
    pub name: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub beats: Vec<String>,
    /// The fun fact / record / engineering marvel this chapter lands.
    #[serde(default)]
    pub wow: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// Approximate narrated seconds.
    #[serde(default = "default_chapter_secs")]
    pub seconds: f64,
}

fn default_chapter_secs() -> f64 {
    45.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortPlan {
    pub chapter: usize,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outline {
    pub title: String,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub music: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub fit: Fit,
    pub chapters: Vec<ChapterPlan>,
    #[serde(default)]
    pub short: Option<ShortPlan>,
}

fn outline_problems(o: &Outline) -> Vec<String> {
    let mut p = Vec::new();
    let n = o.chapters.len();
    if !(4..=7).contains(&n) {
        p.push(format!("use 4-7 chapters (got {n})"));
    }
    if let Some(first) = o.chapters.first()
        && !(10.0..=45.0).contains(&first.seconds)
    {
        p.push("chapter 1 is the cold open: 15-35 seconds".into());
    }
    for (i, c) in o.chapters.iter().enumerate() {
        if c.name.trim().is_empty() {
            p.push(format!("chapter {} has no name", i + 1));
        }
        if i > 0 && i + 1 < n && c.wow.trim().len() < 10 {
            p.push(format!("chapter {} needs a concrete \"wow\" (fun fact, record or engineering marvel)", i + 1));
        }
    }
    match &o.short {
        Some(s) if s.chapter == 0 || s.chapter > n => {
            p.push(format!("short.chapter must be a chapter number from 2 to {n}"))
        }
        None => p.push("pick a chapter for the vertical short (\"short\": {\"chapter\", \"title\"})".into()),
        _ => {}
    }
    if o.fit.reason.trim().is_empty() {
        p.push("explain the niche fit score in fit.reason".into());
    }
    p
}

async fn write_outline(
    client: &Client,
    cfg: &Config,
    cat: &Catalog,
    req: &StoryRequest<'_>,
    budget: &Budget,
) -> AppResult<Outline> {
    let minutes = cfg.target_duration / 60.0;
    let speech_secs = budget.target_words as f64 / WORDS_PER_SEC;
    let themes = cat.themes.keys().cloned().collect::<Vec<_>>().join(", ");
    let system = format!(
        r#"You are the showrunner of an animated YouTube channel. You plan {minutes:.0}-minute videos.

{brief}

## Output
Return ONE JSON object, no prose:
{{
  "title": "curiosity-driven YouTube title, <= 9 words",
  "theme": "one of: {themes}",
  "music": "short music brief, e.g. 'wonder-filled documentary underscore, light pulse, no vocals'",
  "description": "2-3 sentence YouTube description that promises the payoff",
  "fit": {{"score": 1-10, "reason": "how well the topic fits the channel brief and an animated-diagram format"}},
  "chapters": [
    {{"name": "<= 4 words", "goal": "what the viewer understands after it",
      "beats": ["3-5 short story beats"], "wow": "the fun fact, record or engineering marvel it lands",
      "icon": "optional icon name", "seconds": 45}}
  ],
  "short": {{"chapter": 3, "title": "<= 7 word hook title for a 59-second vertical short"}}
}}

## Rules
- fit.score: be honest. 8-10 = a mechanism to animate plus real wow facts; 1-4 = a poor fit (see the brief).
- 5-6 chapters. "seconds" is narrated time; all chapters together ≈ {speech_secs:.0} seconds.
- Chapter 1 is a cold open (15-30s): a jaw-dropping question, fact or scale comparison. No throat-clearing.
- Middle chapters each explain one step of HOW it works, building on the last, and each lands one "wow"
  with a concrete number or true-scale comparison. Relate it to things viewers know.
- The last chapter (20-40s) is the payoff: tie the mechanism together and end on a memorable line.
- short.chapter (1-based) is the middle chapter that best works on its own as a 45-58 second short:
  it opens with a hook needing no context and contains the biggest wow.
- Be factually accurate; prefer well-established facts."#,
        brief = niche::BRIEF,
    );
    let mut user = user_prompt(req);
    let mut last = Vec::new();
    for round in 1..=MAX_ROUNDS {
        let raw = complete(client, cfg, &system, &user, true).await?;
        let parsed = json_body(&raw).and_then(|b| serde_json::from_str::<Outline>(b).map_err(|e| e.to_string()));
        match parsed {
            Ok(o) => {
                if !req.force && o.fit.score > 0.0 && o.fit.score < cfg.min_topic_fit as f64 {
                    return Err(AppError::BadRequest(format!(
                        "topic doesn't fit the channel niche (scored {:.0}/10: {}). Pick another topic \
                         (`story_animator topics` suggests some) or pass --force to make it anyway.",
                        o.fit.score, o.fit.reason
                    )));
                }
                let problems = outline_problems(&o);
                tracing::info!(round, chapters = o.chapters.len(), problems = problems.len(), "outline draft");
                if problems.is_empty() {
                    return Ok(o);
                }
                user = repair_prompt(&user_prompt(req), &raw, &problems);
                last = problems;
            }
            Err(e) => {
                last = vec![format!("invalid outline JSON: {e}")];
                user = repair_prompt(&user_prompt(req), &raw, &last);
            }
        }
    }
    Err(AppError::Llm(format!("could not produce a valid outline in {MAX_ROUNDS} rounds: {}", last.join("; "))))
}

// ---------------------------------------------------------------------------
// long videos, stage 2: chapters
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChapterOut {
    scenes: Vec<Scene>,
}

fn chapter_problems(scenes: &[Scene], budget: &Budget, cat: &Catalog, i: usize, n: usize) -> Vec<String> {
    let mut p = validate_scenes(scenes, budget);
    p.extend(process_problems(scenes, cat));
    if scenes.iter().any(|s| s.layout == "chapter") {
        p.push("don't use the \"chapter\" layout; chapter cards are added automatically".into());
    }
    let middle = i > 0 && i + 1 < n;
    if middle && !scenes.iter().any(|s| matches!(s.layout.as_str(), "fun_fact" | "scale" | "stat")) {
        p.push("land the chapter's wow on screen with a \"fun_fact\", \"scale\" or \"stat\" scene".into());
    }
    if middle && !scenes.iter().any(|s| MECHANISM.contains(&s.layout.as_str())) {
        p.push("show how this step works with a \"process\" diagram (or \"cross_section\"); build it up over \
                2-4 scenes with \"continue\": true"
            .into());
    }
    if n > 1 && middle && scenes.iter().filter(|s| s.layout == "process").count() == 1 && scenes.len() >= 5 {
        p.push("grow the chapter's process diagram over at least 2 consecutive scenes (\"continue\": true)".into());
    }
    if let Some(first) = scenes.first()
        && first.is_silent()
    {
        p.push("the first scene must be narrated".into());
    }
    p
}

async fn write_chapter(
    client: &Client,
    cfg: &Config,
    cat: &Catalog,
    req: &StoryRequest<'_>,
    outline: &Outline,
    i: usize,
    words: usize,
) -> AppResult<Vec<Scene>> {
    let n = outline.chapters.len();
    let ch = &outline.chapters[i];
    let budget = Budget::for_chapter(words);
    let role = if i == 0 {
        "This is the COLD OPEN. Scene 1 hooks within its first sentence (layout \"title\", \"stat\", \"scale\" or \
         \"hero\"). End on a line that makes the viewer need the rest of the video."
            .to_string()
    } else if i + 1 == n {
        "This is the FINAL chapter: pay off the opening hook, tie the mechanism together, and end on a short, \
         memorable closer (layout \"title\" or \"quote\")."
            .to_string()
    } else {
        format!(
            "This is a middle chapter. Explain this step of HOW it works, relate it to something familiar, and land \
             the wow \"{}\" on screen (fun_fact, scale or stat).",
            ch.wow
        )
    };
    let short_note = if outline.short.as_ref().is_some_and(|s| s.chapter == i + 1) {
        "\nThis chapter is ALSO cut out as a standalone vertical short: its first scene must hook without any \
         earlier context, and the chapter must make sense on its own."
    } else {
        ""
    };
    let silent_note = if budget.max_silent > 0 {
        "\n## Silent visual beat (saves narration)\n\
         You may add ONE silent scene: \"narration\": \"\" with \"hold\": 3-6, right after a big reveal, where \
         the animation speaks for itself (a network pulsing, a mechanism running, a scale reveal). Never use it \
         for anything that needs explaining, and never as the first scene."
    } else {
        ""
    };
    let system = format!(
        r#"You are the head writer and director of a premium animated explainer channel. You write ONE
chapter of a {minutes:.0}-minute video as storyboard scenes. A deterministic animation engine renders
them, so you may ONLY use the vocabulary below.

{brief}

## Output
Return ONE JSON object, no prose: {{"scenes": [ {SCENE_SCHEMA} ]}}

## Timing
- {min_s}-{max_s} scenes. Total narration {min_w}-{max_w} spoken words, aim for {tw}.
- Each narrated scene 12-30 words (~150 words/minute).
{silent_note}

{voice}
{VISUAL_RULES}
- Do not use the "chapter" layout (chapter cards are added automatically).
- Video is horizontal 16:9.

{direction}

{vocab}
"#,
        minutes = cfg.target_duration / 60.0,
        brief = niche::BRIEF,
        min_s = budget.min_scenes,
        max_s = budget.max_scenes,
        min_w = budget.min_words,
        max_w = budget.max_words,
        tw = budget.target_words,
        voice = voice_direction(cfg),
        direction = style_direction(req.style, cfg.custom_scenes, cat.latex),
        vocab = vocabulary(cat, req.style, cfg.custom_scenes, true),
    );
    let plan = serde_json::to_string_pretty(&json!({
        "title": outline.title,
        "theme": outline.theme,
        "chapters": outline.chapters.iter().enumerate().map(|(k, c)| json!({
            "number": k + 1, "name": c.name, "goal": c.goal, "beats": c.beats, "wow": c.wow
        })).collect::<Vec<_>>(),
    }))
    .unwrap_or_default();
    let base = format!(
        "Topic: {topic}\n{audience}Video plan:\n{plan}\n\nWrite chapter {num} of {n}: \"{name}\".\nGoal: {goal}\n\
         Beats: {beats}\n{role}{short_note}\nUse theme \"{theme}\". Don't repeat what other chapters cover. \
         Write the JSON now.",
        topic = req.topic.trim(),
        audience = req.audience.map(|a| format!("Audience: {a}\n")).unwrap_or_default(),
        num = i + 1,
        name = ch.name,
        goal = ch.goal,
        beats = ch.beats.join(" | "),
        theme = outline.theme,
    );

    let mut user = base.clone();
    let mut last = Vec::new();
    let mut best: Option<(usize, Vec<Scene>)> = None;
    for round in 1..=MAX_ROUNDS {
        let raw = complete(client, cfg, &system, &user, true).await?;
        let parsed = json_body(&raw).and_then(|b| serde_json::from_str::<ChapterOut>(b).map_err(|e| e.to_string()));
        match parsed {
            Ok(mut out) => {
                normalize_scenes(&mut out.scenes, cat);
                let problems = chapter_problems(&out.scenes, &budget, cat, i, n);
                let w: usize = out.scenes.iter().map(|s| word_count(&s.narration)).sum();
                tracing::info!(chapter = i + 1, round, words = w, scenes = out.scenes.len(),
                               problems = problems.len(), "chapter draft");
                if problems.is_empty() {
                    return Ok(out.scenes);
                }
                if best.as_ref().is_none_or(|(k, _)| problems.len() < *k) {
                    best = Some((problems.len(), out.scenes));
                }
                user = repair_prompt(&base, &raw, &problems);
                last = problems;
            }
            Err(e) => {
                last = vec![format!("invalid chapter JSON: {e}")];
                user = repair_prompt(&base, &raw, &last);
            }
        }
    }
    if let Some((_, scenes)) = best {
        let w: usize = scenes.iter().map(|s| word_count(&s.narration)).sum();
        if !scenes.is_empty() && w >= budget.min_words * 80 / 100 && w <= budget.max_words * 115 / 100 {
            tracing::warn!(chapter = i + 1, problems = ?last, "using best-effort chapter");
            return Ok(scenes);
        }
    }
    Err(AppError::Llm(format!("chapter {} failed after {MAX_ROUNDS} rounds: {}", i + 1, last.join("; "))))
}

/// Join chapters into one storyboard, with a silent card before every chapter
/// after the cold open, and mark the scenes that make the short.
fn assemble(outline: &Outline, chapters: Vec<Vec<Scene>>) -> Storyboard {
    let numbered = chapters.len().saturating_sub(1);
    let mut scenes = Vec::new();
    let mut short = None;
    for (i, chapter_scenes) in chapters.into_iter().enumerate() {
        let plan = &outline.chapters[i];
        if i > 0 {
            scenes.push(Scene {
                narration: String::new(),
                layout: "chapter".into(),
                title: None,
                background: "glow".into(),
                camera: "push_in".into(),
                transition: "fadeblack".into(),
                hold: Some(CHAPTER_CARD_SECS),
                chapter: Some(i),
                content: json!({ "number": i, "total": numbered, "name": plan.name, "icon": plan.icon }),
            });
        }
        let from = scenes.len();
        for mut s in chapter_scenes {
            s.chapter = Some(i);
            scenes.push(s);
        }
        if let Some(sp) = outline.short.as_ref().filter(|sp| sp.chapter == i + 1)
            && scenes.len() > from
        {
            short = Some(ShortPick { from, to: scenes.len() - 1, title: sp.title.clone() });
        }
    }
    let mut customs = 0;
    for s in scenes.iter_mut().filter(|s| s.layout == "custom") {
        customs += 1;
        if customs > MAX_CUSTOM_LONG
            && let Some(layout) = s.content["fallback"]["layout"].as_str().map(str::to_string)
        {
            s.content = s.content["fallback"]["content"].clone();
            s.layout = layout;
        }
    }
    Storyboard {
        title: outline.title.clone(),
        theme: if outline.theme.is_empty() { "blueprint".into() } else { outline.theme.clone() },
        music: outline.music.clone(),
        description: outline.description.clone(),
        chapters: outline.chapters.iter().map(|c| c.name.clone()).collect(),
        short,
        scenes,
    }
}

/// Inputs for writing one `custom` scene's code.
pub struct SceneCodeRequest<'a> {
    pub brief: &'a str,
    pub narration: &'a str,
    /// (word, start seconds) relative to the scene start
    pub words: &'a [(String, f64)],
    pub duration: f64,
    pub content_end: f64,
    pub theme: &'a crate::catalog::Theme,
    pub portrait: bool,
    pub title: Option<&'a str>,
    pub previous_code: Option<&'a str>,
    pub previous_error: Option<&'a str>,
}

/// Ask the LLM for a `build(ctx)` function implementing one custom scene.
pub async fn write_scene_code(client: &Client, cfg: &Config, cat: &Catalog, r: &SceneCodeRequest<'_>) -> AppResult<String> {
    let system = format!(
        "You are an expert Manim Community animator in the style of 3Blue1Brown, writing one scene \
         for a narrated explainer. Output only code.\n{}",
        cat.custom_api
    );
    let words = r
        .words
        .iter()
        .map(|(w, s)| format!("{s:.2}s {w}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut user = format!(
        "SCENE BRIEF (what to show):\n{brief}\n\nNARRATION:\n{narr}\n\nWORD TIMINGS (seconds from scene start):\n{words}\n\n\
         Scene length {dur:.2}s; finish all motion by {end:.2}s. Frame: {frame}. {title}\n\
         Theme colours: bg {bg}, bg2 {bg2}, ink {ink}, muted {muted}, accent {a1}, accent2 {a2}, accent3 {a3}.\n",
        brief = r.brief,
        narr = r.narration,
        dur = r.duration,
        end = r.content_end,
        frame = if r.portrait { "portrait 8 x 14.2 units" } else { "landscape 14.2 x 8 units" },
        title = match r.title {
            Some(t) => format!("A heading \"{t}\" is already drawn at the top; don't add another title."),
            None => "There is no heading; you may use the top of the frame.".into(),
        },
        bg = r.theme.bg,
        bg2 = r.theme.bg2,
        ink = r.theme.ink,
        muted = r.theme.muted,
        a1 = r.theme.accent,
        a2 = r.theme.accent2,
        a3 = r.theme.accent3,
    );
    if let (Some(code), Some(err)) = (r.previous_code, r.previous_error) {
        user.push_str(&format!(
            "\nYOUR PREVIOUS ATTEMPT FAILED. Fix it.\nError:\n{err}\n\nPrevious code:\n```python\n{code}\n```\n"
        ));
    }
    user.push_str("\nWrite the complete file now.");
    let raw = complete(client, cfg, &system, &user, false).await?;
    extract_python(&raw).ok_or_else(|| AppError::Llm("no python code block in response".into()))
}

/// Inputs for writing one new, reusable kit part.
pub struct PartCodeRequest<'a> {
    pub kind: &'a str,
    pub make: &'a str,
    pub actions: &'a [String],
    pub labels: &'a [String],
    pub topic: &'a str,
    pub previous_code: Option<&'a str>,
    pub previous_error: Option<&'a str>,
}

/// Ask the LLM for a kit part file (`PART` + `build(k, spec)`).
pub async fn write_part_code(client: &Client, cfg: &Config, cat: &Catalog, r: &PartCodeRequest<'_>) -> AppResult<String> {
    let existing = cat.parts.keys().cloned().collect::<Vec<_>>().join(", ");
    let system = format!(
        "You are an expert Manim Community animator extending a reusable kit of schematic parts for engineering \
         explainer videos. Output only code.\n{}",
        cat.part_api
    );
    let mut actions: Vec<String> = r.actions.to_vec();
    if actions.is_empty() {
        actions = vec!["on".into(), "off".into()];
    }
    let mut user = format!(
        "NEW PART: \"{kind}\"\nWHAT TO DRAW: {make}\nIT MUST SUPPORT THESE ACTIONS: {acts} (plus a sensible highlight)\n\
         First used for: {topic} (labels: {labels}). Keep it generic so other videos can reuse it.\n\
         Parts that already exist (don't duplicate them; you may draw something similar in your own style): {existing}\n\
         Set PART[\"name\"] to \"{kind}\" and list the actions you implement.\n",
        kind = r.kind,
        make = r.make,
        acts = actions.join(", "),
        topic = r.topic,
        labels = if r.labels.is_empty() { "-".into() } else { r.labels.join(", ") },
    );
    if let (Some(code), Some(err)) = (r.previous_code, r.previous_error) {
        user.push_str(&format!(
            "\nYOUR PREVIOUS ATTEMPT FAILED. Fix it.\nError:\n{err}\n\nPrevious code:\n```python\n{code}\n```\n"
        ));
    }
    user.push_str("\nWrite the complete file now.");
    let raw = complete(client, cfg, &system, &user, false).await?;
    extract_python(&raw).ok_or_else(|| AppError::Llm("no python code block in response".into()))
}

fn extract_python(s: &str) -> Option<String> {
    for fence in ["```python", "```py", "```"] {
        if let Some(i) = s.find(fence) {
            let rest = &s[i + fence.len()..];
            if let Some(j) = rest.find("```") {
                let code = rest[..j].trim();
                if !code.is_empty() {
                    return Some(code.to_string());
                }
            }
        }
    }
    s.contains("def build").then(|| s.trim().to_string())
}

pub(crate) async fn complete(client: &Client, cfg: &Config, system: &str, user: &str, json_mode: bool) -> AppResult<String> {
    let order: Vec<LlmProvider> = match cfg.llm_provider {
        LlmProvider::Gemini => vec![LlmProvider::Gemini],
        LlmProvider::DeepSeek => vec![LlmProvider::DeepSeek],
        LlmProvider::Auto => vec![LlmProvider::Gemini, LlmProvider::DeepSeek],
    };
    let mut errors = Vec::new();
    for p in order {
        let res = match p {
            LlmProvider::Gemini if cfg.gemini_api_key.is_some() => gemini(client, cfg, system, user, json_mode).await,
            LlmProvider::DeepSeek if cfg.deepseek_api_key.is_some() => {
                deepseek(client, cfg, system, user, json_mode).await
            }
            _ => continue,
        };
        match res {
            Ok(text) => return Ok(text),
            Err(e) => {
                tracing::warn!(provider = ?p, error = %e, "llm provider failed");
                errors.push(e.to_string());
            }
        }
    }
    Err(AppError::Llm(errors.join(" | ")))
}

async fn gemini(client: &Client, cfg: &Config, system: &str, user: &str, json_mode: bool) -> AppResult<String> {
    let key = cfg.gemini_api_key.as_deref().unwrap_or_default();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        cfg.gemini_model
    );
    let mut generation = json!({ "temperature": if json_mode { 0.85 } else { 0.4 } });
    if json_mode {
        generation["responseMimeType"] = json!("application/json");
    }
    let body = json!({
        "systemInstruction": { "parts": [{ "text": system }] },
        "contents": [{ "role": "user", "parts": [{ "text": user }] }],
        "generationConfig": generation
    });
    let data = post_json_with_retry(client, &url, &[("x-goog-api-key", key)], &body, "gemini").await?;
    let parts = data["candidates"][0]["content"]["parts"]
        .as_array()
        .ok_or_else(|| AppError::Llm(format!("gemini: no content in response: {}", truncate(&data.to_string()))))?;
    // Thinking models may return thought parts first; take the non-thought text.
    let text: String = parts
        .iter()
        .filter(|p| !p["thought"].as_bool().unwrap_or(false))
        .filter_map(|p| p["text"].as_str())
        .collect();
    if text.trim().is_empty() {
        return Err(AppError::Llm("gemini: empty response".into()));
    }
    Ok(text)
}

async fn deepseek(client: &Client, cfg: &Config, system: &str, user: &str, json_mode: bool) -> AppResult<String> {
    let key = cfg.deepseek_api_key.as_deref().unwrap_or_default();
    let mut body = json!({
        "model": cfg.deepseek_model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ],
        "temperature": if json_mode { 0.8 } else { 0.3 }
    });
    if json_mode {
        body["response_format"] = json!({ "type": "json_object" });
    }
    let auth = format!("Bearer {key}");
    let data = post_json_with_retry(
        client,
        "https://api.deepseek.com/chat/completions",
        &[("Authorization", auth.as_str())],
        &body,
        "deepseek",
    )
    .await?;
    data["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| AppError::Llm("deepseek: no content in response".into()))
}

async fn post_json_with_retry(
    client: &Client,
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
    name: &str,
) -> AppResult<Value> {
    let mut delay = std::time::Duration::from_secs(2);
    for attempt in 1..=4 {
        let mut rb = client.post(url).json(body);
        for (k, v) in headers {
            rb = rb.header(*k, *v);
        }
        let resp = match rb.send().await {
            Ok(r) => r,
            Err(e) if attempt < 4 => {
                tracing::warn!(attempt, error = %e, "{name}: network error, retrying");
                tokio::time::sleep(delay).await;
                delay *= 2;
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let status = resp.status();
        if (status.as_u16() == 429 || status.is_server_error()) && attempt < 4 {
            tracing::warn!(attempt, %status, "{name}: transient error, retrying");
            tokio::time::sleep(delay).await;
            delay *= 2;
            continue;
        }
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(AppError::Llm(format!("{name}: HTTP {status}: {}", truncate(&text))));
        }
        return serde_json::from_str(&text).map_err(|e| AppError::Llm(format!("{name}: bad JSON: {e}")));
    }
    unreachable!("loop returns on final attempt")
}

fn truncate(s: &str) -> String {
    s.chars().take(600).collect()
}
