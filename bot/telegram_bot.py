"""Telegram bot: a render queue on the VPS that delivers finished videos to your chat.

    venv/bin/python bot/telegram_bot.py            # run the bot (systemd: story-bot.service)
    venv/bin/python bot/telegram_bot.py seed FILE  # queue one topic per line, RENDER_GAP_HOURS apart

Every queued topic is rendered when its time comes (one at a time), then
self-reviewed, improved if it scores below the best earlier video (kept only
if the fix scores higher), and packaged. The bot sends you the video, the
9:16 short, the thumbnail, the title/description/tags and the review. Write to it in plain
words to change the last video; see /help for the rest. Only standard library.

Settings (.env): TELEGRAM_BOT_TOKEN, TELEGRAM_PAIR_CODE (send "/start <code>"
once to link your chat) or TELEGRAM_CHAT_ID, BOT_TIMEZONE, RENDER_GAP_HOURS,
AUTO_IMPROVE (read by `story_animator improve`).
State lives in storage/bot/. See docs/TELEGRAM.md.
"""

from __future__ import annotations

import json
import logging
import re
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid
from collections import deque
from datetime import datetime, timedelta
from pathlib import Path
from zoneinfo import ZoneInfo

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target" / "release" / "story_animator"
STATE_DIR = ROOT / "storage" / "bot"
RUNS = ROOT / "storage" / "runs"
QUEUE = STATE_DIR / "queue.json"
STATE = STATE_DIR / "state.json"
TELEGRAM_LIMIT = 49 * 1024 * 1024  # bots may upload up to 50 MB
TELEGRAM_TARGET = 44 * 1024 * 1024  # re-encode bigger videos to about this

log = logging.getLogger("bot")
ANSI = re.compile(r"\x1b\[[0-9;]*m")  # the CLI's coloured log output
LOG_LINE = re.compile(r"^\s*\d{4}-\d\d-\d\dT")  # a tracing line, not the command's own output


def load_env() -> dict[str, str]:
    env = {}
    for line in (ROOT / ".env").read_text(encoding="utf-8").splitlines():
        line = line.split(" #", 1)[0].strip()
        if "=" in line and not line.startswith("#"):
            k, v = line.split("=", 1)
            env[k.strip()] = v.strip().strip('"').strip("'")
    return env


ENV = load_env()
TOKEN = ENV.get("TELEGRAM_BOT_TOKEN", "")
TZ = ZoneInfo(ENV.get("BOT_TIMEZONE") or "UTC")
GAP = timedelta(hours=float(ENV.get("RENDER_GAP_HOURS") or 24))
lock = threading.RLock()


# --------------------------------------------------------------------------- state


def read_json(path: Path, default):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return default


def write_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(value, indent=2), encoding="utf-8")
    tmp.replace(path)


def queue() -> list[dict]:
    return read_json(QUEUE, [])


def save_queue(q: list[dict]) -> None:
    write_json(QUEUE, q)


def state() -> dict:
    return read_json(STATE, {})


def update_state(**kv) -> None:
    with lock:
        s = state()
        s.update(kv)
        write_json(STATE, s)


def add_topics(topics: list[str], first: datetime | None = None) -> list[dict]:
    """Append topics, each GAP after the last queued one (the first at `first`, or now)."""
    with lock:
        q = queue()
        pending = [i for i in q if i["status"] == "queued"]
        at = first or (datetime.fromtimestamp(max(i["at"] for i in pending), TZ) + GAP if pending else datetime.now(TZ))
        next_id = max((i["id"] for i in q), default=0) + 1
        added = []
        for t in topics:
            item = {"id": next_id, "topic": t, "at": at.timestamp(), "status": "queued", "run_id": None, "error": None}
            q.append(item)
            added.append(item)
            next_id += 1
            at += GAP
        save_queue(q)
        return added


def when(ts: float) -> str:
    return datetime.fromtimestamp(ts, TZ).strftime("%a %d %b %H:%M")


