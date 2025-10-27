//! Context Manager Server Binary
//!
//! This is the main entry point for running the Context Manager as a standalone server.
//! It sets up the Axum web server with full CRUD API, authentication, and rate limiting.

use context_manager::{
    api::{handlers::AppState, routes::build_router},
    config::Config,
    v2::{EmbeddingClientV2 as EmbeddingClient, HiRAGManagerV2 as HiRAGManager},
    vector_db::VectorDbClient,
    middleware::{
        auth::{AuthMiddleware, AuthConfig},
        rate_limiter::{RateLimiter, RateLimitConfig},
        BodyLimiter, BodyLimitConfig,
    },
    observability::{HealthChecker, MetricsCollector},
    hirag::ContextManager,
};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::signal;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration from file

    // Load configuration from file
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    let config = Config::from_file(&config_path)?;
    config.validate()?;

    // Initialize tracing with configuration from config (only once)
    use tracing_subscriber::EnvFilter;
    
    // Apply format based on configuration
    match config.logging.format.as_str() {
        "json" => {
            tracing_subscriber::fmt()
                .with_target(false)
                .with_thread_ids(true)
                .with_level(true)
                .json()
                .with_env_filter(EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new(config.logging.level.clone())))
                .init();
        }
        "compact" => {
            tracing_subscriber::fmt()
                .with_target(false)
                .with_thread_ids(true)
                .with_level(true)
                .compact()
                .with_env_filter(EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new(config.logging.level.clone())))
                .init();
        }
        _ => {
            // Default to pretty format
            tracing_subscriber::fmt()
                .with_target(false)
                .with_thread_ids(true)
                .with_level(true)
                .with_env_filter(EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new(config.logging.level.clone())))
                .init();
        }
    }

    use tracing::info;
    info!("Starting Context Manager Server");
    info!("Configuration loaded and validated from {}", config_path);
    info!("Logging initialized with config settings");

    // Initialize metrics
    let metrics = Arc::new(MetricsCollector::new());

    // Initialize embedding client
    let embedding_client = Arc::new(EmbeddingClient::new(config.embedding.clone())?);
    info!("Embedding client initialized");

    // Initialize vector database
    let vector_db = Arc::new(VectorDbClient::new(config.vector_db.clone()).await?);
    vector_db.initialize_collections().await?;
    info!("Vector database initialized");

    // Initialize HiRAG manager
    let hirag_manager_impl = HiRAGManager::new(
        config.hirag.clone(),
        embedding_client.clone(),
        vector_db.clone(),
    )
    .await?;
    hirag_manager_impl.initialize().await?;
    
    let hirag_manager: Arc<dyn ContextManager> = Arc::new(hirag_manager_impl);
    info!("HiRAG manager initialized");

    // Initialize health checker
    let health_checker = Arc::new(
        HealthChecker::new()
            .with_vector_db(vector_db.clone())
            .with_embedding_client(embedding_client.clone()),
    );
    info!("Health checker initialized");

    // Initialize rate limiter
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig {
        max_requests: 100,
        window_duration: Duration::from_secs(60),
        enabled: true,
    }));
    
    // Start background cleanup task for rate limiter
    rate_limiter.clone().start_cleanup_task();
    info!("Rate limiter initialized with cleanup task");

    // Initialize body size limiter with the smaller of server or protocol limits
    let server_limit_bytes = config.server.max_body_size_mb * 1024 * 1024;
    let protocol_limit_bytes = config.protocol.max_message_size_mb * 1024 * 1024;
    let body_limit_bytes = server_limit_bytes.min(protocol_limit_bytes);
    
    let body_limiter = Arc::new(BodyLimiter::new(BodyLimitConfig {
        max_body_size: body_limit_bytes,
    }));
    info!("Body size limiter initialized with {} MB limit (server: {} MB, protocol: {} MB)", 
          body_limit_bytes / (1024 * 1024),
          config.server.max_body_size_mb,
          config.protocol.max_message_size_mb);

    // Initialize authentication
    let auth_config = AuthConfig {
        enabled: true,
        valid_tokens: std::env::var("API_TOKENS")
            .unwrap_or_else(|_| "default-token".to_string())
            .split(',')
            .map(|s| s.trim().to_string()) // Trim whitespace from tokens
            .collect(),
        token_prefix: "Bearer".to_string(),
    };
    let auth_middleware = Arc::new(AuthMiddleware::new(auth_config));
    info!("Authentication middleware initialized");

    // No circuit breaker available in VectorDbClient
    let circuit_breaker = None;

    // Initialize background GC task if enabled
    if config.hirag.gc_enabled {
        use context_manager::hirag::background::BackgroundTaskManager;
        use std::time::Duration;
        
        let background_manager = Arc::new(BackgroundTaskManager::new(
            vector_db.clone(),
            Duration::from_secs(config.hirag.gc_interval_secs),
            config.hirag.l2_ttl_secs,
            format!("{}_shortterm", config.vector_db.collection_prefix), // L2 collection name
            format!("{}_longterm", config.vector_db.collection_prefix), // L3 collection name
            config.vector_db.vector_size,
        ));
        
        background_manager.clone().start();
        
        info!("Background GC task started with {}s interval", config.hirag.gc_interval_secs);
    } else {
        info!("Background GC task is disabled");
    }

    // Create application state
    let app_state = AppState {
        context_manager: hirag_manager,
        vector_db,
        health_checker: health_checker.clone(),
        circuit_breaker,
    };

    // Build router with all middleware
    let app = build_router(
        app_state,
        health_checker,
        metrics,
        rate_limiter,
        auth_middleware,
        body_limiter,
    );

    // Bind to address
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    info!("Server listening on {}", addr);

    // Start server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown complete");

    Ok(())
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C signal");
        },
        _ = terminate => {
            info!("Received terminate signal");
        },
    }

    info!("Starting graceful shutdown");
}