//! Metrics collection and reporting

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// System metrics
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    /// Total requests processed
    pub total_requests: u64,
    
    /// Total errors
    pub total_errors: u64,
    
    /// Active connections
    pub active_connections: usize,
    
    /// Cache hit rate
    pub cache_hit_rate: f64,
    
    /// Average response time (ms)
    pub avg_response_time_ms: f64,
    
    /// Uptime in seconds
    pub uptime_secs: u64,
    
    /// Memory usage (bytes)
    pub memory_usage_bytes: usize,
}

/// Latency histogram buckets (in milliseconds)
const LATENCY_BUCKETS: &[f64] = &[1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0];

/// Histogram for tracking latency distribution
#[derive(Debug, Clone)]
pub struct Histogram {
    buckets: Vec<(f64, Arc<AtomicU64>)>,
    sum: Arc<AtomicU64>,
    count: Arc<AtomicU64>,
}

impl Histogram {
    fn new(buckets: &[f64]) -> Self {
        let bucket_counters = buckets
            .iter()
            .map(|&b| (b, Arc::new(AtomicU64::new(0))))
            .collect();
        
        Self {
            buckets: bucket_counters,
            sum: Arc::new(AtomicU64::new(0)),
            count: Arc::new(AtomicU64::new(0)),
        }
    }
    
