//! Background tasks for context management

use crate::error::Result;
use crate::vector_db::{Filter, Condition, VectorStore};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Background task manager for garbage collection
pub struct BackgroundTaskManager {
    vector_db: Arc<dyn VectorStore>,
    gc_interval: Duration,
    l2_ttl_secs: i64,
    l2_collection_name: String,
    l3_collection_name: String,
    vector_size: usize,
}

impl BackgroundTaskManager {
    /// Create a new background task manager
    pub fn new(
        vector_db: Arc<dyn VectorStore>,
        gc_interval: Duration,
        l2_ttl_secs: i64,
        l2_collection_name: String,
        l3_collection_name: String,
        vector_size: usize,
    ) -> Self {
        Self {
            vector_db,
            gc_interval,
            l2_ttl_secs,
            l2_collection_name,
            l3_collection_name,
            vector_size,
        }
    }

    /// Start all background tasks
    pub fn start(self: Arc<Self>) {
        // Start L2 garbage collection task
        let manager = self.clone();
        tokio::spawn(async move {
            manager.run_l2_gc().await;
        });

        info!("Background GC tasks started");
    }

    /// Run L2 garbage collection periodically
    async fn run_l2_gc(&self) {
        let mut ticker = interval(self.gc_interval);

        loop {
            ticker.tick().await;

            debug!("Running L2 garbage collection");

            match self.cleanup_expired_l2_contexts().await {
                Ok(deleted_count) => {
                    if deleted_count > 0 {
                        info!("L2 GC: Deleted {} expired contexts", deleted_count);
                    } else {
                        debug!("L2 GC: No expired contexts found");
                    }
                }
                Err(e) => {
                    error!("L2 GC error: {}", e);
                }
            }
        }
    }

    /// Clean up expired L2 contexts
    async fn cleanup_expired_l2_contexts(&self) -> Result<usize> {
        let now = chrono::Utc::now().timestamp();
        let cutoff_time = now - self.l2_ttl_secs;

        debug!("Starting L2 GC with cutoff time: {}", cutoff_time);

        // Create filter for expired contexts in L2 (short-term) level
        let filter = Filter::new()
            .must(Condition::Match {
                key: "level".to_string(),
                value: serde_json::Value::String("ShortTerm".to_string()),
            })
            .must(Condition::Range {
                key: "timestamp".to_string(),
                gte: None,
                lte: Some(cutoff_time as f64),
            });

        // Search for expired contexts
        let search_params = crate::vector_db::SearchParams {
            vector: vec![0.0; self.vector_size], // Dummy vector for filter-only search
            limit: 1000, // Process in batches of 1000
            score_threshold: None,
            filter: Some(filter),
            with_payload: false,
            with_vector: false,
        };

        match self.vector_db.search(&self.l2_collection_name, search_params).await {
            Ok(results) => {
                if results.is_empty() {
                    return Ok(0);
                }

                let ids: Vec<_> = results.iter().map(|r| r.id).collect();
                let count = ids.len();

                debug!("Found {} expired L2 contexts to delete", count);

                // Delete in batches to avoid overwhelming the database
                const BATCH_SIZE: usize = 100;
                let mut deleted_total = 0;

                for chunk in ids.chunks(BATCH_SIZE) {
                    match self.vector_db.delete_points(&self.l2_collection_name, chunk.to_vec()).await {
                        Ok(_) => {
                            deleted_total += chunk.len();
                            debug!("Deleted batch of {} contexts", chunk.len());
                        }
                        Err(e) => {
                            warn!("Failed to delete batch: {}", e);
                            // Continue with next batch even if one fails
                        }
                    }
                }

                info!(
                    "L2 GC completed: deleted {}/{} expired contexts",
                    deleted_total, count
                );

                Ok(deleted_total)
            }
            Err(e) => {
                error!("Failed to search for expired contexts: {}", e);
                Err(e)
            }
        }
    }

    /// Clean up expired L3 contexts (long-term)
    /// This is more conservative and only removes contexts that are truly expired
    pub async fn cleanup_expired_l3_contexts(&self, l3_ttl_secs: i64) -> Result<usize> {
        let now = chrono::Utc::now().timestamp();
        let cutoff_time = now - l3_ttl_secs;

        debug!("Starting L3 GC with cutoff time: {}", cutoff_time);

        let filter = Filter::new()
            .must(Condition::Match {
                key: "level".to_string(),
                value: serde_json::Value::String("LongTerm".to_string()),
            })
            .must(Condition::Range {
                key: "timestamp".to_string(),
                gte: None,
                lte: Some(cutoff_time as f64),
            });

        let search_params = crate::vector_db::SearchParams {
            vector: vec![0.0; self.vector_size],
            limit: 1000,
            score_threshold: None,
            filter: Some(filter),
            with_payload: false,
            with_vector: false,
        };

        match self.vector_db.search(&self.l3_collection_name, search_params).await {
            Ok(results) => {
                if results.is_empty() {
                    return Ok(0);
                }

                let ids: Vec<_> = results.iter().map(|r| r.id).collect();
                let count = ids.len();

                debug!("Found {} expired L3 contexts to delete", count);

                const BATCH_SIZE: usize = 100;
                let mut deleted_total = 0;

                for chunk in ids.chunks(BATCH_SIZE) {
                    match self.vector_db.delete_points(&self.l3_collection_name, chunk.to_vec()).await {
                        Ok(_) => {
                            deleted_total += chunk.len();
                        }
                        Err(e) => {
                            warn!("Failed to delete L3 batch: {}", e);
                        }
                    }
                }

                info!(
                    "L3 GC completed: deleted {}/{} expired contexts",
                    deleted_total, count
                );

                Ok(deleted_total)
            }
            Err(e) => {
                error!("Failed to search for expired L3 contexts: {}", e);
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Note: These tests would require mock implementations
    // They are placeholders for future implementation

    #[test]
    fn test_background_task_manager_creation() {
        // This would require a mock VectorStore implementation
        // Skipping for now
    }
}