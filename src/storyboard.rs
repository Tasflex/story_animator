//! Storyboard model + validation.
//!
//! A storyboard is the contract between the writer (LLM or human) and the
//! renderer. `validate` is strict about things that would break timing or
//! pacing (scene count, word budget) and lenient about cosmetic fields, which
//! `normalize` silently repairs to catalog values.
//!
//! Long videos are split into chapters (`chapters` holds their names; each
//! scene carries its chapter index). A scene with empty narration is a
//! *silent* scene: it costs no TTS, lasts `hold` seconds and plays under music
//! (chapter cards, and "visual beats" where the animation speaks for itself).
//! Narration may contain ElevenLabs audio tags like `[curious]`; they direct
//! the delivery, are not spoken, and are ignored by word counts and captions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalog::Catalog;

/// Typical ElevenLabs narration pace (words per second) used for budgeting.
pub const WORDS_PER_SEC: f64 = 2.55;
/// Videos at least this long are written chapter by chapter.
pub const LONG_FORM_SECS: f64 = 120.0;
/// Allowed length of a silent scene.
pub const HOLD_MIN: f64 = 2.0;
pub const HOLD_MAX: f64 = 8.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Storyboard {
    pub title: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Optional prompt for generated background music.
    #[serde(default)]
    pub music: Option<String>,
    /// YouTube description (2-3 sentences).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Chapter names, indexed by `Scene::chapter`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chapters: Vec<String>,
    /// Which scenes make the vertical short.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short: Option<ShortPick>,
    pub scenes: Vec<Scene>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortPick {
    /// First and last scene (inclusive) of the segment.
    pub from: usize,
    pub to: usize,
    /// Hook title shown above the video in the short.
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    /// What the narrator says; empty for a silent scene.
    #[serde(default)]
    pub narration: String,
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default = "default_camera")]
    pub camera: String,
    #[serde(default = "default_transition")]
    pub transition: String,
    /// Length of a silent scene in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<usize>,
    #[serde(default)]
    pub content: Value,
}

fn default_theme() -> String {
    "blueprint".into()
}
fn default_layout() -> String {
    "hero".into()
}
fn default_background() -> String {
    "gradient".into()
}
fn default_camera() -> String {
    "drift".into()
}
fn default_transition() -> String {
    "fade".into()
}

impl Scene {
    pub fn is_silent(&self) -> bool {
        word_count(&self.narration) == 0
    }

    /// Seconds a silent scene lasts (clamped); 0 for narrated scenes.
    pub fn hold_secs(&self) -> f64 {
        if self.is_silent() { self.hold.unwrap_or(3.0).clamp(HOLD_MIN, HOLD_MAX) } else { 0.0 }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub min_words: usize,
    pub max_words: usize,
    pub target_words: usize,
    pub min_scenes: usize,
    pub max_scenes: usize,
    pub max_custom: usize,
    pub max_silent: usize,
}

impl Budget {
    /// Word/scene budget for a whole video. Narration fills ~88% of a short
    /// video; long videos leave more room for chapter cards and visual beats.
    pub fn for_duration(target_secs: f64) -> Self {
        let share = if target_secs >= LONG_FORM_SECS { 0.80 } else { 0.88 };
        let target = (target_secs * share * WORDS_PER_SEC).round() as usize;
        Budget {
            min_words: (target as f64 * 0.78) as usize,
            max_words: (target as f64 * 1.08) as usize,
            target_words: target,
            min_scenes: ((target_secs / 12.0).round() as usize).max(3),
            max_scenes: ((target_secs / 6.0).round() as usize).max(4),
            max_custom: 2,
            max_silent: (target_secs / 40.0) as usize,
        }
    }

    /// Budget for one chapter of a long video.
    pub fn for_chapter(target_words: usize) -> Self {
        let t = target_words.max(20);
        Budget {
            min_words: (t as f64 * 0.85) as usize,
            max_words: (t as f64 * 1.12).ceil() as usize,
            target_words: t,
            min_scenes: (t / 30).max(1),
            max_scenes: (t / 13).max(2) + 1,
            max_custom: 1,
            max_silent: 1,
        }
    }

