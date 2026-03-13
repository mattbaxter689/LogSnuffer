# LogSnuffer

**Intelligent Log Analysis and Incident Management System**

LogSnuffer is an autonomous log monitoring system that uses AI to analyze error patterns, assess severity, and automatically create GitHub issues with historical context. It combines real-time log aggregation with LLM-powered analysis to reduce manual incident response time.

Will be adding / swapping some of the additional pieces for additional llm tool calls, along with an llm managed state to pass through the lifecycle

### Checklist

[ ] Add in an agent Context
[ ] Add an agent state
[ ] Add in Critical error tool
[ ] Add warning tool
[ ] Add final output tool
[ ] Add Log gathering tool
[ ] add /metrics endpoint and logging properly

---

## Personal Note

While this is not an agentic system, this is an AI augmented log analyzer tool. The bones for this project are in place, and I can turn this into an agentic system with just some small schema changes. This is definitely something that I will be exploring. Additionally, many of the issues I faced with this are related to SQLite databases due to the file system needing manipulation across the pods, containers, etc. This caused the bigest pain point, but I wanted a simple database for this project. Also, Kafka could be used instead of Redis, but again, I wanted a project I can start with minimal setup. This is also why you will see the docker setup, along with the helm charts. I wanted to get experience with helm charts since they are used professionally, and while I don't create them day to day, I wanted to understand how they work in systems like this.

As noted toward the end of this file, there are examples of how the project can be updated/upgraded. I want to highlight that I want to implement the prometheus metrics, as well as the rate limiting. These are something that are critical regardless of the system, and I think that they are very important to add to systems like this, which will have a very high throughput.

I am satisfied with where this project is, but I want to take it further to be a true agentic system, that can handle production-like operations. The next steps for me will be to implement the actual agentic system, create the metrics for prometheus, as well as the rate limiting or authentication

---

## Overview

LogSnuffer addresses the challenge of distinguishing critical incidents from transient errors in high-volume production environments. The system:

- Aggregates logs from multiple services in real-time
- Computes confidence metrics using time-series analysis
- Uses an LLM agent to analyze error patterns and triage severity
- Automatically creates GitHub issues for critical errors
- Links related incidents through similarity matching
- Prevents duplicate issue creation
- Tracks sub-threshold errors as warnings

---

## Problem Statement

In production systems, operations teams face:

- **High signal-to-noise ratio**: Distinguishing critical errors from expected transient failures
- **Manual triage overhead**: Deciding which errors warrant immediate attention
- **Context loss**: Difficulty connecting current incidents to historical resolutions
- **Duplicate work**: Creating redundant issues for recurring problems

LogSnuffer automates this triage process, ensuring critical issues are tracked while filtering operational noise.

---

## Architecture

```
Log Generator Client
        |
        | HTTP POST /api/logs
        v
┌─────────────────────────────────┐
│      LogSnuffer Server           │
│                                  │
│  ┌────────────────────────────┐ │
│  │   HTTP API (Axum)           │ │
│  │   - Ingest logs             │ │
│  │   - Query confidence        │ │
│  │   - GitHub webhooks         │ │
│  └────────────┬───────────────┘ │
│               |                  │
│  ┌────────────v───────────────┐ │
│  │   Background Worker         │ │
│  │   - Rotate time buckets     │ │
│  │   - Compute confidence      │ │
│  │   - Trigger agent           │ │
│  └────────────┬───────────────┘ │
│               |                  │
│  ┌────────────v───────────────┐ │
│  │   Redis Metrics Engine      │ │
│  │   - 30s rolling window      │ │
│  │   - Error rate calculation  │ │
│  │   - Pattern detection       │ │
│  └────────────┬───────────────┘ │
│               |                  │
│               | High Confidence  │
│               v                  │
│  ┌────────────────────────────┐ │
│  │   LLM Agent (Ollama)        │ │
│  │   - Analyze patterns        │ │
│  │   - Triage severity         │ │
│  │   - Suggest fixes           │ │
│  └────────┬──────────┬────────┘ │
│           |          |           │
│           v          v           │
│      SQLite     GitHub API       │
└─────────────────────────────────┘
```

### Components

**API Server (Axum)**: Receives log batches via HTTP POST and serves metrics endpoints

