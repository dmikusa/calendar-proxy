use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct CacheManager {
    cache_path: PathBuf,
}

impl CacheManager {
    pub fn new(cache_dir: &str) -> Self {
        let path = PathBuf::from(cache_dir).join("calendar.ics");
        Self { cache_path: path }
    }

    pub fn cache_path(&self) -> &std::path::Path {
        &self.cache_path
    }

    pub fn write_atomic(&self, content: &str) -> Result<(), std::io::Error> {
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp_path = self.cache_path.with_extension("ics.tmp");
        std::fs::write(&tmp_path, content)?;
        std::fs::rename(&tmp_path, &self.cache_path)?;
        Ok(())
    }

    pub fn read(&self) -> Result<String, std::io::Error> {
        std::fs::read_to_string(&self.cache_path)
    }
}

pub async fn refresh_cache(
    manager: CacheManager,
    calendars: &[crate::config::CalendarSource],
    retry_count: u32,
    retry_backoff_secs: u64,
    ready: Arc<RwLock<bool>>,
) {
    match fetch_and_merge(calendars, retry_count, retry_backoff_secs).await {
        Ok(calendar) => {
            let ics = calendar.to_ics_string();
            if let Err(e) = manager.write_atomic(&ics) {
                tracing::error!("Failed to write cache: {e}");
            } else {
                tracing::info!("Cache refreshed successfully");
                *ready.write().await = true;
            }
        }
        Err(e) => {
            tracing::error!("Failed to refresh cache (all calendars failed): {e}");
        }
    }
}

async fn fetch_and_merge(
    calendars: &[crate::config::CalendarSource],
    retry_count: u32,
    retry_backoff_secs: u64,
) -> Result<crate::calendar::SanitizedCalendar, String> {
    let results = fetch_all_calendars(calendars, retry_count, retry_backoff_secs).await;
    let mut merged = crate::calendar::SanitizedCalendar::new();
    let mut any_success = false;

    for result in results {
        match result {
            Ok(cal) => {
                merged.merge(cal);
                any_success = true;
            }
            Err(e) => {
                tracing::warn!("Calendar fetch failed: {e}");
            }
        }
    }

    if any_success {
        Ok(merged)
    } else {
        Err("No calendars could be fetched".to_string())
    }
}

async fn fetch_all_calendars(
    calendars: &[crate::config::CalendarSource],
    retry_count: u32,
    retry_backoff_secs: u64,
) -> Vec<Result<crate::calendar::SanitizedCalendar, String>> {
    use futures::future::join_all;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    let tasks: Vec<_> = calendars
        .iter()
        .map(|cal| {
            let client = client.clone();
            let url = cal.url.clone();
            let retry_count = retry_count;
            let backoff = retry_backoff_secs;
            tokio::spawn(async move {
                fetch_with_retry(&client, &url, retry_count, backoff).await
            })
        })
        .collect();

    join_all(tasks)
        .await
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| Err(format!("Task join error: {e}"))))
        .collect()
}

async fn fetch_with_retry(
    client: &reqwest::Client,
    url: &str,
    max_retries: u32,
    initial_backoff_secs: u64,
) -> Result<crate::calendar::SanitizedCalendar, String> {
    let mut attempt = 0u32;
    loop {
        match client.get(url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let body = resp.text().await.map_err(|e| format!("Read body: {e}"))?;
                    return crate::calendar::parse_ics(&body);
                } else {
                    let status = resp.status();
                    return Err(format!("HTTP {status} for {url}"));
                }
            }
            Err(e) => {
                attempt += 1;
                if attempt > max_retries {
                    return Err(format!("Fetch failed after {max_retries} retries: {e}"));
                }
                let delay = initial_backoff_secs * (2u64.pow(attempt - 1));
                tracing::warn!(
                    "Fetch attempt {attempt}/{max_retries} failed for {url}, \
                     retrying in {delay}s: {e}"
                );
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            }
        }
    }
}
