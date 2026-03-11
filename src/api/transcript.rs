use crate::api::YouTubeClient;
use crate::error::YoutubeError;
use crate::model::{CaptionLanguage, TranscriptEntry, TranscriptResult, VideoDetail};
use regex::Regex;

const WATCH_URL: &str = "https://www.youtube.com/watch?v=";
const INNERTUBE_PLAYER_URL: &str = "https://www.youtube.com/youtubei/v1/player";

/// Internal struct for caption track info from the player API
struct CaptionTrack {
    base_url: String,
    language_code: String,
    language_name: String,
    is_auto_generated: bool,
}

/// Internal struct for raw player API response data
struct PlayerData {
    video_id: String,
    title: String,
    channel: String,
    channel_id: String,
    view_count: Option<String>,
    length_seconds: Option<u64>,
    description: Option<String>,
    keywords: Vec<String>,
    is_live: bool,
    caption_tracks: Vec<CaptionTrack>,
}

impl YouTubeClient {
    /// Fetch video details (title, channel, description, available captions)
    pub async fn video_detail(&self, video_id: &str) -> Result<VideoDetail, YoutubeError> {
        let player_data = self.fetch_player_data(video_id).await?;

        Ok(VideoDetail {
            video_id: player_data.video_id,
            title: player_data.title,
            channel: player_data.channel,
            channel_id: player_data.channel_id,
            view_count: player_data.view_count,
            length_seconds: player_data.length_seconds,
            description: player_data.description,
            keywords: player_data.keywords,
            is_live: player_data.is_live,
            caption_languages: player_data
                .caption_tracks
                .iter()
                .map(|t| CaptionLanguage {
                    language_code: t.language_code.clone(),
                    language_name: t.language_name.clone(),
                    is_auto_generated: t.is_auto_generated,
                })
                .collect(),
        })
    }

    /// Fetch the transcript for a video in the given language
    pub async fn transcript(
        &self,
        video_id: &str,
        lang: &str,
    ) -> Result<TranscriptResult, YoutubeError> {
        let player_data = self.fetch_player_data(video_id).await?;

        if player_data.caption_tracks.is_empty() {
            return Err(YoutubeError::NoTranscript(video_id.to_string()));
        }

        // Find the requested language track
        // Priority: exact match manual > exact match auto > first track
        let track = find_caption_track(&player_data.caption_tracks, lang, video_id)?;

        // Fetch the transcript XML
        let xml = self.get_with_retry(&track.base_url).await?;

        if xml.is_empty() {
            return Err(YoutubeError::NoTranscript(video_id.to_string()));
        }

        let entries = parse_transcript_xml(&xml)?;

        Ok(TranscriptResult {
            video_id: player_data.video_id,
            title: player_data.title,
            channel: player_data.channel,
            language: track.language_name.clone(),
            language_code: track.language_code.clone(),
            is_auto_generated: track.is_auto_generated,
            entries,
        })
    }

