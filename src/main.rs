mod api;
mod cache;
mod cli;
mod config;
mod error;
mod model;
mod output;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Commands};
use config::AppConfig;
use std::time::SystemTime;

use crate::api::YouTubeClient;
use crate::cache::Cache;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("youtube_cli=warn")),
        )
        .with_target(false)
        .init();

    let config = AppConfig::load(cli.no_cache, cli.json);

    ctrlc::set_handler(|| {
        eprintln!("\nInterrupted.");
        std::process::exit(130);
    })
    .context("Failed to set Ctrl+C handler")?;

    let client = YouTubeClient::new();

    match cli.command {
        Commands::Search {
            query,
            limit,
            sort,
            duration,
        } => {
            cmd_search(&config, &client, &query, limit, sort, duration).await?;
        }
        Commands::Transcript {
            video,
            lang,
            timestamps,
        } => {
            cmd_transcript(&config, &client, &video, &lang, timestamps).await?;
        }
        Commands::Video { video } => {
            cmd_video(&config, &client, &video).await?;
        }
    }

    Ok(())
}

async fn cmd_search(
    config: &AppConfig,
    client: &YouTubeClient,
    query: &str,
    limit: usize,
    sort: cli::SortOrder,
    duration: Option<cli::DurationFilter>,
) -> Result<()> {
    if query.trim().is_empty() {
        anyhow::bail!("Search query cannot be empty");
    }
    if limit == 0 || limit > 50 {
        anyhow::bail!("Limit must be between 1 and 50");
    }

    let cache = Cache::new(config.cache_dir.clone(), config.no_cache);
    let sort_str = format!("{:?}", sort);
    let dur_str = duration.map(|d| format!("{:?}", d)).unwrap_or_default();
    let cache_key = Cache::search_cache_key(query, &sort_str, &dur_str);

    // Check cache
    if let Some(hit) = cache.get_search::<model::SearchResult>(&cache_key) {
        // Only use cache if it has enough results
        if hit.data.videos.len() >= limit {
            let mut result = hit.data;
            result.videos.truncate(limit);
            if config.json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print!("{}", output::format_search_results(&result));
                println!("*Cached: {}*", output::format_cached_at(hit.cached_at));
            }
            return Ok(());
        }
    }

    let result = client.search(query, limit, sort, duration).await?;

    if result.videos.is_empty() {
        println!("No results found for: {}", query);
        return Ok(());
    }

    if let Err(e) = cache.set_search(&cache_key, &result) {
        tracing::debug!("Failed to cache search results: {}", e);
    }

    if config.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print!("{}", output::format_search_results(&result));
        println!(
            "*Data from: {}*",
            output::format_cached_at(SystemTime::now())
        );
    }

    Ok(())
}

async fn cmd_transcript(
    config: &AppConfig,
    client: &YouTubeClient,
    video: &str,
    lang: &str,
    timestamps: bool,
) -> Result<()> {
    let video_id = parse_video_id(video)?;

    let cache = Cache::new(config.cache_dir.clone(), config.no_cache);

    // Check cache
    if let Some(hit) = cache.get_transcript::<model::TranscriptResult>(&video_id, lang) {
        if config.json_output {
            println!("{}", serde_json::to_string_pretty(&hit.data)?);
        } else {
            print!("{}", output::format_transcript(&hit.data, timestamps));
            println!("*Cached: {}*", output::format_cached_at(hit.cached_at));
        }
        return Ok(());
    }

    let result = client.transcript(&video_id, lang).await?;

    if let Err(e) = cache.set_transcript(&video_id, lang, &result) {
        tracing::debug!("Failed to cache transcript: {}", e);
    }

    if config.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print!("{}", output::format_transcript(&result, timestamps));
        println!(
            "*Data from: {}*",
            output::format_cached_at(SystemTime::now())
        );
    }

    Ok(())
}

async fn cmd_video(config: &AppConfig, client: &YouTubeClient, video: &str) -> Result<()> {
    let video_id = parse_video_id(video)?;

    let cache = Cache::new(config.cache_dir.clone(), config.no_cache);

    // Check cache
    if let Some(hit) = cache.get_video::<model::VideoDetail>(&video_id) {
        if config.json_output {
            println!("{}", serde_json::to_string_pretty(&hit.data)?);
        } else {
            print!("{}", output::format_video_detail(&hit.data));
            println!("*Cached: {}*", output::format_cached_at(hit.cached_at));
        }
        return Ok(());
    }

    let detail = client.video_detail(&video_id).await?;

    if let Err(e) = cache.set_video(&video_id, &detail) {
        tracing::debug!("Failed to cache video detail: {}", e);
    }

    if config.json_output {
        println!("{}", serde_json::to_string_pretty(&detail)?);
    } else {
        print!("{}", output::format_video_detail(&detail));
        println!(
            "*Data from: {}*",
            output::format_cached_at(SystemTime::now())
        );
    }

    Ok(())
}

/// Parse a video ID from either a plain ID or various YouTube URL formats.
fn parse_video_id(input: &str) -> Result<String> {
    let input = input.trim();

    // Plain video ID (11 chars, alphanumeric + _ + -)
    if input.len() == 11
        && input
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Ok(input.to_string());
    }

    // Try to parse as URL
    if input.contains("youtube.com") || input.contains("youtu.be") {
        // youtube.com/watch?v=VIDEO_ID
        if let Some(pos) = input.find("v=") {
            let rest = &input[pos + 2..];
            let id = rest
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                .next()
                .unwrap_or("");
            if !id.is_empty() {
                return Ok(id.to_string());
            }
        }

        // youtu.be/VIDEO_ID
        if input.contains("youtu.be/") {
            if let Some(pos) = input.find("youtu.be/") {
                let rest = &input[pos + 9..];
                let id = rest
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                    .next()
                    .unwrap_or("");
                if !id.is_empty() {
                    return Ok(id.to_string());
                }
            }
        }

        // youtube.com/embed/VIDEO_ID
        if input.contains("/embed/") {
            if let Some(pos) = input.find("/embed/") {
                let rest = &input[pos + 7..];
                let id = rest
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                    .next()
                    .unwrap_or("");
                if !id.is_empty() {
                    return Ok(id.to_string());
                }
            }
        }
    }

    // Fallback: treat as video ID if it looks reasonable
    if !input.is_empty()
        && input.len() <= 20
        && input
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Ok(input.to_string());
    }

    anyhow::bail!(
        "Could not parse video ID from: {}. Provide a YouTube video ID or URL.",
        input
    );
}