def parse_when(text: str) -> datetime | None:
    """'now', '+3h' / 'in 3h', '18:30' (next one), 'tomorrow 9:00', '2026-11-02 18:30'."""
    t = text.strip().lower()
    now = datetime.now(TZ)
    if t == "now":
        return now
    if t.startswith(("+", "in ")):
        num = t.lstrip("+").removeprefix("in ").strip()
        unit = num[-1]
        try:
            n = float(num[:-1])
        except ValueError:
            return None
        return now + {"m": timedelta(minutes=n), "h": timedelta(hours=n), "d": timedelta(days=n)}.get(unit, timedelta())
    day = now
    if t.startswith("tomorrow"):
        day, t = now + timedelta(days=1), t.removeprefix("tomorrow").strip()
    for fmt in ("%Y-%m-%d %H:%M", "%H:%M"):
        try:
            p = datetime.strptime(t, fmt)
        except ValueError:
            continue
        if fmt == "%H:%M":
            p = day.replace(hour=p.hour, minute=p.minute, second=0, microsecond=0)
            return p + timedelta(days=1) if p < now and day is now else p
        return p.replace(tzinfo=TZ)
    return None


# --------------------------------------------------------------------------- telegram


def api(method: str, http_timeout: float = 60, **params):
    req = urllib.request.Request(
        f"https://api.telegram.org/bot{TOKEN}/{method}",
        data=json.dumps(params).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=http_timeout) as r:
        return json.loads(r.read())["result"]


def upload(method: str, field: str, path: Path, **params):
    boundary = uuid.uuid4().hex
    parts = []
    for k, v in params.items():
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'.encode())
    parts.append(
        f'--{boundary}\r\nContent-Disposition: form-data; name="{field}"; filename="{path.name}"\r\n'
        f"Content-Type: application/octet-stream\r\n\r\n".encode()
    )
    body = b"".join(parts) + path.read_bytes() + f"\r\n--{boundary}--\r\n".encode()
    req = urllib.request.Request(
        f"https://api.telegram.org/bot{TOKEN}/{method}",
        data=body,
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
    )
    with urllib.request.urlopen(req, timeout=900) as r:
        return json.loads(r.read())["result"]


def say(text: str) -> None:
    chat = state().get("chat_id")
    if not chat:
        log.warning("no chat linked yet; dropping message: %s", text[:80])
        return
    for i in range(0, len(text), 4000):  # Telegram's limit is 4096 characters
        try:
            api("sendMessage", chat_id=chat, text=text[i:i + 4000], disable_web_page_preview=True)
        except urllib.error.URLError as e:
            log.error("sendMessage failed: %s", e)


def send_file(method: str, field: str, path: Path, **params) -> None:
    try:
        upload(method, field, path, chat_id=state().get("chat_id"), **params)
    except (urllib.error.URLError, OSError) as e:
        log.error("%s %s failed: %s", method, path.name, e)
        say(f"Couldn't send {path.name}: {e}. It's on the server at {path}")


# --------------------------------------------------------------------------- the work


