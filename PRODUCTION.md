# Production Deployment Guide

## Overview

This guide covers deploying the Context Manager system to production with security, reliability, and performance best practices.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Security Configuration](#security-configuration)
3. [Infrastructure Setup](#infrastructure-setup)
4. [Configuration](#configuration)
5. [Deployment](#deployment)
6. [Monitoring](#monitoring)
7. [Troubleshooting](#troubleshooting)

## Prerequisites

### Required Services

- **Qdrant Vector Database**: v1.7+ (with TLS enabled)
- **Chutes API**: Valid API token with sufficient quota
- **Rust**: 1.70+ for building
- **Docker**: 20.10+ (optional, for containerized deployment)

### System Requirements

- **CPU**: 4+ cores recommended
- **RAM**: 8GB+ (depends on cache size and concurrent connections)
- **Storage**: 10GB+ for logs and temporary files
- **Network**: Low-latency connection to Qdrant and Chutes API

## Security Configuration

### 1. API Token Management

**Never commit tokens to version control!**

```bash
# Set environment variables
export CHUTES_API_TOKEN="your-secure-token-here"
export QDRANT_API_KEY="your-qdrant-api-key"
export PROTOCOL_SECRET="your-hmac-secret-key"
```

### 2. TLS Configuration

Enable TLS for Qdrant connections:

```toml
[vector_db]
url = "https://your-qdrant-instance:6334"
tls_enabled = true
tls_verify = true
tls_cert_path = "/path/to/ca-cert.pem"  # Optional
```

### 3. Protocol Authentication

Configure HMAC authentication for protocol messages:

```rust
use context_manager::protocol::auth::{AuthConfig, authenticate_message};

let auth_config = AuthConfig {
    secret: std::env::var("PROTOCOL_SECRET").expect("PROTOCOL_SECRET not set"),
    validate_timestamp: true,
    max_age_secs: 300, // 5 minutes
};

// Authenticate incoming messages
authenticate_message(
    &auth_config,
    &message.id.to_string(),
    message.timestamp,
    &message.sender,
    message.metadata.get("signature").and_then(|v| v.as_str()),
)?;
```

### 4. Message Size Limits

The system enforces a 10MB message size limit by default. Adjust if needed:

```rust
// In src/protocol/codec.rs
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10MB
```

## Infrastructure Setup

### Qdrant Deployment

#### Docker Compose

```yaml
version: '3.8'

services:
  qdrant:
    image: qdrant/qdrant:v1.7.0
    ports:
      - "6333:6333"
      - "6334:6334"
    volumes:
      - ./qdrant_storage:/qdrant/storage
      - ./qdrant_snapshots:/qdrant/snapshots
    environment:
      - QDRANT__SERVICE__GRPC_PORT=6334
      - QDRANT__SERVICE__HTTP_PORT=6333
    restart: unless-stopped
```

#### Production Recommendations

- Use persistent volumes for data
- Enable authentication
- Configure TLS
- Set up regular backups
- Monitor resource usage

### Circuit Breaker Configuration

```rust
use context_manager::vector_db::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};

let cb_config = CircuitBreakerConfig {
    failure_threshold: 5,      // Open after 5 failures
    success_threshold: 2,      // Close after 2 successes
    timeout: Duration::from_secs(60), // Wait 60s before retry
    window_size: Duration::from_secs(60),
};

let circuit_breaker = CircuitBreaker::new(cb_config);
```

## Configuration

### Production Config Template

Create `config/production.toml`:

```toml
[embedding]
api_url = "https://api.chutes.ai/v1"
api_token = "${CHUTES_API_TOKEN}"
model = "intfloat/multilingual-e5-large"
batch_size = 32
timeout_secs = 30
max_retries = 3
cache_enabled = true
cache_ttl_secs = 3600
cache_size = 10000

[vector_db]
url = "https://qdrant.example.com:6334"
api_key = "${QDRANT_API_KEY}"
collection_prefix = "prod_context"
vector_size = 1024
distance = "Cosine"
timeout_secs = 30
tls_enabled = true
tls_verify = true

[hirag]
l1_size = 50
l2_size = 200
l3_enabled = true
max_context_tokens = 4000
relevance_threshold = 0.7
token_estimator = "WordBased"
retrieval_strategy = "Balanced"
```

### Environment-Specific Overrides

```bash
# Load configuration
export CONFIG_ENV=production
export CONFIG_PATH=/etc/context-manager/config

# Override specific values
export EMBEDDING__API_TOKEN="your-token"
export VECTOR_DB__URL="https://qdrant.prod:6334"
```

## Deployment

### Building for Production

```bash
# Build optimized release
cargo build --release

# Binary location
./target/release/context-manager
```

### Docker Deployment

Create `Dockerfile`:

```dockerfile
FROM rust:1.75-slim as builder

WORKDIR /app
COPY . .

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/context-manager /usr/local/bin/

EXPOSE 8080

CMD ["context-manager"]
```

Build and run:

```bash
docker build -t context-manager:latest .
docker run -d \
  --name context-manager \
  -p 8080:8080 \
  -e CHUTES_API_TOKEN="${CHUTES_API_TOKEN}" \
  -e QDRANT_API_KEY="${QDRANT_API_KEY}" \
  -v /etc/context-manager:/config:ro \
  context-manager:latest
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: context-manager
spec:
  replicas: 3
  selector:
    matchLabels:
      app: context-manager
  template:
    metadata:
      labels:
        app: context-manager
    spec:
      containers:
      - name: context-manager
        image: context-manager:latest
        ports:
        - containerPort: 8080
        env:
        - name: CHUTES_API_TOKEN
          valueFrom:
            secretKeyRef:
              name: context-manager-secrets
              key: chutes-token
        - name: QDRANT_API_KEY
          valueFrom:
            secretKeyRef:
              name: context-manager-secrets
              key: qdrant-key
        resources:
          requests:
            memory: "2Gi"
            cpu: "1000m"
          limits:
            memory: "4Gi"
            cpu: "2000m"
        livenessProbe:
          httpGet:
            path: /health/liveness
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health/readiness
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 5
```

## Monitoring

### Metrics Endpoints

The system exposes Prometheus-compatible metrics:

```bash
# Get metrics
curl http://localhost:8080/metrics

# Example output:
# context_manager_requests_total 1234
# context_manager_errors_total 5
# context_manager_active_connections 10
# context_manager_cache_hit_rate 0.85
# context_manager_avg_response_time_ms 45.23
```

### Health Checks

```bash
# Liveness check (is the service running?)
curl http://localhost:8080/health/liveness

# Readiness check (is the service ready to accept traffic?)
curl http://localhost:8080/health/readiness

# Full health check
curl http://localhost:8080/health
```

### Prometheus Configuration

```yaml
scrape_configs:
  - job_name: 'context-manager'
    static_configs:
      - targets: ['context-manager:8080']
    scrape_interval: 15s
    metrics_path: /metrics
```

### Grafana Dashboard

Key metrics to monitor:

1. **Request Rate**: `rate(context_manager_requests_total[5m])`
2. **Error Rate**: `rate(context_manager_errors_total[5m])`
3. **Response Time**: `context_manager_avg_response_time_ms`
4. **Cache Hit Rate**: `context_manager_cache_hit_rate`
5. **Active Connections**: `context_manager_active_connections`

### Alerting Rules

```yaml
groups:
  - name: context_manager
    rules:
      - alert: HighErrorRate
        expr: rate(context_manager_errors_total[5m]) > 0.05
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High error rate detected"
          
      - alert: SlowResponseTime
        expr: context_manager_avg_response_time_ms > 1000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Slow response times detected"
          
      - alert: LowCacheHitRate
        expr: context_manager_cache_hit_rate < 0.5
        for: 10m
        labels:
          severity: info
        annotations:
          summary: "Cache hit rate below 50%"
```

## Troubleshooting

### Common Issues

#### 1. High Memory Usage

**Symptoms**: OOM kills, slow performance

**Solutions**:
- Reduce cache size in configuration
- Decrease L1/L2/L3 context sizes
- Increase container memory limits
- Monitor for memory leaks

#### 2. Slow Response Times

**Symptoms**: High latency, timeouts

**Solutions**:
- Check Qdrant connection latency
- Verify Chutes API response times
- Increase timeout values
- Enable caching if disabled
- Check circuit breaker status

#### 3. Circuit Breaker Open

**Symptoms**: Requests failing with circuit breaker errors

**Solutions**:
- Check Qdrant health and connectivity
- Review error logs for root cause
- Verify network connectivity
- Consider increasing failure threshold
- Manually reset circuit breaker if needed

#### 4. Authentication Failures

**Symptoms**: "Invalid signature" or "Missing signature" errors

**Solutions**:
- Verify PROTOCOL_SECRET is set correctly
- Check message timestamp (must be within 5 minutes)
- Ensure signature is included in message metadata
- Verify HMAC generation matches on both sides

### Debug Mode

Enable debug logging:

```bash
export RUST_LOG=debug
export RUST_BACKTRACE=1
```

### Performance Profiling

```bash
# Install flamegraph
cargo install flamegraph

# Profile the application
cargo flamegraph --bin context-manager

# View flamegraph.svg in browser
```

## Best Practices

### 1. Capacity Planning

- **Cache Size**: Set to 10-20% of expected unique contexts
- **L1 Size**: 50-100 for most use cases
- **L2 Size**: 200-500 for most use cases
- **Connection Pool**: Match expected concurrent users

### 2. Backup Strategy

- Regular Qdrant snapshots (daily recommended)
- Configuration backups
- Log retention policy (30-90 days)

### 3. Scaling

- **Horizontal**: Deploy multiple instances behind load balancer
- **Vertical**: Increase CPU/RAM for single instance
- **Database**: Use Qdrant cluster for high availability

### 4. Security Checklist

- [ ] TLS enabled for all external connections
- [ ] API tokens stored in secrets management
- [ ] HMAC authentication enabled
- [ ] Message size limits configured
- [ ] Regular security updates applied
- [ ] Access logs enabled and monitored
- [ ] Rate limiting configured (if applicable)

## Support

For issues and questions:
- GitHub Issues: [link]
- Documentation: [link]
- Community: [link]

## License

[Your License Here]