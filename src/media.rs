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
    pub voices: &'a [PathBuf],
    pub transitions: &'a [String],
    pub music: Option<&'a Path>,
    pub subtitles: Option<&'a Path>,
    pub fps: u32,
    pub out: &'a Path,
}

/// Stitch scene clips with crossfades, lay narration at exact offsets, duck
/// music under the voice, burn captions, and cut to exactly `timeline.total`.
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
    for a in inp.voices {
        cmd.arg("-i").arg(a);
    }
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
    match inp.subtitles {
        Some(s) => f.push_str(&format!(
            "[{last}]subtitles=filename='{}':fontsdir='{}',format=yuv420p[vout];",
            filter_path(cfg, s),
            filter_path(cfg, &cfg.fonts_dir())
        )),
        None => f.push_str(&format!("[{last}]format=yuv420p[vout];")),
    }

    // --- audio: each narration clip at its absolute offset
    for (i, sc) in tl.scenes.iter().enumerate() {
        let tempo = if (tl.tempo - 1.0).abs() > 0.005 { format!(",atempo={:.4}", tl.tempo) } else { String::new() };
        let delay_ms = ((sc.start + sc.voice_offset) * 1000.0).round() as u64;
        f.push_str(&format!(
            "[{k}:a]aformat=sample_rates=48000:channel_layouts=stereo{tempo},adelay=delays={delay_ms}:all=1[a{i}];",
            k = n + i
        ));
    }
    let ains: String = (0..n).map(|i| format!("[a{i}]")).collect();
    f.push_str(&format!(
        "{ains}amix=inputs={n}:normalize=0:dropout_transition=0,apad,atrim=0:{total:.4},\
         loudnorm=I=-15:TP=-1.5:LRA=11,aresample=48000[voice];"
    ));
    if inp.music.is_some() {
        let fade_out = (total - 3.0).max(0.0);
        f.push_str(&format!(
            "[{m}:a]aformat=sample_rates=48000:channel_layouts=stereo,atrim=0:{total:.4},asetpts=PTS-STARTPTS,\
             volume={vol:.3},afade=t=in:d=1.5,afade=t=out:st={fade_out:.3}:d=3[mus];\
             [voice]asplit=2[vo1][vo2];\
             [mus][vo2]sidechaincompress=threshold=0.02:ratio=5:attack=30:release=500[duck];\
             [vo1][duck]amix=inputs=2:normalize=0:duration=first[aout]",
            m = 2 * n,
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
        .args(["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p"])
        .args(["-r", &inp.fps.to_string()])
        .args(["-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"])
        .args(["-t", &format!("{total:.4}")])
        .arg(inp.out);
    run_cmd(&mut cmd, 1800)
        .await
        .map_err(|e| AppError::Ffmpeg(format!("compose failed: {e}")))?;
    Ok(())
}
