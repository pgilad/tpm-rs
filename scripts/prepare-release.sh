#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/prepare-release.sh <build-number> [--dry-run]

Arguments:
  <build-number>  Numeric build identifier, typically GitHub's run number
  --dry-run       Generate release notes without modifying CHANGELOG.md
EOF
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage >&2
  exit 1
fi

build_number="$1"
dry_run=0

if [[ $# -eq 2 ]]; then
  if [[ "$2" != "--dry-run" ]]; then
    usage >&2
    exit 1
  fi
  dry_run=1
fi

release_date="$(date -u +%Y.%m.%d)"
release_version="${release_date}-${build_number}"
notes_dir="target/release"
notes_path="${notes_dir}/notes.md"

mkdir -p "${notes_dir}"

previous_tag="$(git tag --list '[0-9]*' --sort=-v:refname | sed -n '1p')"

if [[ -n "${previous_tag}" ]]; then
  revision_range="${previous_tag}..HEAD"
  changelog_entries="$(git log --no-merges --reverse --format='- %s (%h)' "${revision_range}" -- . ':(exclude)CHANGELOG.md')"
else
  changelog_entries="$(git log --no-merges --reverse --format='- %s (%h)' HEAD -- . ':(exclude)CHANGELOG.md')"
fi

if [[ -z "${changelog_entries}" ]]; then
  changelog_entries="- Maintenance release"
fi

{
  printf '## %s\n\n' "${release_version}"
  printf '%s\n' "${changelog_entries}"
} > "${notes_path}"

if [[ "${dry_run}" -eq 0 ]]; then
  existing_body=""
  if [[ -f CHANGELOG.md ]]; then
    first_line="$(sed -n '1p' CHANGELOG.md)"
    if [[ -n "${first_line}" && "${first_line}" != "# Changelog" ]]; then
      echo "error: expected CHANGELOG.md to start with '# Changelog'" >&2
      exit 1
    fi
    existing_body="$(tail -n +3 CHANGELOG.md)"
  fi

  {
    printf '# Changelog\n\n'
    cat "${notes_path}"
    if [[ -n "${existing_body}" ]]; then
      printf '\n%s\n' "${existing_body}"
    fi
  } > CHANGELOG.md
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    printf 'release_version=%s\n' "${release_version}"
    printf 'release_notes_path=%s\n' "${notes_path}"
    printf 'previous_tag=%s\n' "${previous_tag}"
  } >> "${GITHUB_OUTPUT}"
fi

printf 'Prepared release %s\n' "${release_version}"
printf 'Release notes: %s\n' "${notes_path}"
if [[ -n "${previous_tag}" ]]; then
  printf 'Previous tag: %s\n' "${previous_tag}"
else
  printf 'Previous tag: <none>\n'
fi