class Worker(threading.Thread):
    """Runs one job at a time: due renders from the queue, and change requests."""

    def __init__(self):
        super().__init__(daemon=True)
        self.jobs: deque = deque()
        self.proc: subprocess.Popen | None = None
        self.current = ""
        self.started = 0.0
        self.log_path: Path | None = None

    def run(self):
        while True:
            try:
                if not state().get("chat_id"):
                    time.sleep(20)  # nothing renders until there's a chat to send it to
                elif self.jobs:
                    kind, *args = self.jobs.popleft()
                    {"change": self.change, "deliver": self.deliver, "improve": self.improve}[kind](*args)
                elif item := self.due():
                    self.render(item)
                else:
                    time.sleep(20)
            except Exception as e:  # keep the bot alive whatever one job does
                log.exception("job failed")
                say(f"Something went wrong: {e}")
            finally:
                self.current = ""

    def due(self) -> dict | None:
        now = time.time()
        due = [i for i in queue() if i["status"] == "queued" and i["at"] <= now]
        return min(due, key=lambda i: i["at"]) if due else None

    def cli(self, *args: str, label: str) -> tuple[bool, str]:
        """Run the story_animator CLI; returns (ok, last lines of its output)."""
        self.current, self.started = label, time.time()
        self.log_path = STATE_DIR / "logs" / f"{int(self.started)}.log"
        self.log_path.parent.mkdir(parents=True, exist_ok=True)
        with self.log_path.open("w", encoding="utf-8") as out:
            self.proc = subprocess.Popen([str(BIN), *args], cwd=ROOT, stdout=out, stderr=subprocess.STDOUT)
            code = self.proc.wait()
        self.proc = None
        text = ANSI.sub("", self.log_path.read_text(encoding="utf-8", errors="replace"))
        tail = "\n".join(text.splitlines()[-12:])
        return code == 0, tail

    def set_status(self, item_id: int, **kv) -> None:
        with lock:
            q = queue()
            for i in q:
                if i["id"] == item_id:
                    i.update(kv)
            save_queue(q)

    def render(self, item: dict) -> None:
        run_id = uuid.uuid4().hex
        self.set_status(item["id"], status="rendering", run_id=run_id)
        say(f"🎬 Rendering #{item['id']}: {item['topic']}\nThis takes a while; /status shows progress.")
        ok, tail = self.cli("generate", item["topic"], "--run-id", run_id, label=f"rendering #{item['id']}: {item['topic']}")
        if not ok:
            self.set_status(item["id"], status="failed", error=tail[-500:])
            say(f"❌ #{item['id']} failed: {item['topic']}\n\n{tail}\n\n/retry {item['id']} to try again.")
            return
        self.cli("review", run_id, label=f"reviewing #{item['id']}")
        self.cli("improve", run_id, label=f"improving #{item['id']}")
        ok, tail = self.cli("package", run_id, label=f"packaging #{item['id']}")
        if not ok:
            say(f"Packaging failed, sending the video without it:\n{tail}")
        self.set_status(item["id"], status="done")
        self.deliver(run_id)

    def change(self, run_id: str, text: str) -> None:
        title = video_title(run_id)
        ok, tail = self.cli("revise", run_id, text, label=f"changing '{title}'")
        if not ok:
            say(f"❌ Couldn't apply that change:\n{tail}")
            return
        summary = "\n".join(line for line in tail.splitlines() if not LOG_LINE.match(line))
        say(f"✅ {summary.strip() or 'Done.'}")
        self.deliver(run_id)

    def improve(self, run_id: str) -> None:
        title = video_title(run_id)
        ok, tail = self.cli("improve", run_id, "--force", label=f"improving '{title}'")
        summary = "\n".join(line for line in tail.splitlines() if not LOG_LINE.match(line)).strip()
        if not ok:
            say(f"❌ Couldn't improve it:\n{tail}")
            return
        say(summary or "Done.")
        imp = read_json(RUNS / run_id / "improve.json", {})
        if imp.get("after", 0) > imp.get("before", 0):
            self.cli("package", run_id, label=f"re-packaging '{title}'")
            self.deliver(run_id)

    def deliver(self, run_id: str) -> None:
        d = RUNS / run_id
        self.current = f"sending '{video_title(run_id)}'"
        update_state(last_run=run_id)
        delivered = state().get("delivered", [])
        if run_id not in delivered:
            update_state(delivered=(delivered + [run_id])[-30:])
        title = video_title(run_id)
        if (d / "thumbnail.png").exists():
            send_file("sendPhoto", "photo", d / "thumbnail.png", caption=title[:1000])
        for name, caption in (("final.mp4", title), ("short.mp4", "Short (9:16)")):
            video = telegram_copy(d / name)
            if not video:
                continue
            note = "" if video.name == name else " (compressed for Telegram; full quality on the server)"
            w, h, secs = probe(video)
            send_file("sendVideo", "video", video, caption=f"{caption}{note}"[:1000], supports_streaming="true",
                      width=w, height=h, duration=round(secs))
        if (d / "youtube.txt").exists():
            say((d / "youtube.txt").read_text(encoding="utf-8"))
        review = read_json(d / "review.json", None)
        if review:
            lines = [f"Self-review: {review.get('score', '?')}/10"]
            if review.get("vs_best"):
                lines.append(f"vs the best so far: {review['vs_best']}")
            imp = read_json(d / "improve.json", None)
            if imp and imp.get("attempts"):
                kept = imp["after"] > imp["before"]
                lines.append(f"Auto-improve: {imp['before']:.1f} → {imp['after']:.1f}" if kept else
                             f"Auto-improve tried, no attempt beat {imp['before']:.1f}; kept the original")
            lines += [f"• {p}" for p in review.get("problems", [])[:4]]
            facts = review.get("questionable_facts", [])
            if facts:
                lines.append("Check these facts before publishing:")
                lines += [f"• {f}" for f in facts[:4]]
            say("\n".join(lines))
        say(f"Run {run_id[:8]} · server: storage/runs/{run_id}\nReply in plain words to change anything in this video.")