    /// Fetch player data via: page HTML → extract API key → InnerTube player API
    async fn fetch_player_data(&self, video_id: &str) -> Result<PlayerData, YoutubeError> {
        // Step 1: Fetch the video page HTML to extract the INNERTUBE_API_KEY
        let page_url = format!("{}{}", WATCH_URL, video_id);
        let html = self.get_with_retry(&page_url).await?;

        // Check if video exists
        if html.contains("\"playabilityStatus\":{\"status\":\"ERROR\"") {
            return Err(YoutubeError::VideoUnavailable(video_id.to_string()));
        }

        let api_key = extract_innertube_api_key(&html, video_id)?;

        // Step 2: Call InnerTube player API with ANDROID client
        let player_url = format!("{}?key={}", INNERTUBE_PLAYER_URL, api_key);
        let body = serde_json::json!({
            "context": {
                "client": {
                    "clientName": "ANDROID",
                    "clientVersion": "20.10.38"
                }
            },
            "videoId": video_id
        });

        let response_text = self.post_json_with_retry(&player_url, &body).await?;
        let response: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
            YoutubeError::ParseError(format!("Failed to parse player response: {}", e))
        })?;

        // Check playability
        let status = response
            .pointer("/playabilityStatus/status")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN");

        if status == "ERROR" {
            let reason = response
                .pointer("/playabilityStatus/reason")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(YoutubeError::VideoUnavailable(format!(
                "{}: {}",
                video_id, reason
            )));
        }

        if status == "LOGIN_REQUIRED" {
            let reason = response
                .pointer("/playabilityStatus/reason")
                .and_then(|v| v.as_str())
                .unwrap_or("Login required");
            if reason.contains("bot") || reason.contains("Bot") {
                return Err(YoutubeError::Api(
                    "Request blocked by YouTube (bot detection). Try again later.".to_string(),
                ));
            }
            return Err(YoutubeError::Api(format!(
                "Video requires login: {}",
                reason
            )));
        }

        // Extract video details
        let video_details = &response["videoDetails"];
        let title = video_details
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let channel = video_details
            .get("author")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let channel_id = video_details
            .get("channelId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let view_count = video_details
            .get("viewCount")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let length_seconds = video_details
            .get("lengthSeconds")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok());
        let description = video_details
            .get("shortDescription")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let keywords = video_details
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let is_live = video_details
            .get("isLiveContent")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Extract caption tracks
        let caption_tracks = response
            .pointer("/captions/playerCaptionsTracklistRenderer/captionTracks")
            .and_then(|v| v.as_array())
            .map(|tracks| {
                tracks
                    .iter()
                    .filter_map(|track| {
                        let base_url = track.get("baseUrl").and_then(|v| v.as_str())?;
                        // Remove &fmt=srv3 if present
                        let base_url = base_url.replace("&fmt=srv3", "");
                        let language_code = track
                            .get("languageCode")
                            .and_then(|v| v.as_str())?
                            .to_string();
                        let raw_name = track
                            .pointer("/name/runs/0/text")
                            .or_else(|| track.pointer("/name/simpleText"))
                            .and_then(|v| v.as_str())
                            .unwrap_or(&language_code);
                        // Strip "(auto-generated)" suffix since we track that separately
                        let language_name =
                            raw_name.trim_end_matches(" (auto-generated)").to_string();
                        let is_auto_generated =
                            track.get("kind").and_then(|v| v.as_str()) == Some("asr");

                        Some(CaptionTrack {
                            base_url,
                            language_code,
                            language_name,
                            is_auto_generated,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(PlayerData {
            video_id: video_id.to_string(),
            title,
            channel,
            channel_id,
            view_count,
            length_seconds,
            description,
            keywords,
            is_live,
            caption_tracks,
        })
    }
}

fn extract_innertube_api_key(html: &str, video_id: &str) -> Result<String, YoutubeError> {
    let re = Regex::new(r#""INNERTUBE_API_KEY":\s*"([a-zA-Z0-9_-]+)""#).unwrap();
    match re.captures(html) {
        Some(caps) => Ok(caps[1].to_string()),
        None => {
            if html.contains("class=\"g-recaptcha\"") {
                Err(YoutubeError::Api(
                    "IP blocked by YouTube (reCAPTCHA). Try again later.".to_string(),
                ))
            } else {
                Err(YoutubeError::ParseError(format!(
                    "Could not extract INNERTUBE_API_KEY for video {}",
                    video_id
                )))
            }
        }
    }
}

fn find_caption_track<'a>(
    tracks: &'a [CaptionTrack],
    lang: &str,
    video_id: &str,
) -> Result<&'a CaptionTrack, YoutubeError> {
    // 1. Exact match for manual captions
    if let Some(track) = tracks
        .iter()
        .find(|t| t.language_code == lang && !t.is_auto_generated)
    {
        return Ok(track);
    }

    // 2. Exact match for auto-generated
    if let Some(track) = tracks.iter().find(|t| t.language_code == lang) {
        return Ok(track);
    }

    // 3. Prefix match (e.g., "en" matches "en-US")
    if let Some(track) = tracks
        .iter()
        .find(|t| t.language_code.starts_with(lang) || lang.starts_with(&t.language_code))
    {
        return Ok(track);
    }

    // 4. If lang wasn't found, list available languages in error
    let available: Vec<String> = tracks
        .iter()
        .map(|t| {
            format!(
                "{} ({}{})",
                t.language_code,
                t.language_name,
                if t.is_auto_generated {
                    ", auto-generated"
                } else {
                    ""
                }
            )
        })
        .collect();

    Err(YoutubeError::LanguageNotAvailable(
        video_id.to_string(),
        format!(
            "language '{}' not found. Available: {}",
            lang,
            available.join(", ")
        ),
    ))
}

fn parse_transcript_xml(xml: &str) -> Result<Vec<TranscriptEntry>, YoutubeError> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    let mut entries = Vec::new();
    let mut current_start: Option<f64> = None;
    let mut current_dur: Option<f64> = None;
    let mut in_text_element = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"text" => {
                current_start = None;
                current_dur = None;
                in_text_element = true;

                for attr in e.attributes().flatten() {
                    match attr.key.as_ref() {
                        b"start" => {
                            if let Ok(val) = std::str::from_utf8(&attr.value) {
                                current_start = val.parse().ok();
                            }
                        }
                        b"dur" => {
                            if let Ok(val) = std::str::from_utf8(&attr.value) {
                                current_dur = val.parse().ok();
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Text(ref t)) if in_text_element => {
                if let (Some(start), Some(dur)) = (current_start, current_dur) {
                    let raw_text = t.unescape().unwrap_or_default().to_string();
                    let text = decode_html_entities(&raw_text);
                    if !text.is_empty() {
                        entries.push(TranscriptEntry {
                            text,
                            start,
                            duration: dur,
                        });
                    }
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"text" => {
                in_text_element = false;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(YoutubeError::ParseError(format!(
                    "Failed to parse transcript XML: {}",
                    e
                )));
            }
            _ => {}
        }
    }

    Ok(entries)
}

fn decode_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}
