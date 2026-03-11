---
name: youtube-cli
description: Query YouTube videos and download transcripts using the youtube-cli command-line tool. Use when the user needs to search for YouTube videos, get video details, or download video transcripts/captions.
---

# YouTube Agent Skill

Use the `youtube-cli` command-line tool to search YouTube and download video transcripts.

## Commands

### Search Videos

```bash
youtube-cli search "<query>" [--limit N] [--sort ORDER] [--duration FILTER]
```

**Options:**
- `--limit N` — Max results, 1-50 (default: 10)
- `--sort` — relevance | date | views | rating (default: relevance)
- `--duration` — short | medium | long (filter by video length)
- `--json` — Output raw JSON
- `--no-cache` — Bypass cache

**Example:**
```bash
youtube-cli search "rust programming tutorial" --limit 5
youtube-cli search "andrew huberman sleep" --limit 3 --sort views
youtube-cli search "cooking recipes" --limit 5 --duration short
```

### Get Video Transcript

```bash
youtube-cli transcript <VIDEO_ID_OR_URL> [--lang CODE] [--timestamps]
```

**Options:**
- `--lang CODE` — Preferred language code (default: en)
- `--timestamps` — Include [M:SS] timestamps
- `--json` — Output raw JSON with start/duration/text per entry
- `--no-cache` — Bypass cache

**Example:**
```bash
youtube-cli transcript bKCcvfIHfZA
youtube-cli transcript "https://www.youtube.com/watch?v=bKCcvfIHfZA"
youtube-cli transcript bKCcvfIHfZA --timestamps
youtube-cli transcript bKCcvfIHfZA --lang de
youtube-cli transcript bKCcvfIHfZA --json
```

**Accepts:** Video IDs (e.g., `bKCcvfIHfZA`) or full URLs (`youtube.com/watch?v=...`, `youtu.be/...`).

### Get Video Details

```bash
youtube-cli video <VIDEO_ID_OR_URL>
```

**Options:**
- `--json` — Output raw JSON
- `--no-cache` — Bypass cache

**Example:**
```bash
youtube-cli video bKCcvfIHfZA
youtube-cli video "https://youtube.com/watch?v=bKCcvfIHfZA" --json
```

**Returns:** Title, channel, views, duration, description, available caption languages, keywords.

## Workflow Examples

### 1. Find and summarize a video's content
```bash
youtube-cli search "huberman dopamine" --limit 3
youtube-cli transcript <VIDEO_ID>
```

### 2. Research a topic across multiple videos
```bash
youtube-cli search "intermittent fasting benefits" --limit 5 --sort views
# For each interesting video:
youtube-cli transcript <VIDEO_ID> --timestamps
```

### 3. Get timestamped transcript for note-taking
```bash
youtube-cli transcript <VIDEO_ID> --timestamps
```

### 4. Check if a video has captions before downloading
```bash
youtube-cli video <VIDEO_ID>
# Look at "Available Captions" section
```

### 5. Get transcript in a specific language
```bash
youtube-cli video <VIDEO_ID>  # Check available languages
youtube-cli transcript <VIDEO_ID> --lang de
```

### 6. Export transcript data for processing
```bash
youtube-cli transcript <VIDEO_ID> --json
```

## Tips

- Use specific search terms for better results
- Use `--sort views` to find popular/authoritative videos
- Use `--duration short` for quick explainers
- Use `--timestamps` when you need to reference specific parts of a video
- Use `--json` for programmatic processing of results
- Video IDs are the 11-character codes in YouTube URLs (e.g., `bKCcvfIHfZA`)
- Transcripts are cached for 30 days, search results for 7 days
- Use `--no-cache` to get fresh data
- Not all videos have transcripts — check with `youtube-cli video` first
- Auto-generated captions may contain errors; manual captions are more reliable
