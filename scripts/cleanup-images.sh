#!/usr/bin/env bash
set -euo pipefail

dry_run=false
repo=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --dry-run) dry_run=true; shift ;;
    --repo) repo="$2"; shift 2 ;;
    *) echo "Usage: $0 --repo <image-repo> [--dry-run]"; exit 1 ;;
  esac
done

if [ -z "$repo" ]; then
  echo "Error: --repo is required (e.g. ghcr.io/owner/repo)"
  exit 1
fi

echo "=== Cleanup: ${repo} ==="
if [ "$dry_run" = true ]; then
  echo "Mode: DRY RUN (no deletions)"
fi

echo "Fetching all tags from ${repo}..."
all_tags=$(crane ls "$repo" 2>/dev/null || true)

if [ -z "$all_tags" ]; then
  echo "No tags found or unable to list repository."
  exit 0
fi

# Collect digests protected by -release tags
declare -A protected_digests
release_tags=$(echo "$all_tags" | grep -E '^[0-9]+\.[0-9]+\.[0-9]+-release$' || true)
if [ -n "$release_tags" ]; then
  while IFS= read -r tag; do
    digest=$(crane digest "${repo}:${tag}" 2>/dev/null || true)
    if [ -n "$digest" ]; then
      protected_digests["${digest}"]=1
      echo "  Protected digest: ${digest} (from tag ${tag})"
    fi
  done <<< "$release_tags"
fi

# Find stale hash tags (X.Y.Z-<hash>) that are not release tags
hash_tags=$(echo "$all_tags" | grep -E '^[0-9]+\.[0-9]+\.[0-9]+-' | grep -v '\-release$' || true)
if [ -z "$hash_tags" ]; then
  echo "No stale hash tags found."
  exit 0
fi

while IFS= read -r tag; do
  digest=$(crane digest "${repo}:${tag}" 2>/dev/null || true)
  if [ -z "$digest" ]; then
    echo "  Skipping ${tag}: unable to resolve digest"
    continue
  fi

  if [ -n "${protected_digests[${digest}]-}" ]; then
    echo "  Keeping ${tag}: digest matches a release tag"
    continue
  fi

  # Check if any other tag shares this digest
  shared=false
  while IFS= read -r other; do
    if [ "$other" = "$tag" ]; then
      continue
    fi
    other_digest=$(crane digest "${repo}:${other}" 2>/dev/null || true)
    if [ "$other_digest" = "$digest" ]; then
      shared=true
      echo "  Digest ${digest} also tagged as ${other}"
      break
    fi
  done <<< "$all_tags"

  if [ "$shared" = true ]; then
    echo "  Untagging ${tag} (digest shared with other tags)"
    if [ "$dry_run" = false ]; then
      if ! crane delete "${repo}:${tag}" 2>&1; then
        echo "  WARN: failed to untag ${tag}"
      fi
    fi
  else
    echo "  Deleting ${tag} @ ${digest}"
    if [ "$dry_run" = false ]; then
      if ! crane delete "${repo}@${digest}" 2>&1; then
        echo "  WARN: digest delete failed, falling back to untag"
        if ! crane delete "${repo}:${tag}" 2>&1; then
          echo "  WARN: failed to untag ${tag}"
        fi
      fi
    fi
  fi
done <<< "$hash_tags"

echo "Done."
