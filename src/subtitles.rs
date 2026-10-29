//! Burned-in captions (ASS) with the currently spoken word highlighted.
//!
//! Words are grouped into short one-line chunks; each word gets its own event
//! that re-draws the chunk with that word in the theme accent colour.

use std::fmt::Write as _;

use crate::config::Aspect;
use crate::timeline::Timeline;
use crate::tts::Word;

pub struct CaptionStyle<'a> {
    pub font: &'a str,
    pub aspect: Aspect,
    /// "#RRGGBB" highlight colour
    pub highlight: &'a str,
}

fn ass_color(hex: &str) -> String {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return "&H0000D7FF".into();
    }
    format!("&H00{}{}{}", &h[4..6], &h[2..4], &h[0..2]).to_uppercase()
}

fn ts(secs: f64) -> String {
    let cs = (secs.max(0.0) * 100.0).round() as u64;
    format!("{}:{:02}:{:02}.{:02}", cs / 360_000, (cs / 6_000) % 60, (cs / 100) % 60, cs % 100)
}

fn clean(word: &str) -> String {
    word.chars().filter(|c| !matches!(c, '{' | '}' | '\\')).collect()
}

/// Split a scene's words into readable one-line chunks.
fn chunks(words: &[Word], max_chars: usize) -> Vec<&[Word]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut len = 0;
    for (i, w) in words.iter().enumerate() {
        let wl = w.w.chars().count() + 1;
        if len + wl > max_chars && i > start {
            out.push(&words[start..i]);
            start = i;
            len = 0;
        }
        len += wl;
        let ends_clause = w.w.ends_with(['.', '!', '?', ',', ';', ':']);
        if ends_clause && len >= max_chars / 2 {
            out.push(&words[start..=i]);
            start = i + 1;
            len = 0;
        }
    }
    if start < words.len() {
        out.push(&words[start..]);
    }
    out
}

const STYLE_FORMAT: &str = "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, \
    BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, \
    Alignment, MarginL, MarginR, MarginV, Encoding";

fn header(w: u32, h: u32, styles: &[String]) -> String {
    format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {w}\nPlayResY: {h}\nWrapStyle: 2\nScaledBorderAndShadow: yes\n\n\
         [V4+ Styles]\n{STYLE_FORMAT}\n{}\n\n\
         [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
        styles.join("\n")
    )
}

/// Captions for the whole video in its own aspect ratio.
pub fn build(tl: &Timeline, style: &CaptionStyle<'_>) -> String {
    let (w, h, size, margin_v, max_chars) = match style.aspect {
        Aspect::Landscape => (1920, 1080, 62, 54, 36),
        Aspect::Portrait => (1080, 1920, 74, 330, 20),
    };
    let mut s = header(w, h, &[format!(
        "Style: Caption,{},{size},&H00FFFFFF,&H00FFFFFF,&H00101010,&H90000000,-1,0,0,0,100,100,0,0,1,5,1.5,2,80,80,{margin_v},1",
        style.font
    )]);
    caption_events(&mut s, tl, 0..tl.scenes.len(), 0.0, f64::MAX, max_chars, &ass_color(style.highlight));
    s
}

/// Layout of the vertical short (1080x1920): hook title on top, the landscape
/// video in the middle band, captions below it, a call to action at the end.
pub struct ShortStyle<'a> {
    pub font: &'a str,
    pub highlight: &'a str,
    pub title: &'a str,
    pub cta: &'a str,
    /// Top edge of the caption block (px).
    pub captions_y: u32,
}

