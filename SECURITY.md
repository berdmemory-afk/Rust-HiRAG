# Security Guide

## Overview

This document outlines the security features, best practices, and threat model for the Context Manager system.

## Security Features

### 1. Authentication & Authorization

#### HMAC-SHA256 Message Authentication

All protocol messages can be authenticated using HMAC-SHA256 signatures:

```rust
use context_manager::protocol::auth::{generate_signature, AuthConfig};

// Generate signature
let signature = generate_signature(
    &secret,
    &message.id.to_string(),
    message.timestamp,
    &message.sender,
)?;

// Add to message metadata
message.metadata.insert(
    "signature".to_string(),
    serde_json::Value::String(signature),
);
```

#### Configuration

```bash
# Set authentication secret
export PROTOCOL_SECRET="your-256-bit-secret-key-here"
```

**Best Practices**:
- Use cryptographically secure random strings (32+ characters)
- Rotate secrets regularly (every 90 days recommended)
- Store secrets in secure secret management systems (Vault, AWS Secrets Manager)
- Never commit secrets to version control

### 2. Transport Security

#### TLS Configuration

Enable TLS for all external connections:

```toml
[vector_db]
url = "https://qdrant.example.com:6334"
tls_enabled = true
tls_verify = true
tls_cert_path = "/path/to/ca-cert.pem"  # Optional
```

**Certificate Management**:
- Use certificates from trusted CAs
- Implement certificate rotation
- Monitor certificate expiration
- Use certificate pinning for critical connections

### 3. Input Validation

#### Message Size Limits

- **Maximum Size**: 10MB per message
- **Enforcement**: Both encoding and decoding
- **Rationale**: Prevents memory exhaustion attacks

#### Timestamp Validation

- **Window**: 5 minutes (configurable)
- **Protection**: Prevents replay attacks
- **Implementation**: Automatic in authentication layer

#### Content Validation

- **JSON/MessagePack**: Built-in schema validation via serde
- **Depth Limits**: Prevents stack overflow attacks
- **Type Safety**: Rust's type system prevents injection attacks

### 4. Data Protection

#### Secrets Management

**DO NOT**:
- ❌ Hardcode API tokens in code
- ❌ Commit secrets to version control
- ❌ Log sensitive data
- ❌ Include secrets in error messages

**DO**:
- ✅ Use environment variables
- ✅ Use secret management systems
- ✅ Implement secret rotation
- ✅ Audit secret access

#### Sensitive Data Handling

```rust
// Mark sensitive fields
#[derive(Debug)]
pub struct Config {
    #[serde(skip_serializing)]  // Don't log
    pub api_token: String,
}

// Redact in logs
impl std::fmt::Debug for SensitiveData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SensitiveData")
            .field("token", &"[REDACTED]")
            .finish()
    }
}
```

### 5. Network Security

#### Rate Limiting

Implement at the reverse proxy level:

```nginx
limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;

location /api {
    limit_req zone=api burst=20 nodelay;
    proxy_pass http://context-manager:8080;
}
```

#### IP Whitelisting

```nginx
# Allow only specific IPs
allow 10.0.0.0/8;
allow 172.16.0.0/12;
deny all;
```

### 6. Operational Security

#### Logging

**Log Security Events**:
- Authentication failures
- Authorization denials
- Unusual request patterns
- Configuration changes
- System errors

**DO NOT Log**:
- API tokens or secrets
- Full message payloads (may contain PII)
- Passwords or credentials
- Sensitive user data

#### Monitoring

**Alert On**:
- High error rates
- Authentication failures
- Unusual traffic patterns
- Resource exhaustion
- Circuit breaker trips

## Threat Model

### Threats Addressed

