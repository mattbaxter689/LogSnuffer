# LogSnuffer

**Intelligent Log Analysis and Incident Management System**

LogSnuffer is an autonomous log monitoring system that uses AI to analyze error patterns, assess severity, and automatically create GitHub issues with historical context. It combines real-time log aggregation with LLM-powered analysis to reduce manual incident response time.

Will be adding / swapping some of the additional pieces for additional llm tool calls, along with an llm managed state to pass through the lifecycle

### Checklist

- [ ] add /metrics endpoint for prometheus
- [ ] add in logging

---

## Personal Note

As noted toward the end of this file, there are examples of how the project can be updated/upgraded. I want to highlight that I want to implement the prometheus metrics, as well as the rate limiting. These are something that are critical regardless of the system, and I think that they are very important to add to systems like this, which will have a very high throughput.

This project started as a simple example, but quickly grew into something that I wanted to iterate on and improve. While this is something that could be used for production systems, there are obviously costs that are associated
with something like this, especially when calling the LLM every time there are issues. Now, this is also the result of me creating a log generator that pushes error like crazy, but still. There should also be protections in
place to reduce the potential cost of LLM calls, perhaps LLM call limits in a time frame, batching of logs rather than the time binning methodology, etc. If this were a true production agent,
these would have to be considered to reduce the potential ballooning cost of the application.

Finally, is this project rough around the edges? I think the answer is yes. Obviously there are some things that I thing should change in terms of readability, or just development overall,
but this project also became much larger than I had originally anticipated. I think for a first go, this is something to be proud of

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

**LLM Agent**: Uses Gemini (Gemini Pro 2.5)

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
- Gemini API key available. Started with Qwen2.5:1b, but small models are just not enough
for what I wanted to do
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
REDIS_URL=your-redis-url
GEMINI_API_KEY=your-api-key
```

Depending on the redis instance used, you will need to configure the URL to make sure you are able to access the instance. Additionally, this project makes use of Helm charts. You will need to create a `secrets.yaml` or something similar, that contains the same environment variables found in the `.env` file.

---

## Usage

### Start Services

```bash
# with a docker deployment
just service

# with helm deployment
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
- Aggregated by error pattern

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
- Agent optionally decides is it wants a longer error or log pattern
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

---

## Limitations

- **SQLite Concurrency**: High log volumes may cause database locking; consider PostgreSQL
- **LLM Latency**: Agent analysis takes 5-30 seconds; not suitable for sub-second SLAs
- **Memory Usage**: Redis stores 30 seconds of logs in memory; adjust window size for high-volume systems
- **Single Instance**: Current design assumes one server instance; scaling requires Redis coordination
