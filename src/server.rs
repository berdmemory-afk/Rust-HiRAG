//! HTTP server for health checks and metrics

use crate::observability::{HealthChecker, MetricsCollector};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use std::sync::Arc;
use tracing::info;

/// Server state
#[derive(Clone)]
pub struct ServerState {
    pub health_checker: Arc<HealthChecker>,
    pub metrics_collector: Arc<MetricsCollector>,
}

/// Create HTTP server router
pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/health/liveness", get(liveness_check))
        .route("/health/readiness", get(readiness_check))
        .route("/metrics", get(metrics))
        .with_state(state)
}

/// Full health check endpoint
async fn health_check(
    State(state): State<ServerState>,
) -> Result<Json<crate::observability::SystemHealth>, AppError> {
    let health = state.health_checker.check_health().await;
    Ok(Json(health))
}

/// Liveness check endpoint
async fn liveness_check(
    State(state): State<ServerState>,
) -> Result<StatusCode, AppError> {
    if state.health_checker.liveness() {
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::SERVICE_UNAVAILABLE)
    }
}

/// Readiness check endpoint
async fn readiness_check(
    State(state): State<ServerState>,
) -> Result<StatusCode, AppError> {
    if state.health_checker.readiness().await {
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::SERVICE_UNAVAILABLE)
    }
}

/// Metrics endpoint (Prometheus format)
async fn metrics(
    State(state): State<ServerState>,
) -> Result<String, AppError> {
    let metrics = state.metrics_collector.export_prometheus();
    Ok(metrics)
}

/// Start HTTP server
pub async fn start_server(
    addr: &str,
    state: ServerState,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_router(state);
    
    info!("Starting HTTP server on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}

/// Application error wrapper
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Internal error: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_router_creation() {
        let state = ServerState {
            health_checker: Arc::new(HealthChecker::new()),
            metrics_collector: Arc::new(MetricsCollector::new()),
        };
        
        let _router = create_router(state);
        // Just verify router can be created
    }
}