# Story Animator: notes for AI assistants

## This project runs on the VPS, not on this PC

Production lives on a VPS. This Windows machine is only for editing code.

| | |
|---|---|
| Host | `161.97.129.105` (Ubuntu 24.04, 6 cores, 11 GB RAM) |
| Login | `ssh root@161.97.129.105` (password auth; **ask the user for it, never write it into any file**) |
| Project path | `/root/story_animator` |
| Binary | `/root/story_animator/target/release/story_animator` |
| Python env | `/root/story_animator/venv` (Manim 0.21, Python 3.12) |
| Rust | rustup toolchain in `/root/.cargo` (run `. ~/.cargo/env` first) |
| Config | `/root/story_animator/.env` (chmod 600, holds the API keys) |
| Outputs | `/root/story_animator/storage/runs/<run_id>/final.mp4` |

System packages already installed on the VPS: ffmpeg/ffprobe, build-essential,
pkg-config, libcairo2-dev, libpango1.0-dev, python3-venv, TeX Live (latex base,
recommended, extra, science, cm-super) and dvisvgm. `story_animator doctor`
passes every check.

**Do not touch the other project on the VPS.** `/root/films`, `/root/films-venv`,
`/root/cs` and the scripts and PNGs in `/root` belong to a separate project.

## Running it on the VPS

```bash
cd /root/story_animator
./target/release/story_animator doctor
./target/release/story_animator generate "Why do we dream?" -o dreams.mp4
./target/release/story_animator generate --storyboard examples/base_rate.json --silent -q draft   # free test
```

The **Telegram bot** runs as the systemd service `story-bot`
([docs/TELEGRAM.md](docs/TELEGRAM.md)): it renders the queue in
`storage/bot/queue.json` one video every 24 hours (generate → review → improve
→ package) and sends the results to the owner's chat. Restart it after
deploying bot changes: `systemctl restart story-bot`.

Run and test renders **on the VPS**, not on this PC (the owner's standing
request; this PC's `.env` is stale). Short scene renders in
`<run>/short/scenes/` are cached by spec, so after an engine change delete
them before re-checking a short. The system's memory of what earlier videos got
wrong is `storage/LESSONS.md` ([docs/LEARNING.md](docs/LEARNING.md)). It lives
on the VPS only (deploys exclude `storage/`), so never overwrite it from this PC.

The HTTP API (`serve`) is **not** running. `.env` has `SERVER_ADDR=0.0.0.0:3000`
and the API has no auth. Exposing it publicly would let anyone spend the
Gemini/DeepSeek/ElevenLabs credits, so bind it to `127.0.0.1` or add auth before
running it as a service.

## Deploying code changes from this PC

Edit locally, then push only the source to the VPS. Never upload `target/`,
`venv/`, `storage/`, `.git/` or `.env.bak`: they are Windows build output,
local media or stale config.

This PC has no `sshpass`. Supply the password non-interactively with OpenSSH's
askpass, using a temporary script in the session scratchpad that you delete
afterwards:

```bash
export SSH_ASKPASS=/path/to/askpass.sh SSH_ASKPASS_REQUIRE=force DISPLAY=:0
tar -czf /tmp/sa.tgz --exclude=./target --exclude=./venv --exclude=./storage \
    --exclude=./.git --exclude=./.env --exclude=./.env.bak --exclude='__pycache__' .
scp /tmp/sa.tgz root@161.97.129.105:/root/sa.tgz
ssh root@161.97.129.105 'cd /root/story_animator && tar -xzf /root/sa.tgz && rm /root/sa.tgz \
    && grep -rlI $'"'"'\r'"'"' src engine docs examples bot | xargs -r sed -i "s/\r$//" \
    && . ~/.cargo/env && cargo build --release && ./target/release/story_animator doctor'
```

Notes:
- Exclude `.env` on routine deploys so the VPS config isn't overwritten. Only
  upload it on purpose when the keys or settings change.
- If `engine/requirements.txt` changes, also run
  `./venv/bin/pip install -r engine/requirements.txt` on the VPS.
- Files from Windows may have CRLF line endings, which the `sed` above strips.

See [README.md](README.md) and [docs/](docs/) for how the pipeline itself works.
Before writing any thumbnail, title, description or tags, follow
[docs/PACKAGING.md](docs/PACKAGING.md).
