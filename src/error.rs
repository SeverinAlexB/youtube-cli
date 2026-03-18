use thiserror::Error;

#[derive(Error, Debug)]
pub enum YoutubeError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("API error: {0}")]
    Api(String),

    #[error("Video unavailable: {0}")]
    VideoUnavailable(String),

    #[error("No transcripts available for video {0}")]
    NoTranscript(String),

    #[error("Transcript language not available for video {0}: {1}")]
    LanguageNotAvailable(String, String),

    #[error("Channel not found: {0}")]
    ChannelNotFound(String),

    #[error("Rate limited by YouTube (429). Retries exhausted.")]
    RateLimited,

    #[error("Failed to parse YouTube response: {0}")]
    ParseError(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