**Background Worker**: Runs every second to rotate time buckets, compute confidence scores, and trigger agent analysis when thresholds are met

**Redis Metrics Engine**: Maintains a 30-second rolling window of log data with:

- Total log counts per bucket
- Error log counts per bucket
- Unique pod instances
- Message pattern hashing

This can and should be a longer window. I decided on 30 seconds to allow for the log bursts to appear faster, and to ensure I can test the system

**Confidence Calculation**: Multi-factor scoring based on:

- Error rate spike detection (short-term vs. baseline)
- Dominant message pattern frequency
- Pod spread distribution
- Time-weighted decay

**LLM Agent**: Uses Ollama (Qwen model) to:

- Analyze error patterns with context
- Classify severity (critical/high/medium)
- Decide which errors warrant GitHub issues vs. warnings
- Suggest investigation steps

**GitHub Integration**:

- Creates issues with detailed context
- Links to similar historical issues
- Prevents duplicate issue creation (>70% similarity)
- Detects regressions (similar to recently closed issues)
- Adds comments to existing issues when duplicates detected

**SQLite Database**: Stores logs, GitHub issue metadata, and warnings for historical analysis

---

## Installation

### Prerequisites

- Rust 1.75+
- Redis 7+
- Ollama with available model. I pulled a qwen coder model with increased context
- GitHub Personal Access Token (repo scope or issues: read/write)

### Setup

```bash
# Clone repository
git clone https://github.com/yourusername/logsnuffer.git
cd logsnuffer

# Build project
cargo build --release
```

### Configuration

Create `.env` file:

```bash
GITHUB_TOKEN=ghp_your_github_token
GITHUB_OWNER=your-username
GITHUB_REPO=your-repo-name
REDISU_URL=your-redis-url
```

Depending on the redis instance used, you will need to configure the URL to make sure you are able to access the instance. Additionally, this project makes use of Helm charts. You will need to create a `secrets.yaml` or something similar, that contains the same environment variables found in the `.env` file.

---

## Usage

### Start Services

```bash
# with docker
just service

# with helm
helm repo add bitnami <https://charts.bitnami.com/bitnami>
helm install redis bitnami/redis
just helm

# if updating the chart and needing updating
just upgrade-helm

# To stop the helm service
just down-helm
```

## How It Works

### 1. Log Ingestion

Logs are sent to `/api/logs` and:

- Stored in Redis buckets (1 bucket per second)
- Written to SQLite asynchronously
- Aggregated by error pattern and pod

### 2. Confidence Computation

Every second, the system:

- Rotates to a new time bucket
- Compares recent error rates (last 3 seconds) to baseline (last 27 seconds)
- Computes a confidence score (0.0 to 1.0) based on:
  - Error rate spike ratio
  - Dominant message pattern frequency
  - Number of affected pods

### 3. Agent Trigger

When confidence exceeds threshold (configurable via planner):

- Agent fetches last 15 error patterns with occurrence counts
- Agent analyzes patterns using LLM
- Agent decides which errors need GitHub issues vs. warnings

### 4. Issue Creation

For each critical error:

- Checks for duplicate open issues (>70% similarity)
  - If found: Adds comment to existing issue instead
- Checks for recently closed issues (>60% similarity, <7 days)
  - If found: Marks new issue as regression
- Creates GitHub issue with:
  - Error description and suggested fix
  - Links to top 3 related closed issues
  - Automatic labels (severity, automated, regression)

### 5. Issue Tracking

- GitHub webhook updates database when issues are closed
- Closed issues become searchable for future similarity matching
- System learns from historical resolutions

---

## Duplicate Prevention

The system prevents duplicate issues through:

1. **Similarity Matching**: Calculates Jaccard similarity on error patterns and issue titles
2. **Open Issue Check**: If >70% similar to an open issue, adds a comment instead
3. **Regression Detection**: If >60% similar to issue closed within 7 days, creates new issue with regression label
4. **Bidirectional Linking**: References top 3 closed issues and adds backlink comments

---

## Future Improvements

### High Priority

- **PostgreSQL Support**: Replace SQLite for better concurrency handling.
- **Configurable Thresholds**: Move hardcoded values (similarity thresholds, time windows) to configuration file
- **Rate Limiting**: Prevent API abuse and LLM overuse
- **Prometheus Metrics**: Export confidence scores and error rates for monitoring

