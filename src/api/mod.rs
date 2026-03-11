pub mod search;
pub mod transcript;

use crate::error::YoutubeError;
use reqwest::Client;
use std::time::{Duration, Instant};
use tokio::time::sleep;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub struct YouTubeClient {
    client: Client,
    last_request: std::sync::Mutex<Option<Instant>>,
}

impl YouTubeClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            last_request: std::sync::Mutex::new(None),
        }
    }

    async fn rate_limit(&self) {
        let min_interval = Duration::from_millis(1000); // 1 req/sec
        let wait = {
            let mut last = self.last_request.lock().unwrap();
            let now = Instant::now();
            match *last {
                Some(t) => {
                    let elapsed = now.duration_since(t);
                    if elapsed < min_interval {
                        *last = Some(t + min_interval);
                        Some(min_interval - elapsed)
                    } else {
                        *last = Some(now);
                        None
                    }
                }
                None => {
                    *last = Some(now);
                    None
                }
            }
        };
        if let Some(wait) = wait {
            sleep(wait).await;
        }
    }

    pub(crate) async fn get_with_retry(&self, url: &str) -> Result<String, YoutubeError> {
        let mut rate_limit_retries = 0u32;
        let mut server_error_retries = 0u32;

        loop {
            self.rate_limit().await;

            let response = self.client.get(url).send().await?;
            let status = response.status();

            if status.is_success() {
                return Ok(response.text().await?);
            }

            if status.as_u16() == 429 && rate_limit_retries < 3 {
                rate_limit_retries += 1;
                tracing::warn!("Rate limited, retrying in 2s ({}/3)", rate_limit_retries);
                sleep(Duration::from_secs(2)).await;
                continue;
            }

            if status.is_server_error() && server_error_retries < 1 {
                server_error_retries += 1;
                tracing::warn!("Server error {}, retrying in 2s", status);
                sleep(Duration::from_secs(2)).await;
                continue;
            }

            if status.as_u16() == 429 {
                return Err(YoutubeError::RateLimited);
            }

            return Err(YoutubeError::Api(format!(
                "YouTube returned status {}",
                status
            )));
        }
    }

    pub(crate) async fn post_json_with_retry(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<String, YoutubeError> {
        let mut rate_limit_retries = 0u32;
        let mut server_error_retries = 0u32;

        loop {
            self.rate_limit().await;

            let response = self.client.post(url).json(body).send().await?;
            let status = response.status();

            if status.is_success() {
                return Ok(response.text().await?);
            }

            if status.as_u16() == 429 && rate_limit_retries < 3 {
                rate_limit_retries += 1;
                tracing::warn!("Rate limited, retrying in 2s ({}/3)", rate_limit_retries);
                sleep(Duration::from_secs(2)).await;
                continue;
            }

            if status.is_server_error() && server_error_retries < 1 {
                server_error_retries += 1;
                tracing::warn!("Server error {}, retrying in 2s", status);
                sleep(Duration::from_secs(2)).await;
                continue;
            }

            if status.as_u16() == 429 {
                return Err(YoutubeError::RateLimited);
            }

            return Err(YoutubeError::Api(format!(
                "YouTube returned status {}",
                status
            )));
        }
    }
}
