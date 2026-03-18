# youtube-cli

A Rust command-line tool for searching [YouTube](https://www.youtube.com) videos and downloading transcripts. Designed for both AI agents and humans — clean commands, Markdown output, no API key required.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/SeverinAlexB/youtube-cli/master/install.sh | bash
```

This downloads the latest release binary for your platform and installs it to `/usr/local/bin`. Run the same command again to update.

### Build from source

Requires [Rust](https://www.rust-lang.org/tools/install) (1.70+).

```bash
git clone https://github.com/SeverinAlexB/youtube-cli.git
cd youtube-cli
cargo build --release
```

The binary will be at `target/release/youtube-cli`.

## Usage

```
youtube-cli <command> [options] [arguments]
```

### Search for videos

```bash
youtube-cli search "rust programming"
youtube-cli search "andrew huberman sleep" --limit 5 --sort views
youtube-cli search "cooking recipes" --duration short
```

**Options:**

| Flag | Description | Default |
|---|---|---|
| `--limit <n>` | Max results to return (1-50) | 10 |
| `--sort <method>` | `relevance`, `date`, `views`, `rating` | `relevance` |
| `--duration <filter>` | `short`, `medium`, `long` | — |

**Example output:**

```markdown
## YouTube: "rust programming" (showing 3)

1. **Rust Programming Full Course | Learn in 2024**
   BekBrace — 3:05:04 — 446,051 views — 1 year ago
   https://youtube.com/watch?v=rQ_J9WH6CGk

2. **Rust in 100 Seconds**
   Fireship — 2:29 — 2,368,790 views — 4 years ago
   https://youtube.com/watch?v=5C_HPTJg5ek

3. **Learn Rust Programming - Complete Course**
   freeCodeCamp.org — 13:59:10 — 1,111,358 views — 2 years ago
   https://youtube.com/watch?v=BpPEoZW5IiY
```

### Get video transcript

```bash
youtube-cli transcript bKCcvfIHfZA
youtube-cli transcript "https://www.youtube.com/watch?v=bKCcvfIHfZA"
youtube-cli transcript bKCcvfIHfZA --timestamps
youtube-cli transcript bKCcvfIHfZA --lang de
```

Accepts video IDs (e.g., `bKCcvfIHfZA`) or full URLs (`youtube.com/watch?v=...`, `youtu.be/...`, `/embed/...`).

**Options:**

| Flag | Description | Default |
|---|---|---|
| `--lang <code>` | Preferred language code (e.g., `en`, `de`, `es`) | `en` |
| `--timestamps` | Include `[M:SS]` timestamps in output | — |

**Example output (with --timestamps):**

```markdown
## Transcript: Episode #140: The Devil in the Garlic with Dr. Greg Nigh
Channel: BetterHealthGuy — Language: English (auto-generated)
https://youtube.com/watch?v=bKCcvfIHfZA

[0:00] [Music]
[0:01] welcome to better health guy blogcasts
[0:03] empowering your better health and now
[0:06] here's scott
[0:07] your better health guy
[0:15] the content of this show is for
[0:16] informational purposes only and is not
[0:19] intended to diagnose
[0:20] treat or cure any illness or medical
```

### Get video details

```bash
youtube-cli video bKCcvfIHfZA
youtube-cli video "https://youtube.com/watch?v=5C_HPTJg5ek"
```

**Example output:**

```markdown
# Rust in 100 Seconds

## Overview

- **Channel**: Fireship
- **Views**: 2,368,794
- **Duration**: 2:29
- **URL**: https://youtube.com/watch?v=5C_HPTJg5ek

## Available Captions

- English (en) [auto-generated]

## Description

Rust is a memory-safe compiled programming language for building
high-performance systems...
```

### Browse channel videos

```bash
youtube-cli channel @hubermanlab
youtube-cli channel @hubermanlab --sort popular --limit 5
youtube-cli channel @hubermanlab --sort oldest --limit 10
youtube-cli channel UCBJycsmduvYEL83R_U4JriQ --limit 5
youtube-cli channel "https://youtube.com/@mkbhd" --limit 5
```

Accepts channel handles (`@hubermanlab`), channel IDs (`UCxxxxxx`), or full YouTube URLs.

**Options:**

| Flag | Description | Default |
|---|---|---|
| `--limit <n>` | Max results to return (1-200) | 30 |
| `--sort <method>` | `newest`, `oldest`, `popular` | `newest` |
| `--search <query>` | Search within the channel's videos | — |

**Example output:**

```markdown
## Channel: Andrew Huberman
https://youtube.com/channel/UC2D2CMWXMOVWx7giW1n3LIg

### Videos (showing 3)

1. **Science-Based Meditation Tools | Dr. Richard Davidson**
   2:43:45 — 115,266 views — 2 days ago
   https://youtube.com/watch?v=hlOA8ObQJXo

2. **Benefits of Sauna & Deliberate Heat Exposure**
   39:20 — 58,764 views — 6 days ago
   https://youtube.com/watch?v=iuPQmw4Ax00
```

### Search within a channel

```bash
youtube-cli channel @hubermanlab --search "sleep"
youtube-cli channel @mkbhd --search "iPhone" --limit 10
```

Returns only videos from the specified channel that match the query.

### Global flags

| Flag | Description |
|---|---|
| `--no-cache` | Bypass local cache and fetch fresh data |
| `--json` | Output raw JSON instead of Markdown |

```bash
youtube-cli search "rust" --json --limit 3
youtube-cli transcript bKCcvfIHfZA --json
youtube-cli video bKCcvfIHfZA --no-cache
```

## Caching

Fetched data is cached locally to reduce redundant requests. The cache directory is platform-dependent:

- **macOS:** `~/Library/Caches/youtube-cli/`
- **Linux:** `~/.cache/youtube-cli/`

Cache TTLs:

| Data type | TTL |
|---|---|
| Search results | 7 days |
| Channel listings | 1 day |
| Transcripts | 30 days |
| Video details | 7 days |

Every result includes a `Data from:` or `Cached:` timestamp so you know how fresh the data is. Use `--no-cache` to bypass the cache and fetch fresh data.

## How it works

youtube-cli uses YouTube's internal InnerTube API — the same API that powers the YouTube website and mobile apps. No API key, no OAuth, no browser needed.

- **Search** uses the `/youtubei/v1/search` endpoint with the WEB client
- **Channel** uses the `/youtubei/v1/browse` endpoint to list channel videos, with protobuf-encoded continuation tokens for sort order control
- **Channel search** fetches the channel's search page and extracts results from the embedded `ytInitialData`
- **Transcripts** use a two-step process: fetch video page to extract the API key, then call the `/youtubei/v1/player` endpoint to get caption track URLs, then fetch and parse the caption XML
- **Video details** are extracted from the same player API response

Rate limiting (1 request/second) and retry logic are built in.

## Claude Code skill

This repo includes a [Claude Code skill](https://code.claude.com/docs/en/skills) that teaches AI agents how to use `youtube-cli` for video research and transcript analysis. With the skill installed, Claude can autonomously search for videos, download transcripts, and summarize content.

### Install the skill

```bash
/install-plugin youtube-cli@SeverinAlexB/youtube-cli
```

### What the agent can do

Once installed, Claude can handle requests like:

- *"Find the top Huberman Lab episodes on sleep"*
- *"What are the most popular videos on the MKBHD channel?"*
- *"Search the Veritasium channel for videos about physics"*
- *"Download and summarize the transcript from this YouTube video"*
- *"Search for Rust programming tutorials sorted by views"*
- *"What languages are available for this video's captions?"*

The skill guides Claude through multi-step workflows — searching, fetching transcripts, checking available languages, and summarizing content.

### Requirements

The `youtube-cli` binary must be available on `PATH`. Build it first:

```bash
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```
