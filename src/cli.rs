use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "youtube-cli",
    version,
    about = "Search YouTube videos and download transcripts from the command line"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Bypass the local cache and fetch fresh data
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// Output raw JSON instead of Markdown
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Search YouTube videos
    Search {
        /// Search query (e.g., "rust programming tutorial")
        query: String,

        /// Max number of results (1-50)
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Sort order
        #[arg(long, value_enum, default_value_t = SortOrder::Relevance)]
        sort: SortOrder,

        /// Filter by video duration
        #[arg(long, value_enum)]
        duration: Option<DurationFilter>,
    },

    /// Fetch the transcript/captions for a video
    Transcript {
        /// YouTube video ID or URL
        video: String,

        /// Preferred language code (e.g., "en", "de", "es")
        #[arg(long, default_value = "en")]
        lang: String,

        /// Include timestamps in output
        #[arg(long)]
        timestamps: bool,
    },

    /// Get detailed information about a video
    Video {
        /// YouTube video ID or URL
        video: String,
    },

    /// List or search videos from a YouTube channel
    Channel {
        /// Channel ID, handle (@name), or channel URL
        channel: String,

        /// Search within the channel's videos
        #[arg(long)]
        search: Option<String>,

        /// Max number of results (1-200)
        #[arg(long, default_value = "30")]
        limit: usize,

        /// Sort order for video listing (ignored when --search is used)
        #[arg(long, value_enum, default_value_t = ChannelSort::Newest)]
        sort: ChannelSort,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SortOrder {
    Relevance,
    Date,
    Views,
    Rating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ChannelSort {
    Newest,
    Oldest,
    Popular,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DurationFilter {
    Short,
    Medium,
    Long,
}
