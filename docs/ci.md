# Calendar Proxy — CI/CD

All automation lives in `.github/workflows/`. Five workflows handle checking the
code, building and publishing a container image, turning a merged version into a
published release, and keeping dependencies current.

## Workflows at a glance

| Workflow | File | Runs on | What it does |
|---|---|---|---|
| CI | `ci.yml` | PR targeting `main`, push to `main` | Format check, clippy, tests, plus a zizmor scan of the workflow files |
| Build and Release | `release.yml` | Push to `main` | Builds a snapshot OCI image, extracts the binary, and creates or updates a draft release. Skipped when the version is already published |
| Tag release image | `tag-release.yml` | A release is published | Retags the snapshot image as the release version and `latest` |
| Bump version | `bump-version.yml` | Manual (`workflow_dispatch`) | Bumps `Cargo.toml`, opens a PR, enables auto-merge |
| Renovate | `renovate.yml` | Daily at 04:00 UTC, or manual | Runs self-hosted Renovate to open dependency PRs |

## CI (`ci.yml`)

Two jobs, both on `ubuntu-latest`.

The `test` job installs the stable Rust toolchain and runs `cargo fmt -- --check`,
`cargo clippy -- -D warnings`, and `cargo test`.

The `zizmor` job scans the workflow files for security problems and uploads the
results to code scanning, which is why it needs `security-events: write`. The list
of files it scans is written out by hand under `inputs`. A new workflow file only
gets scanned if it is added to that list.

Workflow-level permissions are `contents: read`; the zizmor job adds the extra
scope it needs.

## Dependency automation (`renovate.yml`)

Self-hosted Renovate runs daily at 04:00 UTC and on demand. It authenticates with
the `RENOVATE_TOKEN` secret and reads `renovate.json` for its rules: Cargo and
GitHub Actions managers, a 7-day minimum release age, Actions pinned to commit
digests, and the `dependencies` label on everything it opens.

Renovate pushes branches with its own token, so when a Renovate PR merges, the
push to `main` triggers `release.yml` the same way a hand-made merge would.
Grouped Rust dependency updates auto-merge; major Rust updates and Actions digest
updates wait for approval on the dashboard.

## Version bump (`bump-version.yml`)

Manual dispatch with a `version` input. It sets the version that the next release
will carry.

The version has to match `X.Y.Z` and has to be greater than the version currently
in `Cargo.toml`. Comparison is numeric on each part (`major * 1000000 + minor *
1000 + patch`), not a string compare. If it passes, the workflow rewrites
`Cargo.toml`, syncs the root package's entry in `Cargo.lock`, pushes a branch named
`bump-version-<version>`, opens a PR labeled `dependencies`, and enables squash
auto-merge on it.

The lockfile is synced with `cargo update -p calendar-proxy`, which rewrites the
one line that holds the root package's version. Not `cargo generate-lockfile`:
that re-resolves the whole graph rather than honouring the pins already in the
lock, so it drags every pending upgrade into the version bump commit. In practice
that was around a hundred package versions per bump, and it bypassed the
release-age policy in `renovate.json` while it did it. Dependency updates belong
to Renovate.

Merging that PR puts the new version on `main`, and that is what makes the next
build a release candidate. Nothing else in the repo sets the version, so a release
can't happen without this workflow or an equivalent hand edit. The commit is
authored as the PAT's owner, so the commit, the PR and the merge all carry the same
identity rather than mixing a bot commit with a human merge.

### When in the cycle this runs

It runs before the release it produces. The bump is the first step of cutting a
release: it decides the version, `release.yml` builds it and opens a draft, and
publishing that draft is what makes it a release. On its own the bump releases
nothing.

Seen across two releases, a bump sits between them. It comes after the previous
release is published and before the next one goes out. Running it as soon as the
previous release is published is the habit that avoids trouble: `main` always
names the next version, and the draft for that version picks up work as it lands.
Cutting a release then means reviewing the current draft and publishing it.

