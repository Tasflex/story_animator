//! Storyboard writing with an LLM (Gemini first, DeepSeek as fallback).
//!
//! The LLM never writes code. It fills in a JSON storyboard using only the
//! vocabulary from the engine catalog; `Storyboard::validate` checks pacing
//! and a repair round feeds the problems back until it passes.

use reqwest::Client;
use serde_json::{Value, json};

use crate::catalog::Catalog;
use crate::config::{Config, LlmProvider, VisualStyle};
use crate::error::{AppError, AppResult};
use crate::storyboard::{Budget, Storyboard, parse_storyboard};

const MAX_ROUNDS: usize = 3;

pub struct StoryRequest<'a> {
    pub topic: &'a str,
    pub theme: Option<&'a str>,
    pub audience: Option<&'a str>,
    pub portrait: bool,
    pub style: VisualStyle,
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
    let budget = Budget::for_duration(cfg.target_duration);
    let system = system_prompt(cat, &budget, cfg.target_duration, req.portrait, req.style, cfg.custom_scenes);
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
                let problems = sb.validate(&budget);
                tracing::info!(round, words = sb.total_words(), scenes = sb.scenes.len(),
                               problems = problems.len(), "storyboard draft");
                if problems.is_empty() {
                    return Ok(sb);
                }
                if best.as_ref().is_none_or(|(n, _)| problems.len() < *n) {
                    best = Some((problems.len(), sb.clone()));
                }
                user = repair_prompt(req, &raw, &problems);
                last_problems = problems;
            }
            Err(e) => {
                tracing::warn!(round, error = %e, "storyboard unparseable");
                last_problems = vec![e.clone()];
                user = repair_prompt(req, &raw, &last_problems);
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
    s.push_str("Write the storyboard JSON now.");
    s
}

fn repair_prompt(req: &StoryRequest<'_>, previous: &str, problems: &[String]) -> String {
    format!(
        "{}\n\nYour previous storyboard had these problems:\n- {}\n\nPrevious attempt:\n{}\n\n\
         Return the corrected, complete storyboard JSON only.",
        user_prompt(req),
        problems.join("\n- "),
        previous.chars().take(12000).collect::<String>()
    )
}

fn style_direction(style: VisualStyle, custom: bool, latex: bool) -> String {
    match style {
        VisualStyle::Diagram => format!(
            r#"## Style: diagram explainer (3Blue1Brown-like)
- Use theme "blueprint" (default) or "graphite" (maths, physics, geometry). Backgrounds mostly
  "glow" or "blueprint"; cameras "drift", "push_in" or "static".
- Show HOW the idea works, not just what it is. Prefer: equation, graph, vectors, dot_grid,
  network, flow, compare, bar_chart, stat, timeline, cycle, quote, title.
- Use icon_row / hero / list at most 2 times total, and character at most once.
{custom_rule}- Maths goes in LaTeX "tex" fields{latex_note}. In JSON, escape backslashes: "\\frac{{1}}{{2}}".
- Function expressions ("expr") are Python maths of x: x**2, sin(x), exp(-x), sqrt(x)."#,
            custom_rule = if custom {
                "- For 1-2 scenes that need a one-off diagram no layout can show (a machine, a physical\n  \
                 process, geometry being cut and rearranged), use layout \"custom\" with a precise \"brief\"\n  \
                 (objects, positions, colours, what moves on which narration word) and a \"fallback\"\n  \
                 {\"layout\": ..., \"content\": ...} using another layout.\n"
            } else {
                "- Do not use the \"custom\" layout.\n"
            },
            latex_note = if latex { "" } else { " (shown as plain text on this machine)" },
        ),
        VisualStyle::Infographic => r#"## Style: flat infographic explainer (Kurzgesagt-like)
- Use any non-diagram theme (midnight, sunset, ocean, forest, ember, neon, paper, candy, mono).
- Favour icon_row, hero, list, compare, stat, bar_chart, flow, timeline, character, quote, cycle.
- Use "character" scenes for human moments (someone deciding, struggling, discovering).
- Do not use the "custom" layout."#
            .to_string(),
    }
}

fn system_prompt(cat: &Catalog, b: &Budget, target: f64, portrait: bool, style: VisualStyle, custom: bool) -> String {
    let mut layout_map = cat.layouts.clone();
    if !custom || style == VisualStyle::Infographic {
        layout_map.remove("custom");
    }
    let layouts = serde_json::to_string_pretty(&layout_map).unwrap_or_default();
    let direction = style_direction(style, custom, cat.latex);
    let themes = cat
        .themes
        .values()
        .map(|t| format!("  - {}: {}", t.name, t.description))
        .collect::<Vec<_>>()
        .join("\n");
    let backgrounds = cat
        .backgrounds
        .iter()
        .map(|(k, v)| format!("  - {k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n");
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
  "scenes": [
    {{
      "narration": "what the narrator says in this scene",
      "layout": "<layout name>",
      "title": "optional on-screen heading <= 5 words (omit for title/quote layouts)",
      "background": "<background name>",
      "camera": "<camera move>",
      "transition": "<transition INTO this scene>",
      "content": {{ ...fields for the chosen layout... }}
    }}
  ]
}}

## Timing — this is what makes the video exactly {target:.0}s
- {min_s}-{max_s} scenes (aim for 7). Total narration {min_w}-{max_w} words, aim for {tw}.
- Each scene 12-30 words. Narration is read by a TTS voice at ~150 words/minute.
- Spoken English only: no emojis, stage directions, URLs, or symbols. Write numbers
  the way they are spoken ("seventy one percent", "nineteen sixty nine").

## Story
- Arc: hook (surprising question/fact) -> context -> 2-4 escalating insights -> payoff -> memorable closer.
- Scene 1 should be a strong hook; use layout "title" or "hero" or "stat".
- The last scene is a short closer (layout "title" or "quote" works well).
- Conversational, vivid, concrete. Every sentence earns its place.
- Be factually accurate. Prefer well-established facts; if a number or detail is uncertain,
  say it generally rather than inventing precision.

## Visual direction
- Never use the same layout in two consecutive scenes. Use at least 5 different layouts.
- Every visual element should be NAMED in that scene's narration. Put the exact narration
  word that names it into its "cue" field (a single word copied verbatim from the narration,
  e.g. narration "...needs sunlight, water and..." -> cue "sunlight"). The engine reveals
  each element at the moment that word is spoken.
- On-screen text is short (labels <= 3 words). Never put full narration sentences on screen;
  captions are added automatically.
- Keep one theme for the whole video. Vary backgrounds and camera moves for rhythm.
- Video is {orientation}.

{direction}

## Vocabulary
Layouts (name -> purpose and content schema):
{layouts}

Icons (use these exact names; pick the closest match): {icons}

Themes:
{themes}

Backgrounds:
{backgrounds}

Cameras: {cameras}
Transitions: {transitions}
Character looks: {looks}; poses: {poses}; expressions: {expressions}
"#,
        min_s = b.min_scenes,
        max_s = b.max_scenes,
        min_w = b.min_words,
        max_w = b.max_words,
        tw = b.target_words,
        orientation = if portrait { "vertical 9:16 (phone)" } else { "horizontal 16:9" },
        icons = cat.icons.join(", "),
        cameras = cat.cameras.join(", "),
        transitions = cat.transitions.join(", "),
        looks = cat.looks.join(", "),
        poses = cat.poses.join(", "),
        expressions = cat.expressions.join(", "),
        direction = direction,
    )
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

async fn complete(client: &Client, cfg: &Config, system: &str, user: &str, json_mode: bool) -> AppResult<String> {
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