### Medium Priority

- **Web Dashboard**: Real-time visualization of confidence trends and active incidents
- **Multi-Tenancy**: Support multiple teams/projects with isolated data
- **Advanced Similarity**: Use embeddings instead of keyword-based matching
- **Custom Alert Rules**: User-defined conditions for issue creation

### Low Priority

- **Slack/Discord Notifications**: Alert channels when critical issues are created
- **Distributed Tracing Integration**: Link errors to traces automatically
- **Anomaly Detection**: ML-based detection beyond rule-based confidence
- **Issue Templates**: Customizable GitHub issue formats per error type

---

## Limitations

- **SQLite Concurrency**: High log volumes may cause database locking; consider PostgreSQL
- **LLM Latency**: Agent analysis takes 5-30 seconds; not suitable for sub-second SLAs
- **Memory Usage**: Redis stores 30 seconds of logs in memory; adjust window size for high-volume systems
- **Single Instance**: Current design assumes one server instance; scaling requires Redis coordination

### Components

**API Server (Axum)**: Receives log batches via HTTP POST and serves metrics endpoints

**Background Worker**: Runs every second to rotate time buckets, compute confidence scores, and trigger agent analysis when thresholds are met

**Redis Metrics Engine**: Maintains a 30-second rolling window of log data with:

- Total log counts per bucket
- Error log counts per bucket
- Unique pod instances
- Message pattern hashing

**Confidence Calculation**: Multi-factor scoring based on:

- Error rate spike detection (short-term vs. baseline)
- Dominant message pattern frequency
- Pod spread distribution
- Time-weighted decay

**LLM Agent**: Uses Ollama (Qwen model) to:

- Analyze error patterns with context
- Classify severity (critical/high/medium)
- Decide which errors warrant GitHub issues vs. warnings
- Suggest investigation steps

**GitHub Integration**:

- Creates issues with detailed context
- Links to similar historical issues
- Prevents duplicate issue creation (>70% similarity)
- Detects regressions (similar to recently closed issues)
- Adds comments to existing issues when duplicates detected

**SQLite Database**: Stores logs, GitHub issue metadata, and warnings for historical analysis

---

## Installation

### Prerequisites

- Rust 1.75+
- Redis 7+
- Ollama with `qwen-agent:latest` model
- GitHub Personal Access Token (repo scope or issues: read/write)

### Setup

```bash
# Clone repository
git clone https://github.com/yourusername/logsnuffer.git
cd logsnuffer

# Install Ollama model
ollama pull qwen-agent:latest

# Build project
cargo build --release
```

### Configuration

Create `.env` file:

```bash
GITHUB_TOKEN=ghp_your_github_token
GITHUB_OWNER=your-username
GITHUB_REPO=your-repo-name
```

---

## Usage

### Start Services

```bash
# Terminal 1: Start Redis
redis-server

# Terminal 2: Start Ollama
ollama serve

# Terminal 3: Start LogSnuffer server
export $(cat .env | xargs)
cargo run --release --bin server

# Terminal 4: (Optional) Start test log generator
cargo run --release --bin generator
```

### API Endpoints

**Ingest Logs**

```bash
POST /api/logs
Content-Type: application/json

{
  "logs": [
    {
      "service": "checkout",
      "message": "db_connection_timeout",
      "level": "ERROR",
      "instance": "pod-1",
      "timestamp": 1738234567
    }
  ]
}
```

**Get Confidence Score**

```bash
GET /api/confidence

{
  "confidence": 0.785,
  "message": "Current confidence: 0.785"
}
```

**GitHub Webhook**

```bash
POST /webhooks/github
# Automatically receives GitHub issue state changes
```

---

## How It Works

### 1. Log Ingestion

Logs are sent to `/api/logs` and:

- Stored in Redis buckets (1 bucket per second)
- Written to SQLite asynchronously
- Aggregated by error pattern and pod

### 2. Confidence Computation

Every second, the system:

- Rotates to a new time bucket
- Compares recent error rates (last 3 seconds) to baseline (last 27 seconds)
- Computes a confidence score (0.0 to 1.0) based on:
  - Error rate spike ratio
  - Dominant message pattern frequency
  - Number of affected pods

