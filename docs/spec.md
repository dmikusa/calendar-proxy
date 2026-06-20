# Calendar Proxy — Specification

## 1. Overview

Calendar Proxy is a lightweight HTTP service that aggregates multiple iCalendar (ICS) feeds,
anonymizes them by stripping event details, and serves a combined sanitized ICS feed. It acts
as a privacy layer between your personal calendars and any consumer that only needs to know
when you are busy.

### 1.1 Purpose

- Aggregate calendars from iCloud, Google Calendar, or any service offering a private ICS feed
- Strip all event details (title, description, location, attendees, etc.)
- Output a combined ICS feed showing only busy/free blocks
- Run as a containerized service with minimal dependencies

### 1.2 Non-Goals

- Not a calendar server — does not accept writes or edits
- Not an auth proxy — source calendars must be accessible via URL (private/hidden feeds)
- Not a recurrence expander — RRULEs are passed through for client-side expansion
- Not a high-frequency real-time service (refreshes on a configurable interval)

## 2. Configuration

### 2.1 Config File

Path: configurable via `CALENDAR_PROXY_CONFIG` env var, defaults to `config.yaml` in the working directory.

```yaml
port: 8080
cache_dir: "/tmp/calendar-proxy-cache"
refresh_interval_secs: 300
retry:
  count: 3
  backoff_secs: 5
auth:
  token: "my-secret"
  token_header: ""
  username: ""
  password: ""
calendars:
  - url: "https://pXX-caldav.icloud.com/.../calendar.ics"
  - url: "https://calendar.google.com/calendar/ical/.../basic.ics"
```

| Field | Type | Default | Description |
|---|---|---|---|
| `port` | integer | 8080 | HTTP listen port |
| `cache_dir` | string | (required) | Directory for the cached ICS file |
| `refresh_interval_secs` | integer | 300 | Seconds between cache refreshes |
| `retry.count` | integer | 3 | Max fetch retries per calendar |
| `retry.backoff_secs` | integer | 5 | Initial retry delay (doubles each attempt) |
| `auth` | object | — | Auth configuration (see below) |
| `calendars` | array | (required) | List of source calendar URLs |

### 2.2 Auth Modes

Exactly one auth mode may be active at a time. If more than one is populated, the service
exits with an error on startup. If none is populated, the feed is served without auth.

**Mode 1 — Query Parameter Token:**
```yaml
auth:
  token: "my-secret"
  token_header: ""     # empty means query param mode
```
Request: `GET /calendar.ics?token=my-secret`

**Mode 2 — Custom Header Token:**
```yaml
auth:
  token: "my-secret"
  token_header: "X-Calendar-Token"
```
Request: `GET /calendar.ics` with header `X-Calendar-Token: my-secret`

**Mode 3 — Basic Auth:**
```yaml
auth:
  username: "alice"
  password: "hunter2"
```
Request: `GET /calendar.ics` with `Authorization: Basic ...`

### 2.3 Unauthenticated Mode
```yaml
auth: {}
```
Request: `GET /calendar.ics` — no credentials required.

## 3. HTTP Endpoints

### 3.1 `GET /calendar.ics`

Returns the combined, sanitized ICS feed.

- **Content-Type:** `text/calendar; charset=utf-8`
- **Auth:** Depends on configuration (see §2.2)
- **Cache:** Response is the current on-disk cache file; no `Cache-Control` header set
- **Errors:**
  - 401 Unauthorized — missing or invalid credentials
  - 503 Service Unavailable — cache not yet populated (should not occur after startup)

### 3.2 `GET /health`

Health check endpoint for container probes.

- **Content-Type:** `text/plain`
- **Auth:** None (always open)
- **Response:** `200 OK` — service is running and has a valid cache

## 4. ICS Processing

### 4.1 Fetching

- All source calendars are fetched in parallel on each refresh cycle
- Each fetch uses a 30-second timeout
- Failed fetches are retried with exponential backoff (initial delay × 2^attempt)
- On per-calendar failure: error is logged, that calendar is skipped
- On all-calendars failure: old cache is preserved, error is logged

### 4.2 Parsing

- Each ICS is parsed using the `ical` crate
- Malformed ICS files are skipped with a logged error
- The following components are extracted:
  - VTIMEZONE blocks — preserved verbatim
  - VEVENT blocks — sanitized (see §4.3)

### 4.3 Sanitization (Whitelist)

Only the following VEVENT properties are retained. Everything else is stripped.