    pub fn scaled(self, k: f64) -> Self {
        let s = |v: usize| ((v as f64) * k).round() as usize;
        Budget { min_words: s(self.min_words), max_words: s(self.max_words), target_words: s(self.target_words), ..self }
    }
}

/// Narration with audio tags (`[excited]`) removed: what is actually spoken.
pub fn spoken(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn word_count(s: &str) -> usize {
    spoken(s).split_whitespace().count()
}

impl Storyboard {
    pub fn total_words(&self) -> usize {
        self.scenes.iter().map(|s| word_count(&s.narration)).sum()
    }

    /// Hard problems that require rewriting the storyboard.
    pub fn validate(&self, budget: &Budget) -> Vec<String> {
        let mut problems = validate_scenes(&self.scenes, budget);
        if let Some(sh) = &self.short
            && (sh.from > sh.to || sh.to >= self.scenes.len())
        {
            problems.push(format!("short range {}..{} is outside the {} scenes", sh.from, sh.to, self.scenes.len()));
        }
        problems
    }

    /// Repair cosmetic fields to valid catalog values (never fails).
    pub fn normalize(&mut self, cat: &Catalog) {
        self.title = clean_line(&self.title, 80);
        if !cat.themes.contains_key(&self.theme) {
            self.theme = "blueprint".into();
        }
        normalize_scenes(&mut self.scenes, cat);
        if let Some(sh) = &mut self.short {
            sh.title = clean_line(&sh.title, 70);
        }
    }
}

/// Pacing checks shared by whole storyboards and single chapters.
pub fn validate_scenes(scenes: &[Scene], budget: &Budget) -> Vec<String> {
    let mut problems = Vec::new();
    let n = scenes.len();
    if n < budget.min_scenes || n > budget.max_scenes {
        problems.push(format!("use between {} and {} scenes (got {n})", budget.min_scenes, budget.max_scenes));
    }
    let words: usize = scenes.iter().map(|s| word_count(&s.narration)).sum();
    if words < budget.min_words || words > budget.max_words {
        problems.push(format!(
            "total narration must be {}-{} spoken words (aim for {}; audio tags don't count); it is {words} words — {}",
            budget.min_words,
            budget.max_words,
            budget.target_words,
            if words > budget.max_words { "shorten it" } else { "lengthen it" }
        ));
    }
    for (i, s) in scenes.iter().enumerate() {
        let w = word_count(&s.narration);
        if w > 40 {
            problems.push(format!("scene {} narration is {w} words; keep each scene under 35 words", i + 1));
        }
        if w == 0 && s.layout != "chapter" && s.hold.is_none() {
            problems.push(format!("scene {} has no narration; a silent visual beat needs \"hold\" (seconds)", i + 1));
        }
        let tags = s.narration.matches('[').count();
        if tags > 3 {
            problems.push(format!("scene {} has {tags} audio tags; use at most 2 per scene", i + 1));
        }
    }
    let silent = scenes.iter().filter(|s| s.is_silent() && s.layout != "chapter").count();
    if silent > budget.max_silent {
        problems.push(format!("use at most {} silent visual beat(s) (got {silent})", budget.max_silent));
    }
    let customs = scenes.iter().filter(|s| s.layout == "custom").count();
    if customs > budget.max_custom {
        problems.push(format!("use the \"custom\" layout at most {} times (got {customs})", budget.max_custom));
    }
    for (i, s) in scenes.iter().enumerate().filter(|(_, s)| s.layout == "custom") {
        if s.content["brief"].as_str().is_none_or(|b| b.trim().len() < 20) {
            problems.push(format!("scene {} is \"custom\" but has no detailed \"brief\"", i + 1));
        }
        if !s.content["fallback"]["layout"].is_string() {
            problems.push(format!("scene {} is \"custom\" but has no \"fallback\" {{layout, content}}", i + 1));
        }
    }
    for (i, pair) in scenes.windows(2).enumerate() {
        if pair[0].layout == pair[1].layout {
            problems.push(format!(
                "scenes {} and {} both use layout '{}'; never repeat a layout back-to-back",
                i + 1,
                i + 2,
                pair[0].layout
            ));
        }
    }
    problems
}

pub fn normalize_scenes(scenes: &mut [Scene], cat: &Catalog) {
    let n = scenes.len();
    for (i, s) in scenes.iter_mut().enumerate() {
        s.narration = clean_line(&s.narration, 700);
        s.layout = s.layout.trim().to_lowercase();
        if !cat.layouts.contains_key(&s.layout) {
            s.layout = if i == 0 || i + 1 == n { "title".into() } else { "hero".into() };
        }
        s.background = s.background.trim().to_lowercase();
        if !cat.backgrounds.contains_key(&s.background) {
            s.background = "gradient".into();
        }
        s.camera = s.camera.trim().to_lowercase();
        if !cat.cameras.contains(&s.camera) {
            s.camera = "drift".into();
        }
        s.transition = s.transition.trim().to_lowercase();
        if !cat.transitions.contains(&s.transition) {
            s.transition = "fade".into();
        }
        if !s.content.is_object() {
            s.content = Value::Object(Default::default());
        }
        if let Some(t) = &s.title {
            let t = clean_line(t, 60);
            s.title = (!t.is_empty()).then_some(t);
        }
        s.hold = if s.is_silent() { Some(s.hold.unwrap_or(3.0).clamp(HOLD_MIN, HOLD_MAX)) } else { None };
    }
}

fn clean_line(s: &str, max_chars: usize) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(max_chars).collect()
}

/// Strip a code fence and anything around the outermost JSON object.
pub fn json_body(raw: &str) -> Result<&str, String> {
    let t = raw.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t).trim();
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    let start = t.find('{').ok_or("no JSON object found")?;
    let end = t.rfind('}').ok_or("no JSON object found")?;
    Ok(&t[start..=end])
}

