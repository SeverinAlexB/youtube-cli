use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSummary {
    pub video_id: String,
    pub title: String,
    pub channel: String,
    pub duration: Option<String>,
    pub views: Option<String>,
    pub published: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoDetail {
    pub video_id: String,
    pub title: String,
    pub channel: String,
    pub channel_id: String,
    pub view_count: Option<String>,
    pub length_seconds: Option<u64>,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub is_live: bool,
    pub caption_languages: Vec<CaptionLanguage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionLanguage {
    pub language_code: String,
    pub language_name: String,
    pub is_auto_generated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub text: String,
    pub start: f64,
    pub duration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptResult {
    pub video_id: String,
    pub title: String,
    pub channel: String,
    pub language: String,
    pub language_code: String,
    pub is_auto_generated: bool,
    pub entries: Vec<TranscriptEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub videos: Vec<VideoSummary>,
}