### 3. Agent Trigger

When confidence exceeds threshold (configurable via planner):

- Agent fetches last 15 error patterns with occurrence counts
- Agent analyzes patterns using LLM
- Agent decides which errors need GitHub issues vs. warnings

### 4. Issue Creation

For each critical error:

- Checks for duplicate open issues (>70% similarity)
  - If found: Adds comment to existing issue instead
- Checks for recently closed issues (>60% similarity, <7 days)
  - If found: Marks new issue as regression
- Creates GitHub issue with:
  - Error description and suggested fix
  - Links to top 3 related closed issues
  - Automatic labels (severity, automated, regression)

### 5. Issue Tracking

- GitHub webhook updates database when issues are closed
- Closed issues become searchable for future similarity matching
- System learns from historical resolutions

---

## Confidence Scoring Algorithm

```rust
// Simplified pseudocode
let short_rate = errors_last_3s / logs_last_3s;
let long_rate = errors_last_27s / logs_last_27s;

let error_signal = (short_rate / long_rate) / 10.0;  // Spike detection
let dominant_msg_ratio = max_message_count / total_logs;
let pod_spread = unique_pods / total_pods;

let score = 
    (error_signal * 0.5) +       // 50% weight: error spike
    (dominant_msg_ratio * 0.4) + // 40% weight: message dominance
    (pod_spread * 0.1);          // 10% weight: pod distribution

// Apply exponential smoothing
confidence = (score * 0.8) + (prev_confidence * 0.2);
```

---

## Database Schema

**Logs Table**

```sql
CREATE TABLE logs (
    id INTEGER PRIMARY KEY,
    service TEXT,
    message TEXT,
    level TEXT,
    instance TEXT,
    timestamp INTEGER
);
```

**GitHub Issues Table**

```sql
CREATE TABLE github_issues (
    id INTEGER PRIMARY KEY,
    issue_number INTEGER UNIQUE,
    title TEXT,
    body TEXT,
    error_pattern TEXT,
    state TEXT,
    created_at INTEGER,
    closed_at INTEGER,
    related_issues TEXT  -- JSON array
);
```

**Warnings Table**

```sql
CREATE TABLE warnings (
    id INTEGER PRIMARY KEY,
    error_pattern TEXT,
    severity TEXT,
    description TEXT,
    first_seen INTEGER,
    last_seen INTEGER,
    occurrence_count INTEGER,
    status TEXT
);
```

---

## Duplicate Prevention

The system prevents duplicate issues through:

1. **Similarity Matching**: Calculates Jaccard similarity on error patterns and issue titles
2. **Open Issue Check**: If >70% similar to an open issue, adds a comment instead
3. **Regression Detection**: If >60% similar to issue closed within 7 days, creates new issue with regression label
4. **Bidirectional Linking**: References top 3 closed issues and adds backlink comments

---

## Future Improvements

### High Priority

- **PostgreSQL Support**: Replace SQLite for better concurrency handling
- **Configurable Thresholds**: Move hardcoded values (similarity thresholds, time windows) to configuration file
- **Rate Limiting**: Prevent API abuse and LLM overuse
- **Prometheus Metrics**: Export confidence scores and error rates for monitoring

### Medium Priority

- **Web Dashboard**: Real-time visualization of confidence trends and active incidents
- **Multi-Tenancy**: Support multiple teams/projects with isolated data
- **Advanced Similarity**: Use embeddings instead of keyword-based matching
- **Custom Alert Rules**: User-defined conditions for issue creation

### Low Priority

- **Slack/Discord Notifications**: Alert channels when critical issues are created
- **Distributed Tracing Integration**: Link errors to traces automatically
- **Anomaly Detection**: ML-based detection beyond rule-based confidence
- **Issue Templates**: Customizable GitHub issue formats per error type

---

## Limitations

- **SQLite Concurrency**: High log volumes may cause database locking; consider PostgreSQL
- **LLM Latency**: Agent analysis takes 5-30 seconds; not suitable for sub-second SLAs
- **Memory Usage**: Redis stores 30 seconds of logs in memory; adjust window size for high-volume systems
- **Single Instance**: Current design assumes one server instance; scaling requires Redis coordination

---