    fn observe(&self, value: f64) {
        // Update sum and count (value is already in milliseconds)
        self.sum.fetch_add(value as u64, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        
        // Update buckets (cumulative - all buckets >= value get incremented)
        for (bucket, counter) in &self.buckets {
            if value <= *bucket {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    
    fn export_prometheus(&self, name: &str, help: &str) -> String {
        let mut output = String::new();
        
        output.push_str(&format!("# HELP {} {}\n", name, help));
        output.push_str(&format!("# TYPE {} histogram\n", name));
        
        // Export buckets (already cumulative from observe())
        for (bucket, counter) in &self.buckets {
            let count = counter.load(Ordering::Relaxed);
            output.push_str(&format!("{}_bucket{{le=&quot;{}&quot;}} {}\n", name, bucket, count));
        }
        
        // Export +Inf bucket
        let total_count = self.count.load(Ordering::Relaxed);
        output.push_str(&format!("{}_bucket{{le=&quot;+Inf&quot;}} {}\n", name, total_count));
        
        // Export sum and count (sum is already in milliseconds)
        let sum = self.sum.load(Ordering::Relaxed) as f64;
        output.push_str(&format!("{}_sum {:.3}\n", name, sum));
        output.push_str(&format!("{}_count {}\n", name, total_count));
        
        output
    }
}

/// Metrics collector
pub struct MetricsCollector {
    start_time: Instant,
    total_requests: Arc<AtomicU64>,
    total_errors: Arc<AtomicU64>,
    active_connections: Arc<AtomicUsize>,
    total_response_time_ms: Arc<AtomicU64>,
    cache_hits: Arc<AtomicU64>,
    cache_misses: Arc<AtomicU64>,
    
    // Histograms for latency tracking
    request_latency: Histogram,
    embedding_latency: Histogram,
    vector_db_latency: Histogram,
    
    // GC metrics
    gc_runs: Arc<AtomicU64>,
    gc_deleted_total: Arc<AtomicU64>,
    gc_errors: Arc<AtomicU64>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            total_requests: Arc::new(AtomicU64::new(0)),
            total_errors: Arc::new(AtomicU64::new(0)),
            active_connections: Arc::new(AtomicUsize::new(0)),
            total_response_time_ms: Arc::new(AtomicU64::new(0)),
            cache_hits: Arc::new(AtomicU64::new(0)),
            cache_misses: Arc::new(AtomicU64::new(0)),
            request_latency: Histogram::new(LATENCY_BUCKETS),
            embedding_latency: Histogram::new(LATENCY_BUCKETS),
            vector_db_latency: Histogram::new(LATENCY_BUCKETS),
            gc_runs: Arc::new(AtomicU64::new(0)),
            gc_deleted_total: Arc::new(AtomicU64::new(0)),
            gc_errors: Arc::new(AtomicU64::new(0)),
        }
    }
    
    /// Record a request
    pub fn record_request(&self, response_time: Duration) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        let ms = response_time.as_millis() as u64;
        self.total_response_time_ms.fetch_add(ms, Ordering::Relaxed);
        self.request_latency.observe(ms as f64);
    }
    
    /// Record embedding operation latency
    pub fn record_embedding_latency(&self, duration: Duration) {
        self.embedding_latency.observe(duration.as_millis() as f64);
    }
    
    /// Record vector database operation latency
    pub fn record_vector_db_latency(&self, duration: Duration) {
        self.vector_db_latency.observe(duration.as_millis() as f64);
    }
    
    /// Record GC run
    pub fn record_gc_run(&self, deleted_count: usize, _duration: Duration) {
        self.gc_runs.fetch_add(1, Ordering::Relaxed);
        self.gc_deleted_total.fetch_add(deleted_count as u64, Ordering::Relaxed);
        // Could add GC latency histogram here if needed
    }
    
    /// Record GC error
    pub fn record_gc_error(&self) {
        self.gc_errors.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record an error
    pub fn record_error(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Increment active connections
    pub fn increment_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Decrement active connections
    pub fn decrement_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }
    
    /// Record cache hit
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record cache miss
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Get current metrics
    pub fn get_metrics(&self) -> SystemMetrics {
        let total_requests = self.total_requests.load(Ordering::Relaxed);
        let total_errors = self.total_errors.load(Ordering::Relaxed);
        let active_connections = self.active_connections.load(Ordering::Relaxed);
        let total_response_time = self.total_response_time_ms.load(Ordering::Relaxed);
        let cache_hits = self.cache_hits.load(Ordering::Relaxed);
        let cache_misses = self.cache_misses.load(Ordering::Relaxed);
        
        let avg_response_time_ms = if total_requests > 0 {
            total_response_time as f64 / total_requests as f64
        } else {
            0.0
        };
        
        let cache_total = cache_hits + cache_misses;
        let cache_hit_rate = if cache_total > 0 {
            cache_hits as f64 / cache_total as f64
        } else {
            0.0
        };
        
        let uptime_secs = self.start_time.elapsed().as_secs();
        
        // Get memory usage (approximate)
        let memory_usage_bytes = get_memory_usage();
        
        SystemMetrics {
            total_requests,
            total_errors,
            active_connections,
            cache_hit_rate,
            avg_response_time_ms,
            uptime_secs,
            memory_usage_bytes,
        }
    }
    
    /// Export metrics in Prometheus format
    pub fn export_prometheus(&self) -> String {
        let metrics = self.get_metrics();
        let gc_runs = self.gc_runs.load(Ordering::Relaxed);
        let gc_deleted = self.gc_deleted_total.load(Ordering::Relaxed);
        let gc_errors = self.gc_errors.load(Ordering::Relaxed);
        
        let mut output = format!(
            "# HELP context_manager_requests_total Total number of requests\n\
             # TYPE context_manager_requests_total counter\n\
             context_manager_requests_total {}\n\
             \n\
             # HELP context_manager_errors_total Total number of errors\n\
             # TYPE context_manager_errors_total counter\n\
             context_manager_errors_total {}\n\
             \n\
             # HELP context_manager_active_connections Current active connections\n\
             # TYPE context_manager_active_connections gauge\n\
             context_manager_active_connections {}\n\
             \n\
             # HELP context_manager_cache_hit_rate Cache hit rate\n\
             # TYPE context_manager_cache_hit_rate gauge\n\
             context_manager_cache_hit_rate {:.4}\n\
             \n\
             # HELP context_manager_avg_response_time_ms Average response time in milliseconds\n\
             # TYPE context_manager_avg_response_time_ms gauge\n\
             context_manager_avg_response_time_ms {:.2}\n\
             \n\
             # HELP context_manager_uptime_seconds Uptime in seconds\n\
             # TYPE context_manager_uptime_seconds counter\n\
             context_manager_uptime_seconds {}\n\
             \n\
             # HELP context_manager_memory_usage_bytes Memory usage in bytes\n\
             # TYPE context_manager_memory_usage_bytes gauge\n\
             context_manager_memory_usage_bytes {}\n\
             \n\
             # HELP context_manager_gc_runs_total Total number of GC runs\n\
             # TYPE context_manager_gc_runs_total counter\n\
             context_manager_gc_runs_total {}\n\
             \n\
             # HELP context_manager_gc_deleted_total Total contexts deleted by GC\n\
             # TYPE context_manager_gc_deleted_total counter\n\
             context_manager_gc_deleted_total {}\n\
             \n\
             # HELP context_manager_gc_errors_total Total GC errors\n\
             # TYPE context_manager_gc_errors_total counter\n\
             context_manager_gc_errors_total {}\n\
             \n",
            metrics.total_requests,
            metrics.total_errors,
            metrics.active_connections,
            metrics.cache_hit_rate,
            metrics.avg_response_time_ms,
            metrics.uptime_secs,
            metrics.memory_usage_bytes,
            gc_runs,
            gc_deleted,
            gc_errors,
        );
        
        // Add histograms
        output.push_str(&self.request_latency.export_prometheus(
            "context_manager_request_duration_ms",
            "Request duration in milliseconds"
        ));
        output.push('\n');
        
        output.push_str(&self.embedding_latency.export_prometheus(
            "context_manager_embedding_duration_ms",
            "Embedding operation duration in milliseconds"
        ));
        output.push('\n');
        
        output.push_str(&self.vector_db_latency.export_prometheus(
            "context_manager_vector_db_duration_ms",
            "Vector database operation duration in milliseconds"
        ));
        
        output
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Get approximate memory usage
fn get_memory_usage() -> usize {
    // This is a placeholder - in production, use a proper memory profiling library
    // or read from /proc/self/status on Linux
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();
        
        collector.record_request(Duration::from_millis(100));
        collector.record_request(Duration::from_millis(200));
        collector.record_error();
        collector.increment_connections();
        collector.record_cache_hit();
        collector.record_cache_miss();
        
        let metrics = collector.get_metrics();
        
        assert_eq!(metrics.total_requests, 2);
        assert_eq!(metrics.total_errors, 1);
        assert_eq!(metrics.active_connections, 1);
        assert_eq!(metrics.avg_response_time_ms, 150.0);
        assert_eq!(metrics.cache_hit_rate, 0.5);
    }
    
    #[test]
    fn test_prometheus_export() {
        let collector = MetricsCollector::new();
        collector.record_request(Duration::from_millis(100));
        
        let prometheus = collector.export_prometheus();
        
        assert!(prometheus.contains("context_manager_requests_total 1"));
        assert!(prometheus.contains("context_manager_avg_response_time_ms 100.00"));
    }
}