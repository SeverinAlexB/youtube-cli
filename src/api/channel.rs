use base64::{engine::general_purpose::STANDARD, Engine as _};
use regex::Regex;

use crate::api::search::parse_video_renderer;
use crate::api::YouTubeClient;
use crate::cli::ChannelSort;
use crate::error::YoutubeError;
use crate::model::{ChannelInfo, ChannelVideosResult, VideoSummary};

const INNERTUBE_BROWSE_URL: &str = "https://www.youtube.com/youtubei/v1/browse";

/// Default params to select the "Videos" tab (always returns newest first).
const VIDEOS_TAB_PARAMS: &str = "EgZ2aWRlb3PyBgQKAjoA";

pub enum ChannelInput {
    Id(String),
    Handle(String),
}

fn innertube_web_context() -> serde_json::Value {
    serde_json::json!({
        "client": {
            "clientName": "WEB",
            "clientVersion": "2.20240101.00.00"
        }
    })
}

// ---------------------------------------------------------------------------
// Minimal protobuf encoder (only what we need for continuation tokens)
// ---------------------------------------------------------------------------

fn pb_varint(value: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut v = value;
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if v == 0 {
            break;
        }
    }
    buf
}

fn pb_field_varint(field: u32, value: u64) -> Vec<u8> {
    let tag = ((field as u64) << 3) | 0; // wire type 0 = varint
    let mut buf = pb_varint(tag);
    buf.extend(pb_varint(value));
    buf
}

fn pb_field_bytes(field: u32, data: &[u8]) -> Vec<u8> {
    let tag = ((field as u64) << 3) | 2; // wire type 2 = length-delimited
    let mut buf = pb_varint(tag);
    buf.extend(pb_varint(data.len() as u64));
    buf.extend_from_slice(data);
    buf
}

fn pb_field_string(field: u32, s: &str) -> Vec<u8> {
    pb_field_bytes(field, s.as_bytes())
}

fn pb_field_message(field: u32, msg: &[u8]) -> Vec<u8> {
    pb_field_bytes(field, msg)
}

// ---------------------------------------------------------------------------

impl YouTubeClient {
    /// Parse user input into a channel ID or handle that needs resolution.
    pub fn parse_channel_input(input: &str) -> ChannelInput {
        let input = input.trim();

        // https://youtube.com/channel/UCxxxxxx
        if let Some(pos) = input.find("/channel/") {
            let rest = &input[pos + 9..];
            let id = rest
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                .next()
                .unwrap_or("");
            if id.starts_with("UC") && id.len() == 24 {
                return ChannelInput::Id(id.to_string());
            }
        }

        // https://youtube.com/@handle or https://www.youtube.com/@handle
        if let Some(pos) = input.find("/@") {
            let rest = &input[pos + 2..];
            let handle = rest
                .split(|c: char| c == '/' || c == '?' || c == '#')
                .next()
                .unwrap_or("");
            if !handle.is_empty() {
                return ChannelInput::Handle(format!("@{}", handle));
            }
        }

        // Bare @handle
        if input.starts_with('@') && input.len() > 1 && !input.contains('/') {
            return ChannelInput::Handle(input.to_string());
        }

        // Bare channel ID (UC + 22 chars)
        if input.starts_with("UC")
            && input.len() == 24
            && input
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return ChannelInput::Id(input.to_string());
        }

