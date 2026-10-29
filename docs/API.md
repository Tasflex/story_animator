# HTTP API

Start the server:

```bash
story_animator serve                 # SERVER_ADDR, default 0.0.0.0:3000
story_animator serve --addr 127.0.0.1:8080
```

Generating a video takes minutes, so the API is **job-based**: you submit a
job, poll its status, then download the result. At most `MAX_CONCURRENT_JOBS`
run at once; the rest wait in a queue.

> The API has no authentication. Keep it on localhost or behind your own auth proxy.

## `POST /api/videos`: create a video

```json
{
  "topic": "Why is the sky blue?",
  "style": "diagram",
  "aspect": "portrait",
  "quality": "high",
  "theme": "ocean",
  "audience": "curious 10-year-olds",
  "voice_id": "optional ElevenLabs voice id",
  "silent": false,
  "music": true
}
```

Give either `topic` or `storyboard` (a full storyboard object, see
[STORYBOARD.md](STORYBOARD.md)). All other fields are optional and default
to the server's `.env`.

**202 Accepted**
```json
{ "id": "cd1dc099e75548db9383517ea4df67d8",
  "status_url": "/api/videos/cd1dc099e75548db9383517ea4df67d8",
  "job": { "state": "queued", ... } }
```

## `GET /api/videos/{id}`: job status

```json
{
  "id": "cd1dc099e75548db9383517ea4df67d8",
  "state": "running",
  "stage": "render",
  "progress": 0.67,
  "message": "rendered scene 5/8",
  "topic": "Why is the sky blue?",
  "created_at": 1793219663,
  "finished_at": null,
  "error": null,
  "result": null
}
```

- `state`: `queued` → `running` → `done` | `failed`
- `stage`: `storyboard` → `narration` → `timeline` → `render` → `compose` → `done`
- When finished, `result` contains `title`, `duration`, `scenes`, `width`, `height`, `fps` and paths.

## `GET /api/videos/{id}/video`: download

Streams the MP4 (`video/mp4`). Returns 404 until the job is done.

## `GET /api/videos/{id}/storyboard`

The storyboard used for the job. Useful for editing and re-submitting.

## `GET /api/videos`

All jobs since the server started, newest first.

## `POST /api/storyboard`: storyboard only

```json
{ "topic": "How vaccines work", "style": "diagram", "theme": "blueprint", "audience": "adults", "aspect": "landscape" }
```
Returns `{ "storyboard": {...}, "words": 132 }`. Review or edit it, then send
it back as `POST /api/videos {"storyboard": ...}`.

## `GET /api/catalog`

The vocabulary: layouts with their content schemas, icons, backgrounds, themes, cameras, transitions.

## Health

- `GET /healthz` returns `{"status":"ok"}` (liveness).
- `GET /readyz` runs the same checks as `story_animator doctor`. Returns 503 if a required dependency is missing.

## Example (bash)

```bash
ID=$(curl -s -X POST localhost:3000/api/videos -H 'content-type: application/json' \
      -d '{"topic":"How do bees make honey?"}' | jq -r .id)
while :; do
  S=$(curl -s localhost:3000/api/videos/$ID); echo "$S" | jq -r '"\(.state) \(.progress) \(.message)"'
  [[ $(echo "$S" | jq -r .state) =~ done|failed ]] && break; sleep 5
done
curl -o honey.mp4 localhost:3000/api/videos/$ID/video
```

Errors are JSON: `{"error": "..."}`, with 400 for bad input, 404 for unknown ids, and 500 for failures.
