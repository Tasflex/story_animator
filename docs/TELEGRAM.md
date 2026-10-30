# Telegram bot

The bot runs on the VPS next to the pipeline. It holds a queue of topics and
renders one every `RENDER_GAP_HOURS` (24 by default). For each video it runs
the whole chain and sends you the results:

```
generate ─► review (score vs earlier videos + lessons) ─► improve (if below the best so far) ─► package ─► Telegram
```

In your chat you get the thumbnail, the video, the 9:16 short, the
title/description/tags ready to paste, and the self-review: a score, how it
compares with the best video so far, what auto-improvement changed, the
problems, and any facts to check before publishing.

## Setup (once)

1. In Telegram, talk to **@BotFather**: `/newbot`, pick a name, and copy the token.
2. On the VPS, add to `/root/story_animator/.env`:
   ```
   TELEGRAM_BOT_TOKEN=<token>
   TELEGRAM_PAIR_CODE=<any secret word>
   BOT_TIMEZONE=Africa/Nairobi
   ```
3. Install and start the service:
   ```bash
   cp bot/story-bot.service /etc/systemd/system/
   systemctl daemon-reload && systemctl enable --now story-bot
   ```
4. In Telegram, open your bot and send `/start <pair code>`. Only that chat
   can control the bot from then on. Nothing renders until a chat is linked.
   (If `TELEGRAM_CHAT_ID` is set in `.env`, that chat is linked at startup.)

Logs: `journalctl -u story-bot -f`, or `storage/bot/bot.log`, plus one file
per job in `storage/bot/logs/`.

## Commands

| | |
|---|---|
| `/queue` | What renders when |
| `/add <topic>` | Add a topic `RENDER_GAP_HOURS` after the last queued one, or that long from now if nothing is queued |
| `/add <topic> @ <time>` | Add it for a time: `@ tomorrow 9:00`, `@ 18:30`, `@ +3h`, `@ 2026-11-02 18:30` |
| `/render <topic>` | Render a new topic now |
| `/now <n>` | Render queue item n now |
| `/at <n> <time>` | Reschedule: `18:30`, `tomorrow 9:00`, `+3h`, `2026-11-02 18:30` |
| `/remove <n>`, `/retry <n>` | Drop an item, or try a failed one again |
| `/status`, `/cancel` | What's running, or stop it |
| `/videos`, `/pick <n>` | Recent videos, and which one changes apply to (the newest by default) |
| *plain message* or `/change <text>` | Change the chosen video |
| `/resend` | Send the chosen video again |
| `/improve` | Fix what the chosen video's self-review found; kept only if the re-review scores higher |
| `/feedback <text>` | A rule for every future video (saved as an owner lesson) |
| `/lessons` | What the system has learned so far |

### Changing a video

Write what you want in plain words, for example:

- "make the thumbnail about the pressure, not the cable"
- "shorter title, and add the tag 'deep sea'"
- "scene 12 is confusing, show the valve opening"

`story_animator revise` decides what that touches. Packaging changes are
redone by the packager with your words as its instructions. Scene changes
replace those scenes and resume the run: narration is cached by text, so only
the changed scenes cost TTS credits. Then the bot sends the new version. If a
request states a general preference ("always end on a question"), it's also
saved as an owner lesson for every later video. See
[LEARNING.md](LEARNING.md).

## Notes

- **Video size.** Telegram bots can upload at most 50 MB. A five-minute
  1080p video is often bigger, so the bot sends a copy re-encoded to about
  44 MB (`final_telegram.mp4`, likewise `short_telegram.mp4`). Upload
  `final.mp4` and `short.mp4` from the server to YouTube for full quality.
- **Queue file.** `storage/bot/queue.json`. To add a list of topics in one
  go: `venv/bin/python bot/telegram_bot.py seed topics.txt "2026-11-01 09:00"`
  (one topic per line, `RENDER_GAP_HOURS` apart from that time).
- **Downtime.** If the bot was down when renders were due, it spaces them
  out again from now instead of running them back-to-back. A render cut off
  by a restart is queued again.
- **Cost.** Each video costs one narration (about 3,500 ElevenLabs
  characters) plus about 15 LLM calls for writing, review and packaging.
