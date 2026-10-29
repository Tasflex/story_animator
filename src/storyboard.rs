//! Storyboard model + validation.
//!
//! A storyboard is the contract between the writer (LLM or human) and the
//! renderer. `validate` is strict about things that would break timing or
//! pacing (scene count, word budget) and lenient about cosmetic fields, which
//! `normalize` silently repairs to catalog values.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalog::Catalog;

/// Typical ElevenLabs narration pace (words per second) used for budgeting.
pub const WORDS_PER_SEC: f64 = 2.55;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Storyboard {
    pub title: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Optional prompt for generated background music.
    #[serde(default)]
    pub music: Option<String>,
    pub scenes: Vec<Scene>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
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

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub min_words: usize,
    pub max_words: usize,
    pub target_words: usize,
    pub min_scenes: usize,
    pub max_scenes: usize,
}

impl Budget {
    /// Word/scene budget that lands narration at ~88% of the target duration;
    /// the remainder is breathing room, transitions and the outro hold.
    pub fn for_duration(target_secs: f64) -> Self {
        let speech = target_secs * 0.88;
        let target = (speech * WORDS_PER_SEC).round() as usize;
        Budget {
            min_words: (target as f64 * 0.78) as usize,
            max_words: (target as f64 * 1.08) as usize,
            target_words: target,
            min_scenes: ((target_secs / 12.0).round() as usize).max(3),
            max_scenes: ((target_secs / 6.0).round() as usize).max(4),
        }
    }
}

pub fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

impl Storyboard {
    pub fn total_words(&self) -> usize {
        self.scenes.iter().map(|s| word_count(&s.narration)).sum()
    }

    /// Hard problems that require rewriting the storyboard.
    pub fn validate(&self, budget: &Budget) -> Vec<String> {
        let mut problems = Vec::new();
        let n = self.scenes.len();
        if n < budget.min_scenes || n > budget.max_scenes {
            problems.push(format!(
                "use between {} and {} scenes (got {n})",
                budget.min_scenes, budget.max_scenes
            ));
        }
        let words = self.total_words();
        if words < budget.min_words || words > budget.max_words {
            problems.push(format!(
                "total narration must be {}-{} words (aim for {}); it is {words} words — {}",
                budget.min_words,
                budget.max_words,
                budget.target_words,
                if words > budget.max_words { "shorten it" } else { "lengthen it" }
            ));
        }
        for (i, s) in self.scenes.iter().enumerate() {
            let w = word_count(&s.narration);
            if w == 0 {
                problems.push(format!("scene {} has empty narration", i + 1));
            } else if w > 40 {
                problems.push(format!("scene {} narration is {w} words; keep each scene under 35 words", i + 1));
            }
        }
        let customs = self.scenes.iter().filter(|s| s.layout == "custom").count();
        if customs > 2 {
            problems.push(format!("use the \"custom\" layout at most 2 times (got {customs})"));
        }
        for (i, s) in self.scenes.iter().enumerate().filter(|(_, s)| s.layout == "custom") {
            if s.content["brief"].as_str().is_none_or(|b| b.trim().len() < 20) {
                problems.push(format!("scene {} is \"custom\" but has no detailed \"brief\"", i + 1));
            }
            if !s.content["fallback"]["layout"].is_string() {
                problems.push(format!("scene {} is \"custom\" but has no \"fallback\" {{layout, content}}", i + 1));
            }
        }
        for (i, pair) in self.scenes.windows(2).enumerate() {
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

    /// Repair cosmetic fields to valid catalog values (never fails).
    pub fn normalize(&mut self, cat: &Catalog) {
        self.title = clean_line(&self.title, 80);
        if !cat.themes.contains_key(&self.theme) {
            self.theme = "blueprint".into();
        }
        let n = self.scenes.len();
        for (i, s) in self.scenes.iter_mut().enumerate() {
            s.narration = clean_line(&s.narration, 600);
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
        }
    }
}

fn clean_line(s: &str, max_chars: usize) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(max_chars).collect()
}

/// Parse LLM output that should be JSON but may be wrapped in a code fence.
pub fn parse_storyboard(raw: &str) -> Result<Storyboard, String> {
    let t = raw.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t)
        .trim();
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    let start = t.find('{').ok_or("no JSON object found")?;
    let end = t.rfind('}').ok_or("no JSON object found")?;
    serde_json::from_str::<Storyboard>(&t[start..=end]).map_err(|e| format!("invalid storyboard JSON: {e}"))
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
    fn parses_fenced_json() {
        let raw = "```json\n{\"title\":\"T\",\"scenes\":[{\"narration\":\"hi there\"}]}\n```";
        let sb = parse_storyboard(raw).unwrap();
        assert_eq!(sb.scenes.len(), 1);
        assert_eq!(sb.scenes[0].layout, "hero");
    }
}