        // Fallback: treat as handle without @
        ChannelInput::Handle(format!("@{}", input))
    }

    /// Resolve a @handle to (channel_id, channel_name) by fetching the channel page.
    pub async fn resolve_handle(&self, handle: &str) -> Result<(String, String), YoutubeError> {
        let handle_clean = handle.trim_start_matches('@');
        let url = format!("https://www.youtube.com/@{}", handle_clean);

        let html = match self.get_with_retry(&url).await {
            Ok(html) => html,
            Err(YoutubeError::Api(msg)) if msg.contains("404") => {
                return Err(YoutubeError::ChannelNotFound(format!(
                    "Channel @{} not found",
                    handle_clean
                )));
            }
            Err(e) => return Err(e),
        };

        // Extract channel ID
        let id_re = Regex::new(r#""browseId"\s*:\s*"(UC[a-zA-Z0-9_-]{22})""#).unwrap();
        let channel_id = id_re
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .or_else(|| {
                let meta_re =
                    Regex::new(r#"<meta\s+itemprop="channelId"\s+content="(UC[a-zA-Z0-9_-]{22})""#)
                        .unwrap();
                meta_re
                    .captures(&html)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
            })
            .ok_or_else(|| {
                YoutubeError::ChannelNotFound(format!(
                    "Could not find channel ID for @{}",
                    handle_clean
                ))
            })?;

        // Extract channel name
        let name_re = Regex::new(r#""title"\s*:\s*"([^"]+)""#).unwrap();
        let channel_name = name_re
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| handle.to_string());

        Ok((channel_id, channel_name))
    }

    /// List videos from a channel using the InnerTube browse endpoint.
    ///
    /// For "newest" sort, uses the standard browse with params.
    /// For "popular" and "oldest", constructs a continuation token that encodes
    /// both the channel ID and sort order (YouTube requires this approach).
    pub async fn channel_videos(
        &self,
        channel_id: &str,
        sort: ChannelSort,
        limit: usize,
    ) -> Result<ChannelVideosResult, YoutubeError> {
        // For non-default sorts, we use a continuation token instead of browseId+params.
        // YouTube's API only supports sort via continuation tokens, not via params.
        let (initial_response, channel_info) = if sort == ChannelSort::Newest {
            // Newest: use standard browse request
            let body = serde_json::json!({
                "context": innertube_web_context(),
                "browseId": channel_id,
                "params": VIDEOS_TAB_PARAMS
            });
            let text = self.post_json_with_retry(INNERTUBE_BROWSE_URL, &body).await?;
            let response: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| YoutubeError::ParseError(format!("Failed to parse browse response: {}", e)))?;
            let info = extract_channel_info(&response, channel_id);
            (response, info)
        } else {
            // Popular/Oldest: first fetch channel info with a standard browse,
            // then use a continuation token for the sorted listing.
            let info_body = serde_json::json!({
                "context": innertube_web_context(),
                "browseId": channel_id,
                "params": VIDEOS_TAB_PARAMS
            });
            let info_text = self.post_json_with_retry(INNERTUBE_BROWSE_URL, &info_body).await?;
            let info_response: serde_json::Value = serde_json::from_str(&info_text)
                .map_err(|e| YoutubeError::ParseError(format!("Failed to parse browse response: {}", e)))?;
            let info = extract_channel_info(&info_response, channel_id);

            // Now fetch sorted videos via continuation token
            let ctoken = build_channel_videos_ctoken(channel_id, sort);
            let sort_body = serde_json::json!({
                "context": innertube_web_context(),
                "continuation": ctoken
            });
            let sort_text = self.post_json_with_retry(INNERTUBE_BROWSE_URL, &sort_body).await?;
            let sort_response: serde_json::Value = serde_json::from_str(&sort_text)
                .map_err(|e| YoutubeError::ParseError(format!("Failed to parse sorted browse response: {}", e)))?;

            (sort_response, info)
        };

        // Parse videos from the response
        let (mut videos, mut continuation) = if sort == ChannelSort::Newest {
            parse_browse_initial(&initial_response, &channel_info.name)?
        } else {
            // Continuation-based responses have a different structure
            parse_browse_continuation(&initial_response, &channel_info.name)?
        };

        // Auto-paginate until we have enough videos
        while videos.len() < limit {
            let token = match continuation.take() {
                Some(t) => t,
                None => break,
            };

            let cont_body = serde_json::json!({
                "context": innertube_web_context(),
                "continuation": token
            });

            let cont_text = self
                .post_json_with_retry(INNERTUBE_BROWSE_URL, &cont_body)
                .await?;
            let cont_response: serde_json::Value = serde_json::from_str(&cont_text).map_err(|e| {
                YoutubeError::ParseError(format!("Failed to parse continuation response: {}", e))
            })?;

            let (page_videos, next_cont) =
                parse_browse_continuation(&cont_response, &channel_info.name)?;
            if page_videos.is_empty() {
                break;
            }
            videos.extend(page_videos);
            continuation = next_cont;
        }

        videos.truncate(limit);

        Ok(ChannelVideosResult {
            channel: channel_info,
            query: None,
            videos,
        })
    }

    /// Search within a channel by fetching the channel's search page.
    ///
    /// YouTube's channel search uses a dedicated page at `/@handle/search?query=...`
    /// which embeds `ytInitialData` JSON containing the search results.
    pub async fn channel_search(
        &self,
        channel_id: &str,
        channel_name: &str,
        query: &str,
        limit: usize,
    ) -> Result<ChannelVideosResult, YoutubeError> {
        // Fetch the channel search page
        let url = format!(
            "https://www.youtube.com/channel/{}/search?query={}",
            channel_id,
            urlencoded(query)
        );

        let html = self.get_with_retry(&url).await?;

        // Extract ytInitialData JSON from the page
        let data_re = Regex::new(r"var ytInitialData\s*=\s*(\{.+?\});\s*</script>").unwrap();
        let initial_data: serde_json::Value = data_re
            .captures(&html)
            .and_then(|c| c.get(1))
            .and_then(|m| serde_json::from_str(m.as_str()).ok())
            .ok_or_else(|| {
                YoutubeError::ParseError(
                    "Could not extract search results from channel page".to_string(),
                )
            })?;

        // Extract channel info from this response too
        let info = extract_channel_info(&initial_data, channel_id);
        let final_name = if info.name != "Unknown" {
            info.name.clone()
        } else {
            channel_name.to_string()
        };

        // Navigate to the search results section
        let mut videos = Vec::new();

        let tabs = initial_data
            .pointer("/contents/twoColumnBrowseResultsRenderer/tabs")
            .and_then(|v| v.as_array());

        if let Some(tabs) = tabs {
            for tab in tabs {
                let sections = tab
                    .pointer("/expandableTabRenderer/content/sectionListRenderer/contents")
                    .and_then(|v| v.as_array())
                    .or_else(|| {
                        tab.pointer("/tabRenderer/content/sectionListRenderer/contents")
                            .and_then(|v| v.as_array())
                    });

                if let Some(sections) = sections {
                    for section in sections {
                        let items = section
                            .pointer("/itemSectionRenderer/contents")
                            .and_then(|v| v.as_array());

                        if let Some(items) = items {
                            for item in items {
                                if videos.len() >= limit {
                                    break;
                                }
                                let renderer = match item.get("videoRenderer") {
                                    Some(r) => r,
                                    None => continue,
                                };
                                if let Some(video) =
                                    parse_video_renderer(renderer, Some(&final_name))
                                {
                                    videos.push(video);
                                }
                            }
                        }
                    }
                }
            }
        }

        videos.truncate(limit);

        Ok(ChannelVideosResult {
            channel: ChannelInfo {
                channel_id: channel_id.to_string(),
                name: final_name,
                handle: info.handle,
                subscriber_count: info.subscriber_count,
                video_count: info.video_count,
            },
            query: Some(query.to_string()),
            videos,
        })
    }
}