def probe(video: Path) -> tuple[int, int, float]:
    """Width, height and duration, so Telegram shows the video at its real shape."""
    out = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                          "stream=width,height:format=duration", "-of", "json", str(video)],
                         capture_output=True, text=True).stdout
    info = json.loads(out or "{}")
    st = (info.get("streams") or [{}])[0]
    return int(st.get("width", 0)), int(st.get("height", 0)), float(info.get("format", {}).get("duration", 0) or 0)


def telegram_copy(video: Path) -> Path | None:
    """The video itself if Telegram takes it, else a copy re-encoded to fit."""
    if not video.exists():
        return None
    if video.stat().st_size <= TELEGRAM_LIMIT:
        return video
    small = video.with_name(f"{video.stem}_telegram.mp4")
    if small.exists() and small.stat().st_mtime >= video.stat().st_mtime:
        return small
    secs = probe(video)[2] or 300
    kbps = int(TELEGRAM_TARGET * 8 / 1000 / secs) - 96
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(video), "-c:v", "libx264", "-preset", "medium",
                    "-b:v", f"{kbps}k", "-maxrate", f"{int(kbps * 1.4)}k", "-bufsize", f"{kbps * 2}k",
                    "-c:a", "aac", "-b:a", "96k", "-movflags", "+faststart", str(small)], check=True)
    return small


def video_title(run_id: str) -> str:
    pack = read_json(RUNS / run_id / "package.json", None)
    if pack:
        return pack.get("title", run_id[:8])
    return read_json(RUNS / run_id / "storyboard.json", {}).get("title", run_id[:8])


# --------------------------------------------------------------------------- commands

HELP = """I render one queued video every day and send you the video, the short, the thumbnail, the title, description and tags, and a self-review.

Queue
/queue  list it
/add <topic>  add a topic, 24h after the last queued one (or 24h from now)
/add <topic> @ <time>  add it for a time: @ tomorrow 9:00, @ 18:30, @ +3h
/render <topic>  render a new topic now
/now <n>  render queue item n now
/at <n> <time>  reschedule: 18:30, tomorrow 9:00, +3h, 2026-11-02 18:30
/remove <n>  take it off the queue
/retry <n>  try a failed one again

While rendering
/status  what's running
/cancel  stop it

Your videos
/videos  recent videos (/pick <n> to choose which one changes apply to)
Plain message, or /change <text>: change the chosen video (newest by default), e.g. "make the thumbnail about the pressure" or "scene 12 is confusing, show the valve opening"
/resend  send the chosen video again
/improve  fix what the chosen video's self-review found (kept only if it scores higher)
/feedback <text>  a rule for every future video
/lessons  what the system has learned so far"""


