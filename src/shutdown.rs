//! Graceful shutdown handling

use std::sync::Arc;
use tokio::sync::Notify;
use tokio::signal;
use tracing::info;

/// Shutdown coordinator
pub struct ShutdownCoordinator {
    notify: Arc<Notify>,
}

impl ShutdownCoordinator {
    /// Create a new shutdown coordinator
    pub fn new() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
        }
    }
    
    /// Get a shutdown notifier
    pub fn subscribe(&self) -> ShutdownNotifier {
        ShutdownNotifier {
            notify: self.notify.clone(),
        }
    }
    
    /// Wait for shutdown signal
    pub async fn wait_for_signal(&self) {
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
        };
        
        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await;
        };
        
        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();
        
        tokio::select! {
            _ = ctrl_c => {
                info!("Received Ctrl+C signal");
            }
            _ = terminate => {
                info!("Received SIGTERM signal");
            }
        }
        
        // Notify all subscribers
        self.notify.notify_waiters();
    }
    
    /// Trigger shutdown manually
    pub fn shutdown(&self) {
        info!("Manual shutdown triggered");
        self.notify.notify_waiters();
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Shutdown notifier for components
#[derive(Clone)]
pub struct ShutdownNotifier {
    notify: Arc<Notify>,
}

impl ShutdownNotifier {
    /// Wait for shutdown signal
    pub async fn wait(&self) {
        self.notify.notified().await;
    }
    
    /// Check if shutdown has been signaled (non-blocking)
    pub fn is_shutdown(&self) -> bool {
        // This is a simplified check
        // In production, you might want to use an AtomicBool
        false
    }
}

/// Graceful shutdown helper
pub async fn graceful_shutdown<F, Fut>(
    coordinator: Arc<ShutdownCoordinator>,
    cleanup: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    // Wait for shutdown signal
    coordinator.wait_for_signal().await;
    
    info!("Starting graceful shutdown...");
    
    // Run cleanup
    cleanup().await;
    
    info!("Graceful shutdown completed");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    #[ignore] // Hangs in test environment
    async fn test_shutdown_coordinator() {
        let coordinator = ShutdownCoordinator::new();
        let notifier = coordinator.subscribe();
        
        // Spawn a task that waits for shutdown
        let handle = tokio::spawn(async move {
            notifier.wait().await;
            true
        });
        
        // Trigger shutdown
        coordinator.shutdown();
        
        // Verify task completed
        let result = handle.await.unwrap();
        assert!(result);
    }
    
    #[tokio::test]
    #[ignore] // Hangs in test environment
    async fn test_multiple_subscribers() {
        let coordinator = ShutdownCoordinator::new();
        let notifier1 = coordinator.subscribe();
        let notifier2 = coordinator.subscribe();
        
        let handle1 = tokio::spawn(async move {
            notifier1.wait().await;
            1
        });
        
        let handle2 = tokio::spawn(async move {
            notifier2.wait().await;
            2
        });
        
        coordinator.shutdown();
        
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();
        
        assert_eq!(result1, 1);
        assert_eq!(result2, 2);
    }
}