/// Build a continuation token for channel videos with a specific sort order.
///
/// Structure (from Invidious reverse engineering):
/// ```text
/// field 80226972 (message) {
///   field 2 (string): channel_id
///   field 3 (string): base64(inner_protobuf)
/// }
/// ```
/// Where inner_protobuf is:
/// ```text
/// field 110 (message) {
///   field 3 (message) {           // videos tab
///     field 15 (message) {
///       field 2 (message) {
///         field 1 (string): "00000000-0000-0000-0000-000000000000"
///       }
///       field 4 (varint): sort_value
///       field 8 (message) {
///         field 1 (string): "00000000-0000-0000-0000-000000000000"
///         field 3 (varint): sort_value
///       }
///     }
///   }
/// }
/// ```
/// Sort values: Newest=4, Popular=2, Oldest=5
fn build_channel_videos_ctoken(channel_id: &str, sort: ChannelSort) -> String {
    let sort_value: u64 = match sort {
        ChannelSort::Newest => 4,
        ChannelSort::Popular => 2,
        ChannelSort::Oldest => 5,
    };

    let zero_uuid = "00000000-0000-0000-0000-000000000000";

    // Build innermost messages bottom-up
    let field2_content = pb_field_string(1, zero_uuid);

    let mut field8_content = pb_field_string(1, zero_uuid);
    field8_content.extend(pb_field_varint(3, sort_value));

    let mut field15_content = pb_field_message(2, &field2_content);
    field15_content.extend(pb_field_varint(4, sort_value));
    field15_content.extend(pb_field_message(8, &field8_content));

    let field3_content = pb_field_message(15, &field15_content);
    let field110_content = pb_field_message(3, &field3_content);
    let inner_msg = pb_field_message(110, &field110_content);

    // Base64-encode inner message (standard base64 with padding)
    let inner_b64 = STANDARD.encode(&inner_msg);

    // Build outer message
    let mut outer_content = pb_field_string(2, channel_id);
    outer_content.extend(pb_field_string(3, &inner_b64));

    let outer_msg = pb_field_message(80226972, &outer_content);

    // Base64-encode the whole thing
    STANDARD.encode(&outer_msg)
}