/// Captions for `scenes`, cut out of the long timeline at `[start, end)` and
/// re-timed to start at zero, for the vertical short.
pub fn build_short(tl: &Timeline, scenes: std::ops::Range<usize>, start: f64, end: f64, st: &ShortStyle<'_>) -> String {
    let hl = ass_color(st.highlight);
    let mut s = header(1080, 1920, &[
        format!("Style: Title,{},70,&H00FFFFFF,&H00FFFFFF,&H00101010,&H00000000,-1,0,0,0,100,100,0,0,1,4,0,8,70,70,250,1", st.font),
        format!(
            "Style: Caption,{},74,&H00FFFFFF,&H00FFFFFF,&H00101010,&H90000000,-1,0,0,0,100,100,0,0,1,5,1.5,8,60,60,{},1",
            st.font, st.captions_y
        ),
        format!("Style: Cta,{},44,&H00FFFFFF,&H00FFFFFF,&H00101010,&H00000000,-1,0,0,0,100,100,0,0,1,3,0,2,70,70,230,1", st.font),
    ]);
    let dur = end - start;
    if !st.title.is_empty() {
        let _ = writeln!(s, "Dialogue: 1,{},{},Title,,0,0,0,,{{\\q0\\fad(300,0)}}{}", ts(0.0), ts(dur), clean(st.title));
    }
    if !st.cta.is_empty() && dur > 6.0 {
        let _ = writeln!(s, "Dialogue: 1,{},{},Cta,,0,0,0,,{{\\fad(250,0)\\c{hl}&}}{}", ts(dur - 3.5), ts(dur), clean(st.cta));
    }
    caption_events(&mut s, tl, scenes, start, end, 20, &hl);
    s
}

/// Word-highlight caption events for `scenes`, shifted by `-offset` and kept
/// before `until` (both in timeline seconds).
fn caption_events(s: &mut String, tl: &Timeline, scenes: std::ops::Range<usize>, offset: f64, until: f64,
                  max_chars: usize, hl: &str) {
    let limit = until - offset - 0.05;
    for sc in &tl.scenes[scenes] {
        let base = sc.start - offset;
        let scene_end = (base + sc.slot - 0.05).min(limit);
        let groups = chunks(&sc.words, max_chars);
        for (gi, g) in groups.iter().enumerate() {
            // Chunks must never overlap, or libass stacks them as two lines.
            let next_start = groups.get(gi + 1).map(|n| base + n[0].s).unwrap_or(scene_end);
            let chunk_end = (base + g[g.len() - 1].e + 0.35).min(next_start).min(scene_end);
            for (wi, word) in g.iter().enumerate() {
                let t0 = (base + word.s).max(0.0);
                let t1 = if wi + 1 < g.len() { (base + g[wi + 1].s).min(chunk_end) } else { chunk_end };
                if t1 <= t0 {
                    continue;
                }
                let text = g
                    .iter()
                    .enumerate()
                    .map(|(k, x)| {
                        if k == wi {
                            format!("{{\\c{hl}&}}{}{{\\c&H00FFFFFF&}}", clean(&x.w))
                        } else {
                            clean(&x.w)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let fade = if wi == 0 { "{\\fad(80,0)}" } else { "" };
                let _ = writeln!(s, "Dialogue: 0,{},{},Caption,,0,0,0,,{fade}{text}", ts(t0), ts(t1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_is_bgr() {
        assert_eq!(ass_color("#FFB703"), "&H0003B7FF");
    }

    #[test]
    fn chunking_respects_length() {
        let words: Vec<Word> = "the quick brown fox jumps over the lazy dog, and then it runs far away"
            .split(' ')
            .enumerate()
            .map(|(i, w)| Word { w: w.into(), s: i as f64, e: i as f64 + 0.5 })
            .collect();
        let c = chunks(&words, 20);
        assert!(c.iter().all(|g| g.iter().map(|w| w.w.len() + 1).sum::<usize>() <= 24));
        assert_eq!(c.iter().map(|g| g.len()).sum::<usize>(), words.len());
    }

    #[test]
    fn short_captions_are_retimed_and_clipped() {
        use crate::timeline::SceneTiming;
        let scene = |start: f64| SceneTiming {
            start,
            slot: 10.0,
            clip_len: 10.5,
            voice_offset: 0.35,
            voice_len: 3.0,
            words: vec![Word { w: "hello".into(), s: 1.0, e: 1.5 }, Word { w: "world.".into(), s: 1.6, e: 2.2 }],
        };
        let tl = Timeline { total: 30.0, tempo: 1.0, transition: 0.5, scenes: vec![scene(0.0), scene(10.0), scene(20.0)] };
        let st = ShortStyle { font: "Poppins", highlight: "#38BDF8", title: "Big {hook}", cta: "Full video", captions_y: 1215 };
        let ass = build_short(&tl, 1..2, 10.0, 20.0, &st);
        assert!(ass.contains("Dialogue: 0,0:00:01.00,"), "{ass}");
        assert!(!ass.contains("0:00:11."), "scene 2 words must not leak in");
        assert!(ass.contains("Big hook") && ass.contains("Full video"));
    }
}