**Retained (copied verbatim):**
| Property | Reason |
|---|---|
| `UID` | Event identity, deduplication |
| `DTSTART` | Start time |
| `DTEND` | End time |
| `DURATION` | Alternative to DTEND |
| `RRULE` | Recurrence rule (passed through) |
| `EXDATE` | Exception dates |
| `RDATE` | Recurrence dates |
| `RECURRENCE-ID` | Recurrence instance identification |
| `TRANSP` | Transparency (OPAQUE/TRANSPARENT) — passed through as-is |
| `STATUS` | Event status — used to filter CANCELLED |

**Modified:**
| Property | Change |
|---|---|
| `SUMMARY` | Always set to `Busy` |

**Stripped (complete list):**
| Property | Why |
|---|---|
| `DESCRIPTION` | Leaks event details |
| `LOCATION` | Leaks physical location |
| `GEO` | Leaks GPS coordinates |
| `URL` | Leaks event link |
| `ATTACH` | Leaks attached files |
| `ORGANIZER` | Leaks email address |
| `ATTENDEE` | Leaks email addresses |
| `CONTACT` | Leaks contact info |
| `CONFERENCE` | Leaks video call link |
| `X-*` (all custom) | Unknown content, catch-all risk |
| `CATEGORIES` | Leaks habits/patterns |
| `CLASS` | Leaks sensitivity classification |
| `CREATED` / `DTSTAMP` / `LAST-MODIFIED` | Leaks timing metadata |
| `SEQUENCE` | Leaks revision history |
| `COMMENT` | Leaks free-text notes |
| `VALARM` | Leaks alarm patterns |
| `COLOR` / `X-APPLE-CALENDAR-COLOR` | Leaks calendar organization |
| `X-APPLE-STRUCTURED-LOCATION` | Leaks precise Apple location |
| Any unknown property | Stripped by default |

### 4.4 Merging & Deduplication

- All sanitized events from all calendars are collected into a single list
- Events are deduplicated by `UID` — first occurrence is kept, subsequent duplicates are
  skipped with a debug-level log message
- VTIMEZONE blocks from all calendars are collected (duplicate TZIDs are resolved by
  keeping the first occurrence)

### 4.5 Output Generation

The output is a valid iCalendar (RFC 5545) document:

```
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Calendar Proxy//EN
CALSCALE:GREGORIAN
METHOD:PUBLISH
BEGIN:VTIMEZONE
TZID:America/New_York
...
END:VTIMEZONE
BEGIN:VEVENT
UID:abc@calendar-proxy
DTSTART;TZID=America/New_York:20240601T090000
DTEND;TZID=America/New_York:20240601T100000
SUMMARY:Busy
TRANSP:OPAQUE
END:VEVENT
...
END:VCALENDAR
```

## 5. Caching

### 5.1 Disk-Based Cache

The merged ICS output is stored on disk rather than in memory.

**Write path:**
1. Generate content to `{cache_dir}/calendar.ics.tmp`
2. `rename("{cache_dir}/calendar.ics.tmp", "{cache_dir}/calendar.ics")`
3. The `rename()` syscall is atomic on the same filesystem (POSIX guarantee)

**Read path:**
1. Every request reads `{cache_dir}/calendar.ics` from disk
2. Atomic rename guarantees readers always see a complete file

### 5.2 Startup Gate

- On startup, the initial fetch must complete successfully (at least one calendar) before
  the HTTP server starts listening
- If all source calendars fail on initial fetch, the service exits with code 1
- This ensures the `/health` endpoint only returns 200 when a valid cache exists

### 5.3 Refresh Cycle

- A background task runs every `refresh_interval_secs`
- Each cycle: fetch all → parse → sanitize → merge → dedup → write atomically
- If all calendars fail during a refresh, the old cache is preserved and an error is logged
- If some calendars succeed and some fail, the new cache includes only the successful ones

## 6. Error Handling

| Scenario | Behavior |
|---|---|
| All calendars unreachable on startup | Exit with code 1 |
| Some calendars unreachable on startup | Log warning, serve with available calendars |
| All calendars unreachable on refresh | Log error, keep old cache |
| Malformed ICS from source | Log error, skip that calendar |
| Invalid config | Exit with clear error message |
| Multiple auth modes configured | Exit with clear error message |
| Cache directory unwritable | Exit with error |

## 7. Startup Sequence

1. Load config from file (or env var path)
2. Validate auth mode exclusivity
3. Create cache directory (`mkdir -p`)
4. Initial fetch + merge + write (blocking)
5. If all calendars failed → exit
6. Start HTTP server
7. Spawn background refresh task
8. Wait for shutdown signal (SIGTERM/SIGINT)

## 8. Shutdown Sequence

1. Receive SIGTERM or SIGINT
2. Signal axum to stop accepting new connections
3. Wait for in-flight requests to complete (with timeout)
4. Exit cleanly (code 0)
