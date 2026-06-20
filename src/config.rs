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

fn default_port() -> u16 {
    8080
}
fn default_refresh_interval() -> u64 {
    300
}
fn default_retry_count() -> u32 {
    3
}
fn default_retry_backoff() -> u64 {
    5
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            count: default_retry_count(),
            backoff_secs: default_retry_backoff(),
        }
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
                return Err("Auth configuration is ambiguous. Use exactly one of: \
                     (token), (token + token_header), (username + password), or none"
                    .to_string());
            }
        };

        if modes > 1 {
            return Err("Multiple auth modes configured. Use exactly one auth mode".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_minimal_config() {
        let yaml = r#"
cache_dir: "/tmp/cache"
calendars:
  - url: "https://example.com/cal.ics"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(config.port, 8080);
        assert_eq!(config.refresh_interval_secs, 300);
        assert_eq!(config.retry.count, 3);
        assert_eq!(config.retry.backoff_secs, 5);
    }

    #[test]
    fn test_empty_calendars_fails() {
        let yaml = r#"
cache_dir: "/tmp/cache"
calendars: []
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_empty_cache_dir_fails() {
        let yaml = r#"
cache_dir: ""
calendars:
  - url: "https://example.com/cal.ics"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_auth_query_token_only() {
        let yaml = r#"
cache_dir: "/tmp/cache"
calendars:
  - url: "https://example.com/cal.ics"
auth:
  token: "my-token"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_auth_token_with_header() {
        let yaml = r#"
cache_dir: "/tmp/cache"
calendars:
  - url: "https://example.com/cal.ics"
auth:
  token: "my-token"
  token_header: "X-My-Header"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_auth_basic() {
        let yaml = r#"
cache_dir: "/tmp/cache"
calendars:
  - url: "https://example.com/cal.ics"
auth:
  username: "alice"
  password: "hunter2"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_auth_token_and_basic_fails() {
        let yaml = r#"
cache_dir: "/tmp/cache"
calendars:
  - url: "https://example.com/cal.ics"
auth:
  token: "my-token"
  username: "alice"
  password: "hunter2"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_auth_username_without_password_fails() {
        let yaml = r#"
cache_dir: "/tmp/cache"
calendars:
  - url: "https://example.com/cal.ics"
auth:
  username: "alice"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_auth_password_without_username_fails() {
        let yaml = r#"
cache_dir: "/tmp/cache"
calendars:
  - url: "https://example.com/cal.ics"
auth:
  password: "hunter2"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_token_header_without_token_fails() {
        let yaml = r#"
cache_dir: "/tmp/cache"
calendars:
  - url: "https://example.com/cal.ics"
auth:
  token_header: "X-Cal-Token"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_custom_port() {
        let yaml = r#"
port: 9090
cache_dir: "/tmp/cache"
calendars:
  - url: "https://example.com/cal.ics"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.port, 9090);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_custom_refresh_interval() {
        let yaml = r#"
cache_dir: "/tmp/cache"
refresh_interval_secs: 600
calendars:
  - url: "https://example.com/cal.ics"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.refresh_interval_secs, 600);
    }

    #[test]
    fn test_custom_retry() {
        let yaml = r#"
cache_dir: "/tmp/cache"
retry:
  count: 5
  backoff_secs: 10
calendars:
  - url: "https://example.com/cal.ics"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.retry.count, 5);
        assert_eq!(config.retry.backoff_secs, 10);
    }
}
