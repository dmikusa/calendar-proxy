use axum::response::IntoResponse;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;

mod auth;
mod cache;
mod calendar;
mod config;

struct AppState {
    cache_manager: cache::CacheManager,
    ready: Arc<RwLock<bool>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "calendar_proxy=info".into()),
        )
        .init();

    let config_path =
        std::env::var("CALENDAR_PROXY_CONFIG").unwrap_or_else(|_| "config.yaml".to_string());

    let cfg = config::Config::from_file(&config_path).unwrap_or_else(|e| {
        tracing::error!("Failed to load config: {e}");
        std::process::exit(1);
    });

    let cache_mgr = cache::CacheManager::new(&cfg.cache_dir);

    // Ensure cache directory exists
    if let Some(parent) = cache_mgr.cache_path().parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| {
            tracing::error!("Failed to create cache directory: {e}");
            std::process::exit(1);
        });
    }

    let ready = Arc::new(RwLock::new(false));

    // Initial fetch (blocking — don't start server until we have data)
    cache::refresh_cache(
        cache_mgr.clone(),
        &cfg.calendars,
        cfg.retry.count,
        cfg.retry.backoff_secs,
        &cfg.passthrough,
        ready.clone(),
    )
    .await;

    // Check if we got any data
    if !*ready.read().await {
        tracing::error!("Initial calendar fetch failed — no calendars could be loaded. Exiting.");
        std::process::exit(1);
    }

    // Spawn background refresh loop
    {
        let cache_mgr = cache_mgr.clone();
        let calendars = cfg.calendars.clone();
        let retry_count = cfg.retry.count;
        let retry_backoff = cfg.retry.backoff_secs;
        let interval = cfg.refresh_interval_secs;
        let ready = ready.clone();

        let passthrough = cfg.passthrough.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                cache::refresh_cache(
                    cache_mgr.clone(),
                    &calendars,
                    retry_count,
                    retry_backoff,
                    &passthrough,
                    ready.clone(),
                )
                .await;
            }
        });
    }

    // Determine auth mode
    let auth_mode = auth::AuthMode::from_config(&cfg.auth);

    let state = Arc::new(AppState {
        cache_manager: cache_mgr,
        ready,
    });

    let app = axum::Router::new()
        .route("/calendar.ics", axum::routing::get(handler_calendar))
        .route("/health", axum::routing::get(handler_health))
        .layer(axum::Extension(auth_mode))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cfg.port);
    tracing::info!("Starting server on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to bind {addr}: {e}");
            std::process::exit(1);
        });

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Server error: {e}");
            std::process::exit(1);
        });
}

async fn handler_calendar(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    _auth: auth::Authenticated,
) -> Result<axum::response::Response, (axum::http::StatusCode, String)> {
    match state.cache_manager.read() {
        Ok(content) => {
            let headers = [(
                axum::http::header::CONTENT_TYPE,
                "text/calendar; charset=utf-8",
            )];
            Ok((headers, content).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to read cache: {e}");
            Err((
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Cache not available".to_string(),
            ))
        }
    }
}

async fn handler_health(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::http::StatusCode {
    if *state.ready.read().await {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    let sigterm = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await
    };
    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received SIGINT, shutting down"),
        _ = sigterm => tracing::info!("Received SIGTERM, shutting down"),
    }
}