/// Parse LLM output that should be JSON but may be wrapped in a code fence.
pub fn parse_storyboard(raw: &str) -> Result<Storyboard, String> {
    serde_json::from_str::<Storyboard>(json_body(raw)?).map_err(|e| format!("invalid storyboard JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_for_60s() {
        let b = Budget::for_duration(60.0);
        assert!(b.target_words > 120 && b.target_words < 150, "{b:?}");
        assert_eq!(b.min_scenes, 5);
        assert_eq!(b.max_scenes, 10);
    }

    #[test]
    fn budget_for_5_minutes() {
        let b = Budget::for_duration(300.0);
        assert!(b.target_words > 560 && b.target_words < 660, "{b:?}");
        assert_eq!(b.min_scenes, 25);
        assert_eq!(b.max_scenes, 50);
    }

    #[test]
    fn audio_tags_are_not_spoken() {
        assert_eq!(spoken("[curious] Ever wondered [short pause] why?"), "Ever wondered why?");
        assert_eq!(word_count("[excited] Here it is!"), 3);
        assert_eq!(word_count("[whispers]"), 0);
    }

    #[test]
    fn silent_scene_hold_is_clamped() {
        let s: Scene = serde_json::from_str(r#"{"narration":"","hold":30}"#).unwrap();
        assert!(s.is_silent());
        assert_eq!(s.hold_secs(), HOLD_MAX);
    }

    #[test]
    fn parses_fenced_json() {
        let raw = "```json\n{\"title\":\"T\",\"scenes\":[{\"narration\":\"hi there\"}]}\n```";
        let sb = parse_storyboard(raw).unwrap();
        assert_eq!(sb.scenes.len(), 1);
        assert_eq!(sb.scenes[0].layout, "hero");
    }
}