| Threat | Mitigation | Status |
|--------|------------|--------|
| DoS via large messages | Size limits (10MB) | ✅ Implemented |
| Replay attacks | Timestamp validation | ✅ Implemented |
| Unauthorized access | HMAC authentication | ✅ Implemented |
| Man-in-the-middle | TLS encryption | ✅ Implemented |
| Injection attacks | Type-safe deserialization | ✅ Implemented |
| Resource exhaustion | Circuit breakers | ✅ Implemented |
| Cascading failures | Circuit breakers, retries | ✅ Implemented |

### Threats Requiring Additional Mitigation

| Threat | Current Status | Recommendation |
|--------|----------------|----------------|
| DDoS attacks | Partial (size limits) | Add rate limiting at proxy |
| Insider threats | None | Implement audit logging |
| Data exfiltration | None | Add data access monitoring |
| Privilege escalation | None | Implement RBAC |
| Supply chain attacks | Partial (deps audit) | Regular security scans |

## Security Best Practices

### 1. Deployment

```bash
# Use non-root user
RUN useradd -m -u 1000 appuser
USER appuser

# Read-only filesystem
docker run --read-only \
  --tmpfs /tmp \
  context-manager:latest

# Drop capabilities
docker run --cap-drop=ALL \
  --cap-add=NET_BIND_SERVICE \
  context-manager:latest
```

### 2. Configuration

```toml
# Use strong secrets
[auth]
secret = "${PROTOCOL_SECRET}"  # 32+ characters

# Enable all security features
[vector_db]
tls_enabled = true
tls_verify = true

# Set conservative limits
[protocol]
max_message_size = 10485760  # 10MB
max_connections = 1000
```

### 3. Network

```yaml
# Kubernetes NetworkPolicy
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: context-manager-policy
spec:
  podSelector:
    matchLabels:
      app: context-manager
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - podSelector:
        matchLabels:
          app: api-gateway
    ports:
    - protocol: TCP
      port: 8080
  egress:
  - to:
    - podSelector:
        matchLabels:
          app: qdrant
    ports:
    - protocol: TCP
      port: 6334
```

### 4. Monitoring

```yaml
# Alert on security events
- alert: HighAuthFailureRate
  expr: rate(auth_failures_total[5m]) > 10
  for: 5m
  labels:
    severity: critical
  annotations:
    summary: "High authentication failure rate"

- alert: UnusualTrafficPattern
  expr: rate(requests_total[5m]) > 1000
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Unusual traffic pattern detected"
```

## Compliance

### GDPR Considerations

**Data Minimization**:
- Only store necessary context data
- Implement data retention policies
- Provide data deletion capabilities

**Right to be Forgotten**:
```rust
// Delete all user contexts
hirag_manager.delete_user_contexts(user_id).await?;
```

**Data Portability**:
```rust
// Export user contexts
let contexts = hirag_manager.export_user_contexts(user_id).await?;
```

### Audit Logging

Implement audit logs for:
- Context creation/modification/deletion
- Authentication events
- Configuration changes
- Administrative actions

## Incident Response

### Security Incident Checklist

1. **Detection**
   - Monitor alerts
   - Review logs
   - Check metrics

2. **Containment**
   - Isolate affected systems
   - Revoke compromised credentials
   - Enable additional logging

3. **Investigation**
   - Collect logs and metrics
   - Analyze attack vectors
   - Identify scope of breach

4. **Recovery**
   - Patch vulnerabilities
   - Rotate secrets
   - Restore from backups if needed

5. **Post-Incident**
   - Document incident
   - Update security measures
   - Conduct retrospective

## Security Updates

### Dependency Management

```bash
# Check for vulnerabilities
cargo audit

# Update dependencies
cargo update

# Review security advisories
cargo deny check advisories
```

### Update Schedule

- **Critical**: Immediate (within 24 hours)
- **High**: Within 1 week
- **Medium**: Within 1 month
- **Low**: Next release cycle

## Contact

For security issues:
- **Email**: security@example.com
- **PGP Key**: [link]
- **Bug Bounty**: [link]

**Please do not disclose security issues publicly until they have been addressed.**

## Acknowledgments

We thank the security research community for responsible disclosure of vulnerabilities.