Running it late works too, as long as nothing else reaches `main` first. While
`main` still names a version that is already published, a push to `main` builds
nothing, because `release.yml` checks for a published release before it starts.
The push is silently a no-op for the release pipeline, which is easy to mistake
for a broken workflow. That is the first item in [Known rough edges](#known-rough-edges).

This job runs as a PAT, not as `GITHUB_TOKEN`. The reasoning is in [Why the
version bump uses a PAT](#why-the-version-bump-uses-a-pat).

## Build and release (`release.yml`)

Triggered by every push to `main`, which includes dependency merges and version
bumps. Four jobs run in order via `needs`.

The `check` job decides whether the push is worth building at all. It reads the
version out of `Cargo.toml` and asks whether `v<version>` is already published. If
it is, a published release is final, so the push builds nothing. Its only
permission is `contents: read`.

The `build` job is skipped when that answer is no, and `package` and `release`
follow it, because both `need` it. When it does run, it reads the base version out
of `Cargo.toml` and builds the image tag `<version>-<short-sha>-snapshot`. It logs
in to GHCR and runs `pack build --publish` against
`paketobuildpacks/ubuntu-noble-builder` with the `paketo-community/rust` buildpack
and the `ubuntu-noble-run-tiny` run image. `ghcr.io/<repo>:cache` holds the build
cache. Permissions are `contents: read` and `packages: write`.

The `package` job pulls the image that was just published, exports the container
filesystem, and copies `bin/calendar-proxy` out of it. That binary is gzipped to
`calendar-proxy-<version>.gz` and uploaded as a workflow artifact, so this job
never builds Rust a second time. Its permissions are `contents: read` and
`packages: read`.

The `release` job downloads the artifact and creates the draft release
`v<version>` with a placeholder body: the tag and the container image reference.
If `v<version>` already exists as a draft, it updates the notes and re-uploads the
asset with `--clobber`. If `v<version>` is already published, it does nothing at
all. That branch is rare now that `check` runs first, but it still happens if
someone publishes the draft while the build is running. Its permissions are
`contents: write` and `actions: read`.

A concurrency group named `release` queues runs rather than cancelling them. Two
pushes close together leave the first run to finish and hold the second until it
does, so the in-flight build is never interrupted. Interrupting one is not free:
a cancelled run has already published its snapshot image by the time it reaches
the later jobs, which leaves an image nothing points at, and the cancellation can
land between creating a draft release and uploading its asset. Only one run waits
at a time, so a burst of pushes collapses to the newest one.

## Tagging the release image (`tag-release.yml`)

Runs when a release is published. It parses the version from the tag name
(`v0.2.0` becomes `0.2.0`), resolves the short hash of the commit the tag points
at, and uses `crane tag` to point `<version>` and `latest` at the existing
`<version>-<short-hash>-snapshot` image. Nothing is rebuilt here, so the release
image is the same one that was built on `main`, just under a different tag.

## How a version becomes a release

```mermaid
graph TD
    BUMP["Bump version (manual dispatch)"] --> PR["PR: bump-version-X.Y.Z<br/>labeled dependencies, auto-merge on"]
    PR --> CI["CI on the PR: fmt, clippy, test, zizmor"]
    CI --> MERGE["Merge to main"]
    REN["Renovate PR merged"] --> MERGE
    MERGE --> REL["release.yml on push to main<br/>skipped if vX.Y.Z is already published<br/>build + publish snapshot image<br/>package the binary<br/>create or update draft release vX.Y.Z"]
    REL --> PUB["A human reviews and publishes the draft"]
    PUB --> TAG["tag-release.yml<br/>retag the snapshot image as X.Y.Z and latest"]
    TAG -.->|"bump the next version"| BUMP
```

The draft release is the gate. Everything up to it runs on its own, and nothing
becomes a release until someone publishes it.

## Why the version bump uses a PAT

The bump workflow needs to push a branch, open a PR, and enable auto-merge. Three
separate things get in the way of doing that with `GITHUB_TOKEN`:

1. `actions/checkout` runs with `persist-credentials: false`, so `git` has no
   credentials and `git push` fails with `could not read Username for
   'https://github.com'`. This is fixable on its own (running `gh auth setup-git`
   gives `git` the CLI as a credential helper), but doing that alone isn't enough.
2. GitHub refuses PR creation with `GITHUB_TOKEN` unless the repository setting
   "Allow GitHub Actions to create and approve pull requests" is on. The
   `pull-requests: write` scope does not override it. The setting is off here, and
   turning it on would let every workflow in the repo open PRs.
3. When `GITHUB_TOKEN` enables auto-merge, GitHub attributes the merge to
   `github-actions[bot]`, and pushes made with that token do not trigger
   workflows. The merge lands, `release.yml` never runs, and no release is
   produced.

The workflow uses the `RENOVATE_TOKEN` secret instead. A PAT isn't subject to the
repository setting, and a PAT-driven merge triggers `release.yml` normally. Because
the PAT carries the write permissions, the workflow itself only asks for
`contents: read`.

Checkout is deliberately not given the PAT. Passing it would make the PAT the
token that `actions/checkout` writes into its temporary credential file, and there
is no reason for that: the repo is public, so the job's read-only `GITHUB_TOKEN`
is enough to clone it. The PAT is only present in the environment of the two
steps that need it, and `gh auth setup-git` records the `gh` credential helper
rather than the token itself.

## Repository settings this depends on

| Setting | Required value | Where |
|---|---|---|
| `RENOVATE_TOKEN` secret | PAT with Contents write and Pull requests write | Repository secrets. Used by Renovate and by the version bump |
| Auto-merge | Enabled | Settings, General |
| `dependencies` label | Must exist | Issues, Labels. Both Renovate and the bump workflow apply it |
| Allow GitHub Actions to create and approve pull requests | Can stay off | Settings, Actions, General |
| Default workflow permissions | `read` is fine | Settings, Actions, General. Each workflow declares what it needs |
| Ruleset on `main` | Active | Repository rulesets. See [The ruleset on `main`](#the-ruleset-on-main) |

## The ruleset on `main`

A repository ruleset named `main` protects the default branch. The classic branch
protection API reports the branch as unprotected, which is a false negative: the
two mechanisms are separate, and rulesets are the newer one.

The ruleset is active and applies to `~DEFAULT_BRANCH`. It requires:

| Rule | Effect |
|---|---|
| `pull_request` | Changes reach `main` through a PR. Zero approving reviews are required |
| `required_status_checks` | `test` and `Run zizmor 🌈` must pass. Strict policy is on, so the PR branch must also be up to date with `main` |
| `required_signatures` | Commits on `main` must be signed |
| `deletion`, `non_fast_forward`, `creation` | `main` cannot be deleted or force-pushed |

The two required checks are job names from `ci.yml`, matched by their *display*
names, so renaming the `test` job or the zizmor job there would leave the ruleset
waiting on a check that never reports.

One bypass actor is configured: the repository admin role, with mode `always`.
Admin merges are not held to the rules above, and the effect shows up in the
history: unsigned dependency commits such as `21a69ae` and `4f1dc95` sit on `main`
today, carried there by signed merge commits.

So the ruleset is a real gate for anyone who is not an admin, and a guardrail
against accidents for anyone who is. It is not a guarantee that a particular merge
waited for CI.

Whether it gates the version bump depends on who owns `RENOVATE_TOKEN`, which
isn't recorded in the repo. If that account is a repository admin, its merge can
bypass the required checks. If it isn't, the checks have to pass for the merge to
go through.

Related, and worth knowing before narrowing that bypass: the bump workflow builds
its commit in CI, where there is no signing key, so the commit is unsigned. It
rides in on an admin merge today. Under `required_signatures` without the bypass,
it would need another route, such as creating the commit through the GitHub API,
which signs the commits it creates.

## Known rough edges

Publishing a release is treated as the end of a version, but nothing enforces
that. After `v0.1.1` is published, `Cargo.toml` still says `0.1.1`, so pushes to
`main` cannot produce a release again until the version moves. The `check` job in
`release.yml` sees the published release and skips the build, so the push is a
no-op: a green workflow, no new image, and nothing in the log that looks like a
failure. That is intended, since a published release is final, but it means a
fix cannot be shipped without bumping the version. See [When in the cycle this
runs](#when-in-the-cycle-this-runs).

The snapshot tag is computed in two places. `release.yml` uses `git rev-parse
--short HEAD`, which lets Git pick the abbreviation length, while `tag-release.yml`
uses `--short=7`. Both produce 7 characters at this repo's size, but a larger repo
would let them drift apart and the `crane tag` step would fail to find the image.

Auto-merge on the bump PR is not a hard CI gate, but for a different reason than
"nothing protects `main`". The ruleset does require the `test` and `Run zizmor 🌈`
checks; the admin bypass means the merge isn't held to them. See [The ruleset on
`main`](#the-ruleset-on-main).

## Running the same checks locally

```bash
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test
zizmor .github/workflows/
```
