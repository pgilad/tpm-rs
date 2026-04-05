#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/render-installer.sh <version> [output-path]

Arguments:
  <version>      Release tag embedded as the installer's default version
  [output-path]  Output path for the rendered installer (default: target/release/install.sh)
EOF
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage >&2
  exit 1
fi

version="$1"
output_path="${2:-target/release/install.sh}"
template_path="scripts/install.sh"

mkdir -p "$(dirname "${output_path}")"

printf -v release_default_line 'TPM_INSTALL_RELEASE_DEFAULT_VERSION=%q' "${version}"

awk -v release_default_line="${release_default_line}" '
  NR == 1 {
    print
    print release_default_line
    next
  }

  {
    print
  }
' "${template_path}" > "${output_path}"

chmod 755 "${output_path}"

printf 'Rendered %s with default version %s\n' "${output_path}" "${version}"