def handle(worker: Worker, chat: int, text: str) -> None:
    s = state()
    cmd, _, arg = text.strip().partition(" ")
    cmd = cmd.split("@", 1)[0].lower()
    arg = arg.strip()

    linked = s.get("chat_id") or (int(ENV["TELEGRAM_CHAT_ID"]) if ENV.get("TELEGRAM_CHAT_ID") else None)
    if linked is None:
        code = ENV.get("TELEGRAM_PAIR_CODE", "")
        if cmd == "/start" and code and arg == code:
            update_state(chat_id=chat)
            respace()
            say("Linked. ✅\n\n" + HELP + "\n\n/queue shows what renders when.")
        return
    if chat != linked:
        log.warning("ignoring message from chat %s", chat)
        return
    if not s.get("chat_id"):
        update_state(chat_id=chat)

    target = s.get("picked") or s.get("last_run")
    if cmd in ("/start", "/help"):
        say(HELP)
    elif cmd == "/queue":
        q = [i for i in queue() if i["status"] in ("queued", "rendering", "failed")]
        say("\n".join(f"#{i['id']} {when(i['at'])} — {i['topic']} [{i['status']}]" for i in q) or "The queue is empty.")
    elif cmd == "/add" and arg:
        # "/add <topic> @ <time>" picks the time; plain "/add" goes RENDER_GAP_HOURS after the last
        # queued topic, or from now when nothing is queued (never "right now": that's /render).
        topic, sep, at_text = arg.rpartition(" @ ")
        topic = topic.strip() if sep else arg
        first = parse_when(at_text) if sep else None
        if sep and first is None:
            say("I didn't understand the time. Try: /add <topic> @ 18:30, @ tomorrow 9:00, @ +3h or @ 2026-11-02 18:30")
            return
        if first is None and not any(i["status"] == "queued" for i in queue()):
            first = datetime.now(TZ) + GAP
        item = add_topics([topic], first=first)[0]
        say(f"Queued #{item['id']} for {when(item['at'])}. (/now {item['id']} renders it now, "
            f"/at {item['id']} <time> moves it.)")
    elif cmd == "/render" and arg:
        item = add_topics([arg], first=datetime.now(TZ))[0]
        say(f"Queued #{item['id']} to render now" + (" (after the current job)." if worker.current else "."))
    elif cmd in ("/now", "/at", "/remove", "/retry"):
        num, _, rest = arg.partition(" ")
        at = datetime.now(TZ) if cmd in ("/now", "/retry") else parse_when(rest) if cmd == "/at" else None
        with lock:
            q = queue()
            item = next((i for i in q if str(i["id"]) == num.lstrip("#")), None)
            if item is None:
                say("No such item; /queue lists them.")
                return
            if cmd == "/remove":
                q.remove(item)
                msg = f"Removed #{item['id']}."
            elif at is None:
                say("I didn't understand the time. Try 18:30, tomorrow 9:00, +3h or 2026-11-02 18:30.")
                return
            elif item["status"] == "rendering":
                say("That one is rendering now.")
                return
            else:
                item.update(at=at.timestamp(), status="queued", error=None)
                msg = f"#{item['id']} will render {when(item['at'])}."
            save_queue(q)
        say(msg)
    elif cmd == "/status":
        if worker.current:
            mins = (time.time() - worker.started) / 60
            last = ""
            if worker.log_path and worker.log_path.exists():
                lines = worker.log_path.read_text(encoding="utf-8", errors="replace").splitlines()
                last = ANSI.sub("", lines[-1])[-300:] if lines else ""
            say(f"Now: {worker.current} ({mins:.0f} min)\n{last}")
        else:
            nxt = min((i for i in queue() if i["status"] == "queued"), key=lambda i: i["at"], default=None)
            say("Idle." + (f" Next: #{nxt['id']} {nxt['topic']} at {when(nxt['at'])}." if nxt else " The queue is empty."))
    elif cmd == "/cancel":
        if worker.proc:
            worker.proc.terminate()
            say(f"Stopping: {worker.current}")
        else:
            say("Nothing is running.")
    elif cmd == "/videos":
        delivered = s.get("delivered", [])[-8:]
        say("\n".join(f"{n}. {video_title(r)}{'  ← chosen' if r == target else ''}"
                      for n, r in enumerate(delivered, 1)) or "No videos yet.")
    elif cmd == "/pick" and arg.isdigit():
        delivered = s.get("delivered", [])[-8:]
        if 1 <= int(arg) <= len(delivered):
            update_state(picked=delivered[int(arg) - 1])
            say(f"Changes now apply to: {video_title(delivered[int(arg) - 1])}")
        else:
            say("No such video; /videos lists them.")
    elif cmd == "/resend" and target:
        worker.jobs.append(("deliver", target))
    elif cmd == "/improve" and target:
        worker.jobs.append(("improve", target))
        say(f"On it: improving '{video_title(target)}'" + (" (after the current job)." if worker.current else "."))
    elif cmd == "/feedback" and arg:
        threading.Thread(target=feedback, args=(arg,), daemon=True).start()
    elif cmd == "/lessons":
        text = (ROOT / "storage" / "LESSONS.md").read_text(encoding="utf-8") if (ROOT / "storage" / "LESSONS.md").exists() else ""
        say(text or "No lessons yet; they come from each video's self-review and your /feedback.")
    elif cmd.startswith("/") and cmd != "/change":
        say("I don't know that one. /help lists what I can do.")
    else:
        request = arg if cmd == "/change" else text
        if not target:
            say("There's no video to change yet.")
        elif request:
            worker.jobs.append(("change", target, request))
            say(f"On it: changing '{video_title(target)}'" + (" (after the current job)." if worker.current else "."))


