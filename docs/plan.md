# Implementation Plan

## Phase 1: Project Scaffolding

### 1.1 Initialize Cargo Project
- Create `Cargo.toml` with all dependencies
- Create `rust-toolchain.toml` pinning `stable`
- Create `.gitignore` (Rust standard)

### 1.2 Create Base Config
- Create `src/config.rs` with `Config`, `AuthConfig`, `RetryConfig`, `CalendarSource` structs
- YAML deserialization via `serde_yaml`
- Config file path from `CALENDAR_PROXY_CONFIG` env var, default `config.yaml`
- Auth mode validation: exactly one of (query token, header token, basic auth, none)
- Example `config.yaml`

## Phase 2: ICS Core (src/calendar.rs)

### 2.1 Domain Types
- `SanitizedEvent` — holds only the fields we preserve (UID, DTSTART, DTEND/DURATION, RRULE, EXDATE, RDATE, RECURRENCE-ID, TRANSP, STATUS)
- `SanitizedCalendar` — holds VTIMEZONE blocks + Vec<SanitizedEvent>

### 2.2 ICS Parser
- Parse source ICS with the `ical` crate
- Extract VEVENT components into `SanitizedEvent`
- Preserve VTIMEZONE components verbatim
- Skip CANCELLED events

### 2.3 ICS Generator
- Serialize `SanitizedCalendar` back to ICS string
- Output proper VCALENDAR wrapper with `PRODID:-//Calendar Proxy//EN`
- Emit property parameters correctly (TZID, VALUE=DATE, etc.)
- Set SUMMARY to `Busy`

### 2.4 Whitelist Sanitization
- Copy only: UID, DTSTART, DTEND, DURATION, RRULE, EXDATE, RDATE, RECURRENCE-ID, TRANSP, STATUS
- Strip everything else (DESCRIPTION, LOCATION, ORGANIZER, ATTENDEE, X-*, etc.)
- Override SUMMARY to `Busy`

### 2.5 Merge & Dedup
- Merge calendars by collecting all events
- Dedup by UID (first seen wins, debug log on skip)

## Phase 3: Fetcher (src/calendar.rs)

### 3.1 ICS Fetcher
- Fetch ICS from URL via `reqwest`
- Timeout handling
- Non-200 response handling

### 3.2 Retry Logic
- Configurable retry count + exponential backoff
- Initial backoff from config, doubles each attempt
- Log each retry attempt and final failure

## Phase 4: Cache (src/cache.rs)

### 4.1 Cache Manager
- Struct wrapping cache directory path
- `write_atomic(content)` — write to `.tmp` file, `rename()` to final path
- `read()` — read cached file contents

### 4.2 Background Refresh
- `tokio::spawn` background task
- On each tick: fetch all calendars → parse → sanitize → merge → dedup → write atomically
- Sleep for `refresh_interval_secs` between ticks
- On all-fetch-failure: log error, keep old cache

## Phase 5: Authentication (src/auth.rs)

### 5.1 Auth Middleware
- Axum middleware layer
- Three mutually exclusive modes, determined from config at startup

#### 5.1.1 Query Token Mode
- Extract `token` query parameter
- Compare to configured token
- 401 on mismatch/missing

#### 5.1.2 Custom Header Mode
- Extract configured header name (e.g., `X-Calendar-Token`)
- Compare value to configured token
- 401 on mismatch/missing

#### 5.1.3 Basic Auth Mode
- Parse `Authorization: Basic <base64>` header
- Decode base64, split at `:`
- Compare username/password
- 401 on mismatch/missing

### 5.2 Config Validation
- On startup, validate exactly one auth mode is active
- If >1 mode populated, panic with clear error message

## Phase 6: Server (src/main.rs)

### 6.1 Main Entry Point
- Init `tracing-subscriber` with env-filter
- Load and validate config
- Create cache directory
- Initial fetch + write (block until ready; exit if all calendars fail)
- Spawn background refresh task
- Build axum Router with auth middleware

### 6.2 Routes
- `GET /calendar.ics` — read cache file, return with `Content-Type: text/calendar`
- `GET /health` — return `200 OK` (no auth)

### 6.3 Graceful Shutdown
- Handle SIGTERM (via `tokio::signal::unix`) + SIGINT (via `ctrl_c`)
- Axum `with_graceful_shutdown()`

## Phase 7: Testing

### 7.1 Unit Tests
- `config.rs`: valid YAML, missing file, bad auth combos
- `calendar.rs`: parse fixture → verify sanitized output; all-day events; VTIMEZONE passthrough; RRULE preservation; CANCELLED filtering; X-* stripping; TRANSP passthrough; UID dedup
- `auth.rs`: all 3 modes valid/invalid/missing; mutual exclusivity validation; no-auth passthrough
- `cache.rs`: atomic rename correctness, temp file cleanup

### 7.2 Integration Tests
- Server with mock auth: request with/without credentials
- `/health` returns 200
- `/calendar.ics` returns correct content type

### 7.3 Test Fixtures
- ICS files with various edge cases (all-day, recurring, cancelled, various TZID)

## Phase 8: CI/CD (`.github/workflows/`)

### 8.1 CI Workflow (ci.yml)
- Trigger: pull_request to main, push to main
- Steps: checkout, Rust toolchain, `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
- Permissions: `contents: read`

### 8.2 Release Workflow (release.yml)
- Trigger: push to main
- Steps: checkout, Rust toolchain, compute version, `cargo build --release`
- Install `pack` via `buildpacks/github-actions/setup-pack@v5`
- Build + publish OCI image to GHCR
- Create/update draft release, upload binary
- Permissions: `contents: write`, `packages: write`
- Concurrency group to serialize releases

## Phase 9: Documentation

### 9.1 Documentation Files
- `docs/plan.md` — this file
- `docs/spec.md` — application specification
- `docs/architecture.md` — architecture & design decisions
- `README.md` — user-facing getting started guide
- `AGENTS.md` — brief reference for LLM agents
