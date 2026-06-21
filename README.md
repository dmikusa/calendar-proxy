# Calendar Proxy

A lightweight HTTP service that aggregates multiple iCalendar (ICS) feeds from iCloud,
Google Calendar, or any service offering a private ICS feed, anonymizes event details,
and serves a combined sanitized feed showing only busy/free blocks.

## Quick Start

### 1. Get private ICS feed URLs

- **iCloud**: Calendar → Share Calendar → Public Calendar → copy link
- **Google Calendar**: Settings → Integrate Calendar → Private/Secret in iCal Format → copy link

A couple quick notes about these options:

1. For iCloud, this makes your calendar public, but it's still only visible with the URL. From what I can tell, it being "public" in this case does not mean someone can search or lookup your calendar in any way. It's public in the sense that if you have the URL, there's no authentication on it.

2. For Google, the terminology is different. They call the URL a "private" or "secret" URL, but it's not protected any more or less than the iCloud URL. There is no auth requirement and anyone with the URL can view what's in it. This is compared to Google's concept of a public calendar which is searchable and visable to people without the link. Do *NOT* go that route, use the "private" or "secret" URL unless your calendar is really meant to be visible to the whole world.

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