def feedback(text: str) -> None:
    ok = subprocess.run([str(BIN), "feedback", text], cwd=ROOT, capture_output=True).returncode == 0
    say("Saved. Every future video will follow it." if ok else "Couldn't save that, sorry.")


def respace() -> None:
    """Keep queued renders GAP apart: after downtime, the overdue ones don't all run at once."""
    with lock:
        q = queue()
        prev = None
        for item in sorted((i for i in q if i["status"] == "queued"), key=lambda i: i["at"]):
            earliest = time.time() if prev is None else prev + GAP.total_seconds()
            item["at"] = prev = max(item["at"], earliest)
        save_queue(q)


def main() -> None:
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s",
                        handlers=[logging.StreamHandler(), logging.FileHandler(STATE_DIR / "bot.log")])
    if len(sys.argv) >= 3 and sys.argv[1] == "seed":
        topics = [t.strip() for t in Path(sys.argv[2]).read_text(encoding="utf-8").splitlines()
                  if t.strip() and not t.startswith("#")]
        first = parse_when(" ".join(sys.argv[3:])) if len(sys.argv) > 3 else None
        for i in add_topics(topics, first=first):
            print(f"#{i['id']} {when(i['at'])} {i['topic']}")
        return
    if not TOKEN:
        sys.exit("TELEGRAM_BOT_TOKEN is not set in .env")
    # A chat set in .env is linked straight away; otherwise "/start <pair code>" links one.
    if ENV.get("TELEGRAM_CHAT_ID") and not state().get("chat_id"):
        update_state(chat_id=int(ENV["TELEGRAM_CHAT_ID"]))
    # A render that was running when the bot stopped never finished; queue it again.
    with lock:
        q = queue()
        for i in q:
            if i["status"] == "rendering":
                i.update(status="queued", at=time.time())
        save_queue(q)
    respace()
    worker = Worker()
    worker.start()
    log.info("bot up as @%s", api("getMe")["username"])
    offset = state().get("offset", 0)
    while True:
        try:
            updates = api("getUpdates", http_timeout=70, timeout=50, offset=offset, allowed_updates=["message"])
        except (urllib.error.URLError, TimeoutError, ValueError) as e:
            log.warning("getUpdates: %s", e)
            time.sleep(5)
            continue
        for u in updates:
            offset = u["update_id"] + 1
            update_state(offset=offset)
            msg = u.get("message") or {}
            if msg.get("text"):
                try:
                    handle(worker, msg["chat"]["id"], msg["text"])
                except Exception:
                    log.exception("handling %r", msg["text"][:80])


if __name__ == "__main__":
    main()
