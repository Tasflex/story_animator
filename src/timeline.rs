//! Fits narrated scenes into an exact target duration (default 300.000s).
//!
//! Each narrated scene gets a *slot*: lead-in, narration (optionally sped up a
//! little), and a breathing tail. A silent scene's slot is its `hold`. Slots
//! sum to exactly the target. Scene clips are rendered `slot + transition`
//! long (except the last) so that chained crossfades starting at each slot's
//! start produce a video of exactly `sum(slots)` with no drift:
//!
//!   xfade offset for scene i = start_i
//!   output length            = sum(clip_len) - (n-1)*transition = sum(slot)
//!
//! Spare time is spread as breathing room: first a little after every narrated
//! scene, then on silent scenes (they let visuals land), then the outro, then
//! evenly again. If the narration is far too short to fill the target, that is
//! an error rather than a video full of dead air.

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
/// Extra breathing room per narrated scene in the first pass.
const EXTRA_CAP: f64 = 1.3;
/// Extra time a silent scene may absorb.
const SILENT_EXTRA_CAP: f64 = 2.5;
/// Extra time the outro may absorb.
const OUTRO_EXTRA_CAP: f64 = 4.0;
/// Most extra time per narrated scene before the script counts as too short.
const SPREAD_LIMIT: f64 = 3.0;

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
    /// Narration length after tempo (0 for silent scenes).
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
    /// Narration leaves too much of the target empty; `fill` is speech / needed speech.
    TooShort { fill: f64, speech_secs: f64 },
}

impl std::fmt::Display for FitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FitError::TooLong { needed_tempo, speech_secs } => write!(
                f,
                "narration is {speech_secs:.1}s and would need {needed_tempo:.2}x speed to fit; shorten the script"
            ),
            FitError::TooShort { fill, speech_secs } => write!(
                f,
                "narration is only {speech_secs:.1}s ({:.0}% of what the target needs); lengthen the script",
                fill * 100.0
            ),
        }
    }
}

/// `durations[i]` is scene i's narration length (0 for silent scenes) and
/// `holds[i]` a silent scene's length (0 for narrated scenes).
pub fn fit(
    durations: &[f64],
    words: &[Vec<Word>],
    holds: &[f64],
    target: f64,
    transition: f64,
    max_tempo: f64,
) -> Result<Timeline, FitError> {
    let n = durations.len();
    assert!(n > 0 && words.len() == n && holds.len() == n, "fit() needs one duration, word list and hold per scene");
    let silent: Vec<bool> = durations.iter().map(|d| *d <= 0.0).collect();
    let narrated = silent.iter().filter(|s| !**s).count().max(1) as f64;
    let n_silent = silent.iter().filter(|s| **s).count() as f64;
    let speech: f64 = durations.iter().sum();

    // Fixed time: leads and tails around narration, silent holds, the outro.
    let mut fixed = 0.0;
    for i in 0..n {
        let last = i + 1 == n;
        fixed += if silent[i] {
            if last { holds[i].max(OUTRO_MIN) } else { holds[i] }
        } else {
            let lead = if i == 0 { LEAD_FIRST } else { LEAD };
            lead + if last { OUTRO_MIN } else { TAIL_MIN }
        };
    }

    let mut tempo = 1.0;
    if fixed + speech > target {
        tempo = speech / (target - fixed).max(1.0);
        if tempo > max_tempo {
            return Err(FitError::TooLong { needed_tempo: tempo, speech_secs: speech });
        }
    }
    let spoken = speech / tempo;
    let mut slack = (target - fixed - spoken).max(0.0);

    let per_narrated = (slack * 0.7 / narrated).min(EXTRA_CAP);
    slack -= per_narrated * narrated;
    let per_silent = if n_silent > 0.0 { (slack / n_silent).min(SILENT_EXTRA_CAP) } else { 0.0 };
    slack -= per_silent * n_silent;
    let outro_extra = slack.min(OUTRO_EXTRA_CAP);
    slack -= outro_extra;
    let spread = slack / narrated;
    if spread > SPREAD_LIMIT && speech > 0.0 {
        let usable = target - fixed - (EXTRA_CAP + SPREAD_LIMIT) * narrated - SILENT_EXTRA_CAP * n_silent - OUTRO_EXTRA_CAP;
        return Err(FitError::TooShort { fill: speech / usable.max(1.0), speech_secs: speech });
    }
    let extra_narrated = per_narrated + spread;

    let mut scenes = Vec::with_capacity(n);
    let mut start = 0.0;
    for i in 0..n {
        let last = i + 1 == n;
        let (slot, lead, voice_len) = if silent[i] {
            let base = if last { holds[i].max(OUTRO_MIN) + outro_extra } else { holds[i] };
            (base + per_silent, 0.0, 0.0)
        } else {
            let lead = if i == 0 { LEAD_FIRST } else { LEAD };
            let voice_len = durations[i] / tempo;
            let tail = if last { OUTRO_MIN + outro_extra } else { TAIL_MIN } + extra_narrated;
            (lead + voice_len + tail, lead, voice_len)
        };
        let clip_len = if last { slot } else { slot + transition };
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

    fn total(tl: &Timeline) -> f64 {
        tl.scenes.iter().map(|s| s.slot).sum()
    }

    #[test]
    fn sums_to_target_exactly() {
        let d = [7.2, 6.8, 8.1, 7.5, 6.9, 7.7, 5.1];
        let tl = fit(&d, &w(7), &[0.0; 7], 60.0, 0.5, 1.15).unwrap();
        assert!((total(&tl) - 60.0).abs() < 1e-9);
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
        let tl = fit(&d, &w(6), &[0.0; 6], 60.0, 0.5, 1.15).unwrap();
        assert!(tl.tempo > 1.0 && tl.tempo <= 1.15);
        assert!((total(&tl) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_way_too_long() {
        let d = [12.0; 6];
        assert!(matches!(fit(&d, &w(6), &[0.0; 6], 60.0, 0.5, 1.15), Err(FitError::TooLong { .. })));
    }

    #[test]
    fn short_narration_gets_breathing_room() {
        let d = [5.0; 6];
        let tl = fit(&d, &w(6), &[0.0; 6], 60.0, 0.5, 1.15).unwrap();
        assert!(tl.scenes[0].slot > 0.6 + 5.0 + 0.45);
        assert!((total(&tl) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn five_minutes_with_silent_scenes() {
        // 30 narrated scenes of ~7.6s plus 5 silent chapter cards / beats.
        let mut d = vec![7.6; 35];
        let mut h = vec![0.0; 35];
        for k in [6, 13, 20, 27, 33] {
            d[k] = 0.0;
            h[k] = 3.2;
        }
        let words: Vec<Vec<Word>> = d.iter().map(|x| if *x > 0.0 { w(1).remove(0) } else { vec![] }).collect();
        let tl = fit(&d, &words, &h, 300.0, 0.5, 1.15).unwrap();
        assert!((total(&tl) - 300.0).abs() < 1e-9);
        let card = &tl.scenes[6];
        assert!(card.voice_len == 0.0 && card.words.is_empty() && card.slot >= 3.2);
    }

    #[test]
    fn rejects_way_too_short() {
        let d = [5.0; 6];
        assert!(matches!(fit(&d, &w(6), &[0.0; 6], 300.0, 0.5, 1.15), Err(FitError::TooShort { .. })));
    }
}
