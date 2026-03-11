use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub no_cache: bool,
    pub json_output: bool,
    pub cache_dir: PathBuf,
}

impl AppConfig {
    pub fn load(no_cache: bool, json_output: bool) -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from(".cache"))
            .join("youtube-cli");

        AppConfig {
            no_cache,
            json_output,
            cache_dir,
        }
    }
}
