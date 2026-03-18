use crate::error::YoutubeError;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub struct Cache {
    dir: PathBuf,
    read_enabled: bool,
}

pub struct CacheHit<T> {
    pub data: T,
    pub cached_at: SystemTime,
}

const SEARCH_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60); // 7 days
const TRANSCRIPT_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60); // 30 days
const VIDEO_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60); // 7 days
const CHANNEL_TTL: Duration = Duration::from_secs(24 * 60 * 60); // 1 day
const CHANNEL_RESOLVE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60); // 30 days

impl Cache {
    pub fn new(cache_dir: PathBuf, no_cache: bool) -> Self {
        Self {
            dir: cache_dir,
            read_enabled: !no_cache,
        }
    }

    pub fn get_search<T: DeserializeOwned>(&self, cache_key: &str) -> Option<CacheHit<T>> {
        if !self.read_enabled {
            return None;
        }
        let path = self.dir.join(format!("search_{}.json", cache_key));
        self.read_cached(&path, SEARCH_TTL)
    }

    pub fn set_search<T: Serialize>(&self, cache_key: &str, data: &T) -> Result<(), YoutubeError> {
        let path = self.dir.join(format!("search_{}.json", cache_key));
        self.write_cached(&path, data)
    }

    pub fn get_transcript<T: DeserializeOwned>(
        &self,
        video_id: &str,
        lang: &str,
    ) -> Option<CacheHit<T>> {
        if !self.read_enabled {
            return None;
        }
        let path = self
            .dir
            .join(format!("transcript_{}_{}.json", video_id, lang));
        self.read_cached(&path, TRANSCRIPT_TTL)
    }

    pub fn set_transcript<T: Serialize>(
        &self,
        video_id: &str,
        lang: &str,
        data: &T,
    ) -> Result<(), YoutubeError> {
        let path = self
            .dir
            .join(format!("transcript_{}_{}.json", video_id, lang));
        self.write_cached(&path, data)
    }

    pub fn get_video<T: DeserializeOwned>(&self, video_id: &str) -> Option<CacheHit<T>> {
        if !self.read_enabled {
            return None;
        }
        let path = self.dir.join(format!("video_{}.json", video_id));
        self.read_cached(&path, VIDEO_TTL)
    }

    pub fn set_video<T: Serialize>(&self, video_id: &str, data: &T) -> Result<(), YoutubeError> {
        let path = self.dir.join(format!("video_{}.json", video_id));
        self.write_cached(&path, data)
    }

    pub fn get_channel<T: DeserializeOwned>(&self, cache_key: &str) -> Option<CacheHit<T>> {
        if !self.read_enabled {
            return None;
        }
        let path = self.dir.join(format!("channel_{}.json", cache_key));
        self.read_cached(&path, CHANNEL_TTL)
    }

    pub fn set_channel<T: Serialize>(&self, cache_key: &str, data: &T) -> Result<(), YoutubeError> {
        let path = self.dir.join(format!("channel_{}.json", cache_key));
        self.write_cached(&path, data)
    }

    pub fn get_channel_id<T: DeserializeOwned>(&self, handle: &str) -> Option<CacheHit<T>> {
        if !self.read_enabled {
            return None;
        }
        let normalized = handle.trim_start_matches('@').to_lowercase();
        let path = self.dir.join(format!("channel_id_{}.json", normalized));
        self.read_cached(&path, CHANNEL_RESOLVE_TTL)
    }

    pub fn set_channel_id<T: Serialize>(
        &self,
        handle: &str,
        data: &T,
    ) -> Result<(), YoutubeError> {
        let normalized = handle.trim_start_matches('@').to_lowercase();
        let path = self.dir.join(format!("channel_id_{}.json", normalized));
        self.write_cached(&path, data)
    }

    pub fn channel_cache_key(channel_id: &str, sort: &str, query: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(channel_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(sort.as_bytes());
        hasher.update(b"\0");
        hasher.update(query.as_bytes());
        let result = hasher.finalize();
        hex::encode(&result[..8])
    }

    pub fn search_cache_key(query: &str, sort: &str, duration: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(query.as_bytes());
        hasher.update(b"\0");
        hasher.update(sort.as_bytes());
        hasher.update(b"\0");
        hasher.update(duration.as_bytes());
        let result = hasher.finalize();
        hex::encode(&result[..8])
    }

    fn read_cached<T: DeserializeOwned>(&self, path: &Path, ttl: Duration) -> Option<CacheHit<T>> {
        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata.modified().ok()?;
        let age = SystemTime::now().duration_since(modified).ok()?;
        if age > ttl {
            tracing::debug!("Cache expired for {}", path.display());
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        match serde_json::from_str(&content) {
            Ok(data) => {
                tracing::info!("Cache hit for {}", path.display());
                Some(CacheHit {
                    data,
                    cached_at: modified,
                })
            }
            Err(e) => {
                tracing::warn!("Cache parse error for {}: {}", path.display(), e);
                None
            }
        }
    }

    fn write_cached<T: Serialize>(&self, path: &Path, data: &T) -> Result<(), YoutubeError> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| YoutubeError::Cache(format!("Failed to create cache dir: {}", e)))?;
        let content = serde_json::to_string_pretty(data)?;
        std::fs::write(path, content)
            .map_err(|e| YoutubeError::Cache(format!("Failed to write cache: {}", e)))?;
        tracing::debug!("Cached to {}", path.display());
        Ok(())
    }
}
