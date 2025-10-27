//! Observability module for logging, metrics, and tracing

pub mod metrics;
pub mod health;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub use metrics::{MetricsCollector, SystemMetrics};
pub use health::{HealthChecker, SystemHealth, HealthStatus, ComponentHealth};

/// Initialize logging and tracing
pub fn init_observability(log_level: &str, format: &str) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));
    
    match format {
        "json" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().json())
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer())
                .init();
        }
    }
}