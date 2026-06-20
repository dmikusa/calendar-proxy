use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,
    pub cache_dir: String,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: u64,
    #[serde(default)]
    pub retry: RetryConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    pub calendars: Vec<CalendarSource>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_retry_count")]
    pub count: u32,
    #[serde(default = "default_retry_backoff")]
    pub backoff_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub token_header: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CalendarSource {
    pub url: String,
}

fn default_port() -> u16 { 8080 }
fn default_refresh_interval() -> u64 { 300 }
fn default_retry_count() -> u32 { 3 }
fn default_retry_backoff() -> u64 { 5 }

impl Default for RetryConfig {
    fn default() -> Self {
        Self { count: default_retry_count(), backoff_secs: default_retry_backoff() }
    }
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.calendars.is_empty() {
            return Err("At least one calendar must be configured".to_string());
        }
        for (i, cal) in self.calendars.iter().enumerate() {
            if cal.url.trim().is_empty() {
                return Err(format!("Calendar at index {i} has an empty URL"));
            }
        }
        if self.cache_dir.trim().is_empty() {
            return Err("cache_dir must not be empty".to_string());
        }

        let auth = &self.auth;
        let has_token = !auth.token.is_empty();
        let has_token_header = !auth.token_header.is_empty();
        let has_basic_user = !auth.username.is_empty();
        let has_basic_pass = !auth.password.is_empty();

        let modes = match (has_token, has_token_header, has_basic_user, has_basic_pass) {
            (false, false, false, false) => 0,
            (true, false, false, false) => 1,
            (true, true, false, false) => 1,
            (false, false, true, true) => 1,
            _ => {
                return Err(
                    "Auth configuration is ambiguous. Use exactly one of: \
                     (token), (token + token_header), (username + password), or none"
                        .to_string(),
                );
            }
        };

        if modes > 1 {
            return Err(
                "Multiple auth modes configured. Use exactly one auth mode".to_string(),
            );
        }

        if has_basic_user && !has_basic_pass {
            return Err("auth.username requires auth.password".to_string());
        }
        if !has_basic_user && has_basic_pass {
            return Err("auth.password requires auth.username".to_string());
        }

        Ok(())
    }
}
