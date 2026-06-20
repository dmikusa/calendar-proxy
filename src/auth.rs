use axum::{
    extract::{FromRequestParts, Query},
    http::{request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::Engine;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum AuthMode {
    None,
    QueryToken { token: String },
    HeaderToken { token: String, header_name: String },
    BasicAuth { username: String, password: String },
}

impl AuthMode {
    pub fn from_config(auth: &crate::config::AuthConfig) -> Self {
        let has_token = !auth.token.is_empty();
        let has_token_header = !auth.token_header.is_empty();
        let has_basic_user = !auth.username.is_empty();
        let has_basic_pass = !auth.password.is_empty();

        match (has_token, has_token_header, has_basic_user, has_basic_pass) {
            (true, true, false, false) => Self::HeaderToken {
                token: auth.token.clone(),
                header_name: auth.token_header.clone(),
            },
            (true, false, false, false) => {
                Self::QueryToken { token: auth.token.clone() }
            }
            (false, false, true, true) => Self::BasicAuth {
                username: auth.username.clone(),
                password: auth.password.clone(),
            },
            _ => Self::None,
        }
    }
}

#[derive(Debug)]
pub struct AuthError {
    pub status: StatusCode,
    pub message: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}

pub struct Authenticated;

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for Authenticated
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Clone the AuthMode so we don't hold an immutable borrow on `parts`
        let mode = {
            let extensions = &parts.extensions;
            extensions
                .get::<AuthMode>()
                .cloned()
                .ok_or_else(|| AuthError {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: "Auth not configured on server".to_string(),
                })?
        };

        match mode {
            AuthMode::None => Ok(Authenticated),
            AuthMode::QueryToken { token } => {
                let query: Query<HashMap<String, String>> =
                    Query::from_request_parts(parts, _state)
                        .await
                        .map_err(|_| AuthError {
                            status: StatusCode::UNAUTHORIZED,
                            message: "Missing token query parameter".to_string(),
                        })?;
                match query.0.get("token") {
                    Some(t) if t == &token => Ok(Authenticated),
                    _ => Err(AuthError {
                        status: StatusCode::UNAUTHORIZED,
                        message: "Invalid or missing token".to_string(),
                    }),
                }
            }
            AuthMode::HeaderToken { token, header_name } => {
                let headers = &parts.headers;
                match headers.get(header_name.as_str()) {
                    Some(value) if value == &token => Ok(Authenticated),
                    _ => Err(AuthError {
                        status: StatusCode::UNAUTHORIZED,
                        message: format!("Invalid or missing {header_name} header"),
                    }),
                }
            }
            AuthMode::BasicAuth { username, password } => {
                let headers = &parts.headers;
                match parse_basic_auth(headers) {
                    Some((u, p)) if u == username && p == password => Ok(Authenticated),
                    _ => Err(AuthError {
                        status: StatusCode::UNAUTHORIZED,
                        message: "Invalid credentials".to_string(),
                    }),
                }
            }
        }
    }
}

fn parse_basic_auth(headers: &HeaderMap) -> Option<(String, String)> {
    let auth_header = headers.get("Authorization")?.to_str().ok()?;
    let auth_header = auth_header.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(auth_header)
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (user, pass) = decoded.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn test_parse_basic_auth_valid() {
        let mut headers = HeaderMap::new();
        let encoded = base64::engine::general_purpose::STANDARD
            .encode("alice:hunter2");
        headers.insert("Authorization", format!("Basic {encoded}").parse().unwrap());
        let result = parse_basic_auth(&headers);
        assert_eq!(result, Some(("alice".into(), "hunter2".into())));
    }

    #[test]
    fn test_parse_basic_auth_missing_header() {
        let headers = HeaderMap::new();
        assert_eq!(parse_basic_auth(&headers), None);
    }

    #[test]
    fn test_parse_basic_auth_wrong_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Bearer token123".parse().unwrap());
        assert_eq!(parse_basic_auth(&headers), None);
    }

    #[test]
    fn test_parse_basic_auth_invalid_base64() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Basic !!!invalid!!!".parse().unwrap());
        assert_eq!(parse_basic_auth(&headers), None);
    }

    #[test]
    fn test_parse_basic_auth_missing_colon() {
        let mut headers = HeaderMap::new();
        let encoded = base64::engine::general_purpose::STANDARD
            .encode("justausername");
        headers.insert("Authorization", format!("Basic {encoded}").parse().unwrap());
        assert_eq!(parse_basic_auth(&headers), None);
    }

    #[test]
    fn test_auth_mode_none() {
        let mut auth = crate::config::AuthConfig::default();
        let mode = AuthMode::from_config(&auth);
        assert!(matches!(mode, AuthMode::None));
        auth.token = "".into();
        assert!(matches!(mode, AuthMode::None));
    }

    #[test]
    fn test_auth_mode_query_token() {
        let mut auth = crate::config::AuthConfig::default();
        auth.token = "secret".into();
        let mode = AuthMode::from_config(&auth);
        match mode {
            AuthMode::QueryToken { token } => assert_eq!(token, "secret"),
            _ => panic!("Expected QueryToken"),
        }
    }

    #[test]
    fn test_auth_mode_header_token() {
        let mut auth = crate::config::AuthConfig::default();
        auth.token = "secret".into();
        auth.token_header = "X-Cal-Token".into();
        let mode = AuthMode::from_config(&auth);
        match mode {
            AuthMode::HeaderToken { token, header_name } => {
                assert_eq!(token, "secret");
                assert_eq!(header_name, "X-Cal-Token");
            }
            _ => panic!("Expected HeaderToken"),
        }
    }

    #[test]
    fn test_auth_mode_basic() {
        let mut auth = crate::config::AuthConfig::default();
        auth.username = "alice".into();
        auth.password = "hunter2".into();
        let mode = AuthMode::from_config(&auth);
        match mode {
            AuthMode::BasicAuth { username, password } => {
                assert_eq!(username, "alice");
                assert_eq!(password, "hunter2");
            }
            _ => panic!("Expected BasicAuth"),
        }
    }

    #[test]
    fn test_auth_mode_token_with_all_defaults_is_none() {
        let mut auth = crate::config::AuthConfig::default();
        auth.token_header = "X-Cal-Token".into();
        // token_header without token -> falls to None
        let mode = AuthMode::from_config(&auth);
        assert!(matches!(mode, AuthMode::None));
    }
}
