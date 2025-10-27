//! Middleware for limiting request body size

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tracing::warn;

/// Configuration for body size limiting
#[derive(Debug, Clone)]
pub struct BodyLimitConfig {
    /// Maximum body size in bytes (0 = unlimited)
    pub max_body_size: usize,
}

impl Default for BodyLimitConfig {
    fn default() -> Self {
        Self {
            max_body_size: 10 * 1024 * 1024, // 10 MB default
        }
    }
}

/// Body size limiting middleware
#[derive(Debug, Clone)]
pub struct BodyLimiter {
    config: BodyLimitConfig,
}

impl BodyLimiter {
    /// Create a new body limiter
    pub fn new(config: BodyLimitConfig) -> Self {
        Self { config }
    }
    
    /// Get the maximum body size in bytes
    pub fn max_body_size(&self) -> usize {
        self.config.max_body_size
    }
}

/// Middleware function for body size limiting
pub async fn body_limit_middleware(
    axum::extract::State(limiter): axum::extract::State<Arc<BodyLimiter>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // If no limit is set, pass through
    if limiter.config.max_body_size == 0 {
        return Ok(next.run(req).await);
    }
    
    // Check Content-Length header if available
    if let Some(content_length) = req.headers().get("content-length") {
        if let Ok(len) = content_length.to_str() {
            if let Ok(len) = len.parse::<usize>() {
                if len > limiter.config.max_body_size {
                    warn!("Request body too large: {} bytes (max: {})", len, limiter.config.max_body_size);
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
            }
        }
    }
    
    // For requests without Content-Length or when we need to check actual body size,
    // we would need to buffer the body which is more complex.
    // For now, we'll rely on the Content-Length header which is sent by most clients.
    
    Ok(next.run(req).await)
}