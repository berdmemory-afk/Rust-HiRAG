//! API request handlers

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    hirag::{ContextManager, ContextRequest, Priority},
    vector_db::{ContextLevel, circuit_breaker::CircuitBreaker},
};

use crate::vector_db::VectorStore;
use crate::observability::HealthChecker;

/// Application state
#[derive(Clone)]
pub struct AppState {
    pub context_manager: Arc<dyn ContextManager>,
    pub vector_db: Arc<dyn VectorStore>,
    pub health_checker: Arc<HealthChecker>,
    pub circuit_breaker: Option<Arc<CircuitBreaker>>,
}

/// Request to store a context
#[derive(Debug, Deserialize)]
pub struct StoreContextRequest {
    pub text: String,
    pub level: ContextLevel,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// Response from storing a context
#[derive(Debug, Serialize)]
pub struct StoreContextResponse {
    pub id: Uuid,
}

/// Request to search contexts
#[derive(Debug, Deserialize)]
pub struct SearchContextRequest {
    pub query: String,
    pub max_tokens: usize,
    #[serde(default)]
    pub levels: Vec<ContextLevel>,
    #[serde(default)]
    pub priority: Priority,
    pub session_id: Option<String>,
}

/// Request to delete a context
#[derive(Debug, Deserialize)]
pub struct DeleteContextRequest {
    pub id: Uuid,
}

/// Generic success response
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub message: String,
}

/// Generic error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Store a new context
pub async fn store_context(
    State(state): State<AppState>,
    Json(req): Json<StoreContextRequest>,
) -> impl IntoResponse {
    // Validate metadata before storing
    use crate::middleware::validator::InputValidator;
    for (key, value) in &req.metadata {
        if let Err(e) = InputValidator::validate_metadata_key(key) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid metadata key '{}': {}", key, e),
                }),
            ).into_response();
        }
        if let Err(e) = InputValidator::validate_metadata_value(value) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid metadata value for key '{}': {}", key, e),
                }),
            ).into_response();
        }
    }
    
    match state.context_manager.store_context(&req.text, req.level, req.metadata).await {
        Ok(id) => (
            StatusCode::CREATED,
            Json(StoreContextResponse { id }),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ).into_response(),
    }
}

/// Search for contexts
pub async fn search_contexts(
    State(state): State<AppState>,
    Json(req): Json<SearchContextRequest>,
) -> impl IntoResponse {
    let context_req = ContextRequest {
        query: req.query,
        max_tokens: req.max_tokens,
        levels: req.levels,
        filters: None,
        priority: req.priority,
        session_id: req.session_id,
    };

    match state.context_manager.retrieve_context(context_req).await {
        Ok(response) => (
            StatusCode::OK,
            Json(response),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ).into_response(),
    }
}

/// Delete a context
pub async fn delete_context(
    State(state): State<AppState>,
    Json(req): Json<DeleteContextRequest>,
) -> impl IntoResponse {
    match state.context_manager.delete_context(req.id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(SuccessResponse {
                message: format!("Context {} deleted", req.id),
            }),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ).into_response(),
    }
}

/// Clear contexts by level
pub async fn clear_level(
    State(state): State<AppState>,
    Json(level): Json<ContextLevel>,
) -> impl IntoResponse {
    match state.context_manager.clear_level(level).await {
        Ok(_) => (
            StatusCode::OK,
            Json(SuccessResponse {
                message: format!("Level {:?} cleared", level),
            }),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ).into_response(),
    }
}