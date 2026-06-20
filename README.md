# Calendar Proxy

A lightweight HTTP service that aggregates multiple iCalendar (ICS) feeds from iCloud,
Google Calendar, or any service offering a private ICS feed, anonymizes event details,
and serves a combined sanitized feed showing only busy/free blocks.

## Quick Start

### 1. Get private ICS feed URLs

- **iCloud**: Calendar → Share Calendar → Public Calendar → copy link
- **Google Calendar**: Settings → Integrate Calendar → Private address → ICS

### 2. Create `config.yaml`

```yaml
port: 8080
cache_dir: "/tmp/calendar-proxy-cache"
refresh_interval_secs: 300
retry:
  count: 3
  backoff_secs: 5
calendars:
  - url: "https://pXX-caldav.icloud.com/.../calendar.ics"
  - url: "https://calendar.google.com/calendar/ical/.../basic.ics"
```

### 3. Build and run with Docker

```bash
pack build calendar-proxy \
  --buildpak paketo-community/rust \
  --builder paketobuildpacks/ubuntu-noble-builder \
  --run-image paketobuildpacks/ubuntu-noble-run-static

docker run -p 8080:8080 \
  -v $(pwd)/config.yaml:/workspace/config.yaml \
  calendar-proxy
```

### 4. Subscribe to the feed

Add `http://localhost:8080/calendar.ics` to your calendar client.

## Auth

Calendar Proxy supports three mutually exclusive auth modes:

| Mode | Config | Example Request |
|---|---|---|
| None | `auth: {}` | `GET /calendar.ics` |
| Query token | `auth.token: "secret"` | `GET /calendar.ics?token=secret` |
| Header token | `auth.token: "secret"`, `auth.token_header: "X-Cal-Token"` | Header `X-Cal-Token: secret` |
| Basic auth | `auth.username: "user"`, `auth.password: "pass"` | Standard `Authorization: Basic ...` |

See [docs/spec.md](docs/spec.md) for full config reference.

## Documentation

| Document | Description |
|---|---|
| [docs/spec.md](docs/spec.md) | Complete application specification |
| [docs/architecture.md](docs/architecture.md) | Architecture and design decisions |
| [docs/plan.md](docs/plan.md) | Implementation plan and task list |

## Endpoints

| Path | Auth | Description |
|---|---|---|
| `GET /calendar.ics` | Configurable | Combined sanitized ICS feed |
| `GET /health` | None | Health check (no auth required) |

## Development

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```
