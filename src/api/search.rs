use crate::api::YouTubeClient;
use crate::cli::{DurationFilter, SortOrder};
use crate::error::YoutubeError;
use crate::model::{SearchResult, VideoSummary};

const INNERTUBE_SEARCH_URL: &str = "https://www.youtube.com/youtubei/v1/search";

impl YouTubeClient {
    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        sort: SortOrder,
        duration: Option<DurationFilter>,
    ) -> Result<SearchResult, YoutubeError> {
        let mut body = serde_json::json!({
            "context": {
                "client": {
                    "clientName": "WEB",
                    "clientVersion": "2.20240101.00.00"
                }
            },
            "query": query
        });

        let params = build_search_params(sort, duration);
        if let Some(params) = params {
            body["params"] = serde_json::Value::String(params);
        }

        let response_text = self
            .post_json_with_retry(INNERTUBE_SEARCH_URL, &body)
            .await?;
        let response: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
            YoutubeError::ParseError(format!("Failed to parse search response: {}", e))
        })?;

        let videos = parse_search_response(&response, limit)?;

        Ok(SearchResult {
            query: query.to_string(),
            videos,
        })
    }
}

fn parse_search_response(
    response: &serde_json::Value,
    limit: usize,
) -> Result<Vec<VideoSummary>, YoutubeError> {
    let mut videos = Vec::new();

    let sections = response
        .pointer(
            "/contents/twoColumnSearchResultsRenderer/primaryContents/sectionListRenderer/contents",
        )
        .and_then(|v| v.as_array())
        .ok_or_else(|| YoutubeError::ParseError("Missing search results structure".to_string()))?;

    for section in sections {
        let items = match section
            .pointer("/itemSectionRenderer/contents")
            .and_then(|v| v.as_array())
        {
            Some(items) => items,
            None => continue,
        };

        for item in items {
            if videos.len() >= limit {
                break;
            }

            let renderer = match item.get("videoRenderer") {
                Some(r) => r,
                None => continue, // skip ads, channels, playlists, etc.
            };

            let video_id = renderer
                .get("videoId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            if video_id.is_empty() {
                continue;
            }

            let title = renderer
                .pointer("/title/runs/0/text")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();

            let channel = renderer
                .pointer("/ownerText/runs/0/text")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();

            let duration = renderer
                .pointer("/lengthText/simpleText")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let views = renderer
                .pointer("/viewCountText/simpleText")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let published = renderer
                .pointer("/publishedTimeText/simpleText")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let description = extract_description(renderer);

            videos.push(VideoSummary {
                video_id,
                title,
                channel,
                duration,
                views,
                published,
                description,
            });
        }

        if videos.len() >= limit {
            break;
        }
    }

    Ok(videos)
}

fn extract_description(renderer: &serde_json::Value) -> Option<String> {
    // Try detailedMetadataSnippets first
    if let Some(snippets) = renderer
        .get("detailedMetadataSnippets")
        .and_then(|v| v.as_array())
    {
        if let Some(snippet) = snippets.first() {
            if let Some(runs) = snippet
                .pointer("/snippetText/runs")
                .and_then(|v| v.as_array())
            {
                let text: String = runs
                    .iter()
                    .filter_map(|r| r.get("text").and_then(|v| v.as_str()))
                    .collect();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }

    // Fallback to descriptionSnippet
    if let Some(runs) = renderer
        .pointer("/descriptionSnippet/runs")
        .and_then(|v| v.as_array())
    {
        let text: String = runs
            .iter()
            .filter_map(|r| r.get("text").and_then(|v| v.as_str()))
            .collect();
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

/// Build the `params` field for InnerTube search filtering.
/// YouTube encodes sort/filter as a base64-encoded protobuf `params` string.
/// These are pre-computed values from YouTube's own filter UI.
fn build_search_params(sort: SortOrder, duration: Option<DurationFilter>) -> Option<String> {
    // YouTube's search params are protobuf-encoded. Known values:
    // Sort: relevance=default(no param), date=CAISAhAB, views=CAMSAhAB, rating=CAISBBABGAE
    // Duration: short=EgIYAQ, medium=EgIYAw, long=EgIYAg
    // Combined sort+duration use different params.

    match (sort, duration) {
        (SortOrder::Relevance, None) => None,
        (SortOrder::Date, None) => Some("CAISAhAB".to_string()),
        (SortOrder::Views, None) => Some("CAMSAhAB".to_string()),
        (SortOrder::Rating, None) => Some("CAESAhAB".to_string()),
        (SortOrder::Relevance, Some(DurationFilter::Short)) => Some("EgIYAQ%3D%3D".to_string()),
        (SortOrder::Relevance, Some(DurationFilter::Medium)) => Some("EgIYAw%3D%3D".to_string()),
        (SortOrder::Relevance, Some(DurationFilter::Long)) => Some("EgIYAg%3D%3D".to_string()),
        // For combined sort + duration, use sort-only params (duration filter may not combine easily)
        (_, Some(_)) => {
            tracing::info!(
                "Duration filter combined with sort may not work perfectly; applying sort only"
            );
            build_search_params(sort, None)
        }
    }
}
