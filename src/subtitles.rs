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

pub fn build(tl: &Timeline, style: &CaptionStyle<'_>) -> String {
    let (w, h, size, margin_v, max_chars) = match style.aspect {
        Aspect::Landscape => (1920, 1080, 62, 54, 36),
        Aspect::Portrait => (1080, 1920, 74, 330, 20),
    };
    let hl = ass_color(style.highlight);
    let mut s = String::new();
    let _ = write!(
        s,
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {w}\nPlayResY: {h}\nWrapStyle: 2\nScaledBorderAndShadow: yes\n\n\
         [V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, \
         Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, \
         MarginL, MarginR, MarginV, Encoding\n\
         Style: Caption,{font},{size},&H00FFFFFF,&H00FFFFFF,&H00101010,&H90000000,-1,0,0,0,100,100,0,0,1,5,1.5,2,80,80,{margin_v},1\n\n\
         [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
        font = style.font
    );
    for sc in &tl.scenes {
        let scene_end = sc.start + sc.slot - 0.05;
        let groups = chunks(&sc.words, max_chars);
        for (gi, g) in groups.iter().enumerate() {
            // Chunks must never overlap, or libass stacks them as two lines.
            let next_start = groups.get(gi + 1).map(|n| sc.start + n[0].s).unwrap_or(scene_end);
            let chunk_end = (sc.start + g[g.len() - 1].e + 0.35).min(next_start).min(scene_end);
            for (wi, word) in g.iter().enumerate() {
                let t0 = sc.start + word.s;
                let t1 = if wi + 1 < g.len() { sc.start + g[wi + 1].s } else { chunk_end };
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
    s
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
}
