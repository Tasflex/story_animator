//! ffmpeg / ffprobe / subprocess helpers and the final compositor.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::timeline::Timeline;

/// Run a command with a timeout; return stdout, or an error carrying the
/// tail of stderr (the useful part of ffmpeg / Python tracebacks).
pub async fn run_cmd(cmd: &mut Command, timeout_secs: u64) -> AppResult<String> {
    cmd.kill_on_drop(true).stdin(std::process::Stdio::null());
    let fut = cmd.output();
    let out = match tokio::time::timeout(Duration::from_secs(timeout_secs), fut).await {
        Ok(r) => r.map_err(|e| AppError::Render(format!("failed to spawn {:?}: {e}", cmd.as_std().get_program())))?,
        Err(_) => {
            return Err(AppError::Render(format!(
                "{:?} timed out after {timeout_secs}s",
                cmd.as_std().get_program()
            )));
        }
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        return Err(AppError::Render(format!(
            "{:?} exited with {}: {}",
            cmd.as_std().get_program(),
            out.status,
            tail(&format!("{stdout}\n{stderr}"), 40)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn tail(s: &str, lines: usize) -> String {
    let v: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    v[v.len().saturating_sub(lines)..].join("\n")
}

pub async fn probe_duration(cfg: &Config, path: &Path) -> AppResult<f64> {
    let out = run_cmd(
        Command::new(&cfg.ffprobe_bin)
            .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
            .arg(path),
        60,
    )
    .await
    .map_err(|e| AppError::Ffmpeg(format!("ffprobe {}: {e}", path.display())))?;
    let d: f64 = out
        .trim()
        .parse()
        .map_err(|e| AppError::Ffmpeg(format!("ffprobe duration parse for {}: {e}", path.display())))?;
    if !(d.is_finite() && d > 0.0) {
        return Err(AppError::Ffmpeg(format!("invalid duration {d} for {}", path.display())));
    }
    Ok(d)
}

pub async fn silent_audio(cfg: &Config, secs: f64, out: &Path) -> AppResult<()> {
    run_cmd(
        Command::new(&cfg.ffmpeg_bin)
            .args(["-y", "-v", "error", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono", "-t"])
            .arg(format!("{secs:.3}"))
            .arg(out),
        60,
    )
    .await
    .map(|_| ())
    .map_err(|e| AppError::Ffmpeg(e.to_string()))
}

/// Path relative to the project root with forward slashes — safe to embed in
/// an ffmpeg filtergraph (no drive-letter colon to escape).
fn filter_path(cfg: &Config, p: &Path) -> String {
    let rel = p.strip_prefix(&cfg.root).unwrap_or(p);
    let s = rel.to_string_lossy().replace('\\', "/");
    s.replace('\'', "")
}

pub struct ComposeInput<'a> {
    pub timeline: &'a Timeline,
    pub clips: &'a [PathBuf],
    /// Narration per scene; `None` for silent scenes.
    pub voices: &'a [Option<PathBuf>],
    pub transitions: &'a [String],
    pub music: Option<&'a Path>,
    pub fps: u32,
    pub out: &'a Path,
}

/// Stitch scene clips with crossfades, lay narration at exact offsets, duck
/// music under the voice, and cut to exactly `timeline.total`. The result is
/// the clean master (no captions) that the long video and the short are cut from.
pub async fn compose(cfg: &Config, inp: ComposeInput<'_>) -> AppResult<()> {
    let tl = inp.timeline;
    let n = tl.scenes.len();
    if n == 0 || inp.clips.len() != n || inp.voices.len() != n {
        return Err(AppError::Ffmpeg("compose: clip/voice/scene count mismatch".into()));
    }
    let total = tl.total;
    let x = tl.transition;
    let mut cmd = Command::new(&cfg.ffmpeg_bin);
    cmd.current_dir(&cfg.root).args(["-y", "-v", "error"]);
    for c in inp.clips {
        cmd.arg("-i").arg(c);
    }
    // (scene index, input index) for every narrated scene
    let mut voiced = Vec::new();
    for (i, a) in inp.voices.iter().enumerate() {
        if let Some(a) = a {
            voiced.push((i, n + voiced.len()));
            cmd.arg("-i").arg(a);
        }
    }
    let music_input = n + voiced.len();
    if let Some(m) = inp.music {
        cmd.args(["-stream_loop", "-1"]).arg("-i").arg(m);
    }

    let mut f = String::new();
    // --- video: normalise each clip to its exact length, then crossfade-chain
    for (i, sc) in tl.scenes.iter().enumerate() {
        f.push_str(&format!(
            "[{i}:v]fps={fps},format=yuv420p,settb=AVTB,tpad=stop_mode=clone:stop_duration=3,\
             trim=duration={len:.4},setpts=PTS-STARTPTS[v{i}];",
            fps = inp.fps,
            len = sc.clip_len
        ));
    }
    let mut last = "v0".to_string();
    if x > 0.0 {
        for i in 1..n {
            let tr = inp.transitions.get(i).map(String::as_str).unwrap_or("fade");
            f.push_str(&format!(
                "[{last}][v{i}]xfade=transition={tr}:duration={x:.3}:offset={off:.4}[x{i}];",
                off = tl.scenes[i].start
            ));
            last = format!("x{i}");
        }
    } else if n > 1 {
        let ins: String = (0..n).map(|i| format!("[v{i}]")).collect();
        f.push_str(&format!("{ins}concat=n={n}:v=1:a=0[xc];"));
        last = "xc".into();
    }
    f.push_str(&format!("[{last}]format=yuv420p[vout];"));

    // --- audio: each narration clip at its absolute offset
    for &(i, k) in &voiced {
        let sc = &tl.scenes[i];
        let tempo = if (tl.tempo - 1.0).abs() > 0.005 { format!(",atempo={:.4}", tl.tempo) } else { String::new() };
        let delay_ms = ((sc.start + sc.voice_offset) * 1000.0).round() as u64;
        f.push_str(&format!(
            "[{k}:a]aformat=sample_rates=48000:channel_layouts=stereo{tempo},adelay=delays={delay_ms}:all=1[a{i}];"
        ));
    }
    if voiced.is_empty() {
        f.push_str(&format!("anullsrc=r=48000:cl=stereo,atrim=0:{total:.4}[voice];"));
    } else {
        let ains: String = voiced.iter().map(|(i, _)| format!("[a{i}]")).collect();
        f.push_str(&format!(
            "{ains}amix=inputs={nv}:normalize=0:dropout_transition=0,apad,atrim=0:{total:.4},\
             loudnorm=I=-15:TP=-1.5:LRA=11,aresample=48000[voice];",
            nv = voiced.len()
        ));
    }
    if inp.music.is_some() {
        // Tracks arrive at very different levels, so each is normalised to
        // -16 LUFS first; MUSIC_VOLUME then sets the bed relative to that
        // (0.18 ≈ -31 LUFS, about 16 dB under the -15 LUFS voice).
        let fade_out = (total - 3.0).max(0.0);
        f.push_str(&format!(
            "[{m}:a]aformat=sample_rates=48000:channel_layouts=stereo,atrim=0:{total:.4},asetpts=PTS-STARTPTS,\
             loudnorm=I=-16:TP=-2:LRA=11,aresample=48000,\
             volume={vol:.3},afade=t=in:d=1.5,afade=t=out:st={fade_out:.3}:d=3[mus];\
             [voice]asplit=2[vo1][vo2];\
             [mus][vo2]sidechaincompress=threshold=0.02:ratio=5:attack=30:release=500[duck];\
             [vo1][duck]amix=inputs=2:normalize=0:duration=first[aout]",
            m = music_input,
            vol = cfg.music_volume
        ));
    } else {
        f.push_str("[voice]anull[aout]");
    }

    // Saved for debugging; passed inline (`-filter_complex_script` is deprecated in ffmpeg 7+).
    tokio::fs::write(inp.out.with_extension("filtergraph.txt"), &f).await?;
    cmd.arg("-filter_complex")
        .arg(&f)
        .args(["-map", "[vout]", "-map", "[aout]"])
        // Near-lossless: the master is re-encoded once more for each output.
        .args(["-c:v", "libx264", "-preset", "fast", "-crf", "14", "-pix_fmt", "yuv420p"])
        .args(["-r", &inp.fps.to_string()])
        .args(["-c:a", "aac", "-b:a", "256k", "-movflags", "+faststart"])
        .args(["-t", &format!("{total:.4}")])
        .arg(inp.out);
    run_cmd(&mut cmd, 3600)
        .await
        .map_err(|e| AppError::Ffmpeg(format!("compose failed: {e}")))?;
    Ok(())
}

fn subtitles_filter(cfg: &Config, subs: &Path) -> String {
    format!(
        "subtitles=filename='{}':fontsdir='{}'",
        filter_path(cfg, subs),
        filter_path(cfg, &cfg.fonts_dir())
    )
}

/// The YouTube video: the master with captions burned in.
pub async fn burn_captions(cfg: &Config, master: &Path, subs: &Path, out: &Path) -> AppResult<()> {
    let mut cmd = Command::new(&cfg.ffmpeg_bin);
    cmd.current_dir(&cfg.root)
        .args(["-y", "-v", "error", "-i"])
        .arg(master)
        .arg("-vf")
        .arg(format!("{},format=yuv420p", subtitles_filter(cfg, subs)))
        .args(["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p"])
        .args(["-c:a", "copy", "-movflags", "+faststart"])
        .arg(out);
    run_cmd(&mut cmd, 3600)
        .await
        .map_err(|e| AppError::Ffmpeg(format!("caption burn failed: {e}")))?;
    Ok(())
}

/// Where the landscape video sits inside the 1080x1920 short. The bottom of
/// the landscape frame is reserved for captions (empty in the master), so it
/// is cropped off and the panel gets the full width.
pub const SHORT_PANEL_TOP: u32 = 600;
pub const SHORT_PANEL_H: u32 = 534;
const SHORT_KEEP: f64 = 0.88;

pub struct ShortInput<'a> {
    pub master: &'a Path,
    /// Segment of the master, in seconds.
    pub start: f64,
    pub duration: f64,
    pub subtitles: &'a Path,
    /// "#RRGGBB" for the hairlines framing the video.
    pub accent: &'a str,
    pub out: &'a Path,
}

/// The vertical short, cut from the same master: a blurred, darkened copy
/// fills the 9:16 frame, the landscape video sits full-width in the middle
/// band, and the hook title, captions and call to action go around it.
pub async fn make_short(cfg: &Config, inp: ShortInput<'_>) -> AppResult<()> {
    let d = inp.duration;
    let accent = format!("0x{}", inp.accent.trim_start_matches('#'));
    let (top, h) = (SHORT_PANEL_TOP, SHORT_PANEL_H);
    let f = format!(
        "[0:v]split=2[b][f];\
         [b]scale=-2:640,crop=360:640,boxblur=10:2,scale=1080:1920,setsar=1,eq=brightness=-0.12:saturation=1.15[bg];\
         [f]crop=iw:trunc(ih*{SHORT_KEEP}/2)*2:0:0,scale=1080:{h},setsar=1[fg];\
         [bg][fg]overlay=0:{top},\
         drawbox=x=0:y={above}:w=1080:h=4:color={accent}@0.85:t=fill,\
         drawbox=x=0:y={below}:w=1080:h=4:color={accent}@0.85:t=fill,\
         {subs},fade=t=in:st=0:d=0.3,fade=t=out:st={fo:.3}:d=0.4,format=yuv420p[v];\
         [0:a]afade=t=in:st=0:d=0.3,afade=t=out:st={ao:.3}:d=0.8[a]",
        above = top - 4,
        below = top + h,
        subs = subtitles_filter(cfg, inp.subtitles),
        fo = (d - 0.4).max(0.0),
        ao = (d - 0.8).max(0.0),
    );
    tokio::fs::write(inp.out.with_extension("filtergraph.txt"), &f).await?;
    let mut cmd = Command::new(&cfg.ffmpeg_bin);
    cmd.current_dir(&cfg.root)
        .args(["-y", "-v", "error", "-ss", &format!("{:.4}", inp.start), "-t", &format!("{d:.4}"), "-i"])
        .arg(inp.master)
        .arg("-filter_complex")
        .arg(&f)
        .args(["-map", "[v]", "-map", "[a]"])
        .args(["-c:v", "libx264", "-preset", "medium", "-crf", "19", "-pix_fmt", "yuv420p"])
        .args(["-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"])
        .args(["-t", &format!("{d:.4}")])
        .arg(inp.out);
    run_cmd(&mut cmd, 1800)
        .await
        .map_err(|e| AppError::Ffmpeg(format!("short failed: {e}")))?;
    Ok(())
}
