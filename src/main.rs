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
        Commands::Channel {
            channel,
            search,
            limit,
            sort,
        } => {
            cmd_channel(&config, &client, &channel, search.as_deref(), limit, sort).await?;
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

async fn cmd_channel(
    config: &AppConfig,
    client: &api::YouTubeClient,
    channel_input: &str,
    search_query: Option<&str>,
    limit: usize,
    sort: cli::ChannelSort,
) -> Result<()> {
    if limit == 0 || limit > 200 {
        anyhow::bail!("Limit must be between 1 and 200");
    }
    if let Some(q) = search_query {
        if q.trim().is_empty() {
            anyhow::bail!("Search query cannot be empty");
        }
    }

    let cache = Cache::new(config.cache_dir.clone(), config.no_cache);

    // Step 1: Resolve channel ID
    let (channel_id, channel_name) =
        resolve_channel_input(client, &cache, channel_input).await?;

    // Step 2: Build cache key
    let sort_str = if search_query.is_some() {
        "search".to_string()
    } else {
        format!("{:?}", sort)
    };
    let query_str = search_query.unwrap_or("");
    let cache_key = Cache::channel_cache_key(&channel_id, &sort_str, query_str);

    // Step 3: Check cache
    if let Some(hit) = cache.get_channel::<model::ChannelVideosResult>(&cache_key) {
        if hit.data.videos.len() >= limit {
            let mut result = hit.data;
            result.videos.truncate(limit);
            if config.json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print!("{}", output::format_channel_videos(&result));
                println!("*Cached: {}*", output::format_cached_at(hit.cached_at));
            }
            return Ok(());
        }
    }

    // Step 4: Fetch from API
    let result = if let Some(query) = search_query {
        client
            .channel_search(&channel_id, &channel_name, query, limit)
            .await?
    } else {
        client.channel_videos(&channel_id, sort, limit).await?
    };

    if result.videos.is_empty() {
        if let Some(query) = search_query {
            println!(
                "No videos found for \"{}\" in channel {}",
                query, result.channel.name
            );
        } else {
            println!("No videos found for channel {}", result.channel.name);
        }
        return Ok(());
    }

    // Step 5: Cache
    if let Err(e) = cache.set_channel(&cache_key, &result) {
        tracing::debug!("Failed to cache channel results: {}", e);
    }

    // Step 6: Output
    if config.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print!("{}", output::format_channel_videos(&result));
        println!(
            "*Data from: {}*",
            output::format_cached_at(SystemTime::now())
        );
    }

    Ok(())
}

/// Resolve channel input (handle, URL, or ID) to (channel_id, channel_name).
async fn resolve_channel_input(
    client: &api::YouTubeClient,
    cache: &Cache,
    input: &str,
) -> Result<(String, String)> {
    use crate::api::channel::ChannelInput;

    match api::YouTubeClient::parse_channel_input(input) {
        ChannelInput::Id(id) => {
            // We don't know the name yet; it will be populated from the browse response
            Ok((id, "Unknown".to_string()))
        }
        ChannelInput::Handle(handle) => {
            // Check cache for handle resolution
            if let Some(hit) = cache.get_channel_id::<(String, String)>(&handle) {
                return Ok(hit.data);
            }
            let (id, name) = client.resolve_handle(&handle).await?;
            let _ = cache.set_channel_id(&handle, &(id.clone(), name.clone()));
            Ok((id, name))
        }
    }
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
