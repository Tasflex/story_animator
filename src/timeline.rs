//! Fits narrated scenes into an exact target duration (default 60.000s).
//!
//! Each scene gets a *slot*: lead-in, narration (optionally sped up a little),
//! and a breathing tail. Slots sum to exactly the target. Scene clips are
//! rendered `slot + transition` long (except the last) so that chained
//! crossfades starting at each slot's start produce a video of exactly
//! `sum(slots)` with no drift:
//!
//!   xfade offset for scene i = start_i
//!   output length            = sum(clip_len) - (n-1)*transition = sum(slot)

use serde::{Deserialize, Serialize};

use crate::tts::Word;

/// Silence before the first words of the video.
const LEAD_FIRST: f64 = 0.6;
/// Silence after a crossfade begins, before the next scene's narration.
const LEAD: f64 = 0.35;
/// Minimum pause after each scene's narration.
const TAIL_MIN: f64 = 0.45;
/// Minimum hold on the final frame after the last word.
const OUTRO_MIN: f64 = 1.8;
/// Max extra breathing room added per scene when narration runs short.
const EXTRA_CAP: f64 = 1.3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneTiming {
    /// Scene start on the final timeline (s).
    pub start: f64,
    /// Time this scene owns on the final timeline.
    pub slot: f64,
    /// Rendered clip length (slot + transition overlap, except last scene).
    pub clip_len: f64,
    /// Narration start, relative to the scene start.
    pub voice_offset: f64,
    /// Narration length after tempo.
    pub voice_len: f64,
    /// Word timings relative to the scene start (tempo applied).
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub total: f64,
    pub tempo: f64,
    pub transition: f64,
    pub scenes: Vec<SceneTiming>,
}

#[derive(Debug)]
pub enum FitError {
    /// Narration cannot fit even at the max tempo; holds the tempo it would need.
    TooLong { needed_tempo: f64, speech_secs: f64 },
}

impl std::fmt::Display for FitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FitError::TooLong { needed_tempo, speech_secs } => write!(
                f,
                "narration is {speech_secs:.1}s and would need {needed_tempo:.2}x speed to fit; shorten the script"
            ),
        }
    }
}

pub fn fit(
    durations: &[f64],
    words: &[Vec<Word>],
    target: f64,
    transition: f64,
    max_tempo: f64,
) -> Result<Timeline, FitError> {
    let n = durations.len();
    assert!(n > 0 && words.len() == n, "fit() needs one duration and word list per scene");
    let speech: f64 = durations.iter().sum();
    let fixed = LEAD_FIRST + (n as f64 - 1.0) * (LEAD + TAIL_MIN) + OUTRO_MIN;

    let mut tempo = 1.0;
    if fixed + speech > target {
        tempo = speech / (target - fixed).max(1.0);
        if tempo > max_tempo {
            return Err(FitError::TooLong { needed_tempo: tempo, speech_secs: speech });
        }
    }
    let spoken = speech / tempo;
    let slack = (target - fixed - spoken).max(0.0);
    // Spread most of the slack as per-scene breathing room; the rest extends the outro.
    let per_scene = (slack * 0.7 / n as f64).min(EXTRA_CAP);
    let outro_extra = slack - per_scene * n as f64;

    let mut scenes = Vec::with_capacity(n);
    let mut start = 0.0;
    for i in 0..n {
        let lead = if i == 0 { LEAD_FIRST } else { LEAD };
        let voice_len = durations[i] / tempo;
        let tail = if i + 1 == n { OUTRO_MIN + per_scene + outro_extra } else { TAIL_MIN + per_scene };
        let slot = lead + voice_len + tail;
        let clip_len = if i + 1 == n { slot } else { slot + transition };
        let ws = words[i]
            .iter()
            .map(|w| Word { w: w.w.clone(), s: lead + w.s / tempo, e: lead + w.e / tempo })
            .collect();
        scenes.push(SceneTiming { start, slot, clip_len, voice_offset: lead, voice_len, words: ws });
        start += slot;
    }
    // Absorb floating-point residue into the last slot so the sum is exact.
    let residue = target - start;
    if let Some(last) = scenes.last_mut() {
        last.slot += residue;
        last.clip_len += residue;
    }
    Ok(Timeline { total: target, tempo, transition, scenes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(n: usize) -> Vec<Vec<Word>> {
        (0..n).map(|_| vec![Word { w: "x".into(), s: 0.1, e: 0.5 }]).collect()
    }

    #[test]
    fn sums_to_target_exactly() {
        let d = [7.2, 6.8, 8.1, 7.5, 6.9, 7.7, 5.1];
        let tl = fit(&d, &w(7), 60.0, 0.5, 1.15).unwrap();
        let sum: f64 = tl.scenes.iter().map(|s| s.slot).sum();
        assert!((sum - 60.0).abs() < 1e-9);
        assert_eq!(tl.tempo, 1.0);
        // output length of the xfade chain
        let out: f64 = tl.scenes.iter().map(|s| s.clip_len).sum::<f64>() - 6.0 * 0.5;
        assert!((out - 60.0).abs() < 1e-9);
        // xfade offsets are the slot starts
        assert!((tl.scenes[1].start - tl.scenes[0].slot).abs() < 1e-9);
    }

    #[test]
    fn speeds_up_long_narration() {
        let d = [9.0; 6]; // 54s of speech
        let tl = fit(&d, &w(6), 60.0, 0.5, 1.15).unwrap();
        assert!(tl.tempo > 1.0 && tl.tempo <= 1.15);
        let sum: f64 = tl.scenes.iter().map(|s| s.slot).sum();
        assert!((sum - 60.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_way_too_long() {
        let d = [12.0; 6];
        assert!(matches!(fit(&d, &w(6), 60.0, 0.5, 1.15), Err(FitError::TooLong { .. })));
    }

    #[test]
    fn short_narration_gets_breathing_room() {
        let d = [5.0; 6];
        let tl = fit(&d, &w(6), 60.0, 0.5, 1.15).unwrap();
        assert!(tl.scenes[0].slot > 0.6 + 5.0 + 0.45);
        let sum: f64 = tl.scenes.iter().map(|s| s.slot).sum();
        assert!((sum - 60.0).abs() < 1e-9);
    }
}