/// Simple percent-encoding for URL query parameters.
fn urlencoded(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            b' ' => result.push('+'),
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

/// Extract channel info from the initial browse response.
fn extract_channel_info(response: &serde_json::Value, channel_id: &str) -> ChannelInfo {
    // Try c4TabbedHeaderRenderer first
    let header = response.pointer("/header/c4TabbedHeaderRenderer");

    // Try pageHeaderRenderer as fallback
    let page_header = response.pointer("/header/pageHeaderRenderer");

    let name = header
        .and_then(|h| h.get("title"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            page_header
                .and_then(|h| h.pointer("/pageTitle"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            response
                .pointer("/metadata/channelMetadataRenderer/title")
                .and_then(|v| v.as_str())
        })
        .unwrap_or("Unknown")
        .to_string();

    let handle = header
        .and_then(|h| h.pointer("/channelHandleText/runs/0/text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let subscriber_count = header
        .and_then(|h| h.pointer("/subscriberCountText/simpleText"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let video_count = header
        .and_then(|h| h.pointer("/videosCountText/runs/0/text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    ChannelInfo {
        channel_id: channel_id.to_string(),
        name,
        handle,
        subscriber_count,
        video_count,
    }
}

/// Parse the initial browse response: extract videos and continuation token.
fn parse_browse_initial(
    response: &serde_json::Value,
    channel_name: &str,
) -> Result<(Vec<VideoSummary>, Option<String>), YoutubeError> {
    let tabs = response
        .pointer("/contents/twoColumnBrowseResultsRenderer/tabs")
        .and_then(|v| v.as_array())
        .ok_or_else(|| YoutubeError::ParseError("Missing tabs in browse response".to_string()))?;

    let mut grid_contents = None;
    for tab in tabs {
        let contents = tab
            .pointer("/tabRenderer/content/richGridRenderer/contents")
            .and_then(|v| v.as_array());
        if contents.is_some() {
            grid_contents = contents;
            break;
        }
    }

    let items = grid_contents.ok_or_else(|| {
        YoutubeError::ParseError("No video grid found in browse response".to_string())
    })?;

    let (videos, continuation) = extract_videos_from_grid(items, channel_name);
    Ok((videos, continuation))
}

/// Parse a continuation response for more videos.
/// Handles both `appendContinuationItemsAction` (pagination) and
/// `reloadContinuationItemsCommand` (sort-based continuation tokens).
fn parse_browse_continuation(
    response: &serde_json::Value,
    channel_name: &str,
) -> Result<(Vec<VideoSummary>, Option<String>), YoutubeError> {
    let actions = response
        .pointer("/onResponseReceivedActions")
        .and_then(|v| v.as_array());

    let actions = match actions {
        Some(a) => a,
        None => return Ok((Vec::new(), None)),
    };

    let mut all_videos = Vec::new();
    let mut continuation = None;

    for action in actions {
        // Try appendContinuationItemsAction (standard pagination)
        let items = action
            .pointer("/appendContinuationItemsAction/continuationItems")
            .and_then(|v| v.as_array())
            // Try reloadContinuationItemsCommand (sort-based continuation)
            .or_else(|| {
                action
                    .pointer("/reloadContinuationItemsCommand/continuationItems")
                    .and_then(|v| v.as_array())
            });

        if let Some(items) = items {
            let (videos, cont) = extract_videos_from_grid(items, channel_name);
            all_videos.extend(videos);
            if cont.is_some() {
                continuation = cont;
            }
        }
    }

    Ok((all_videos, continuation))
}

/// Extract VideoSummary items and continuation token from a grid contents array.
fn extract_videos_from_grid(
    items: &[serde_json::Value],
    channel_name: &str,
) -> (Vec<VideoSummary>, Option<String>) {
    let mut videos = Vec::new();
    let mut continuation = None;

    for item in items {
        // Video items
        if let Some(renderer) = item.pointer("/richItemRenderer/content/videoRenderer") {
            if let Some(video) = parse_video_renderer(renderer, Some(channel_name)) {
                videos.push(video);
            }
        }

        // Continuation token (usually the last item)
        if let Some(token) = item
            .pointer("/continuationItemRenderer/continuationEndpoint/continuationCommand/token")
            .and_then(|v| v.as_str())
        {
            continuation = Some(token.to_string());
        }
    }

    (videos, continuation)
}
