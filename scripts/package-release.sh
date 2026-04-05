#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/package-release.sh <version> [output-dir] [--target <triple>]

Arguments:
  <version>     Release version used in archive file names
  [output-dir]  Directory for packaged artifacts (default: target/release/dist)
  --target      Rust target triple to build and package explicitly
EOF
}

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 1
fi

version=""
output_dir="target/release/dist"
target=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      if [[ $# -lt 2 ]]; then
        echo "error: --target requires a Rust target triple" >&2
        usage >&2
        exit 1
      fi
      target="$2"
      shift 2
      ;;
    -*)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
    *)
      if [[ -z "${version}" ]]; then
        version="$1"
      elif [[ "${output_dir}" == "target/release/dist" ]]; then
        output_dir="$1"
      else
        echo "error: unexpected argument: $1" >&2
        usage >&2
        exit 1
      fi
      shift
      ;;
  esac
done

if [[ -z "${version}" ]]; then
  usage >&2
  exit 1
fi

mkdir -p "${output_dir}"
output_dir="$(cd "${output_dir}" && pwd)"

if command -v shasum >/dev/null 2>&1; then
  checksum_command=(shasum -a 256)
elif command -v sha256sum >/dev/null 2>&1; then
  checksum_command=(sha256sum)
else
  echo "error: no SHA-256 checksum tool found (expected shasum or sha256sum)" >&2
  exit 1
fi

if [[ -n "${target}" ]]; then
  TPM_RELEASE_VERSION="${version}" cargo build --release --target "${target}"
  archive_target="${target}"
  binary_path="target/${target}/release/tpm"
else
  TPM_RELEASE_VERSION="${version}" cargo build --release

  archive_target="$(rustc -vV | sed -n 's/^host: //p')"
  if [[ -z "${archive_target}" ]]; then
    echo "error: could not determine rust host target" >&2
    exit 1
  fi

  binary_path="target/release/tpm"
fi

if [[ ! -f "${binary_path}" ]]; then
  echo "error: built binary not found at ${binary_path}" >&2
  exit 1
fi

staging_dir="$(mktemp -d "${TMPDIR:-/tmp}/tpm-rs-release.XXXXXX")"
trap 'rm -rf "${staging_dir}"' EXIT

archive_basename="tpm-${version}-${archive_target}"
archive_dir="${staging_dir}/${archive_basename}"
stable_archive_basename="tpm-${archive_target}"
stable_archive_dir="${staging_dir}/${stable_archive_basename}"

mkdir -p "${archive_dir}"
mkdir -p "${stable_archive_dir}"

for package_dir in "${archive_dir}" "${stable_archive_dir}"; do
  cp "${binary_path}" "${package_dir}/tpm"
  cp README.md "${package_dir}/README.md"
  cp CHANGELOG.md "${package_dir}/CHANGELOG.md"
  cp LICENSE "${package_dir}/LICENSE"
done

tarball_path="${output_dir}/${archive_basename}.tar.gz"
(
  cd "${staging_dir}"
  tar -czf "${tarball_path}" "${archive_basename}"
)

stable_tarball_path="${output_dir}/${stable_archive_basename}.tar.gz"
(
  cd "${staging_dir}"
  tar -czf "${stable_tarball_path}" "${stable_archive_basename}"
)

checksum="$("${checksum_command[@]}" "${tarball_path}" | awk '{print $1}')"
checksum_path="${tarball_path}.sha256"
printf '%s\n' "${checksum}" > "${checksum_path}"

stable_checksum="$("${checksum_command[@]}" "${stable_tarball_path}" | awk '{print $1}')"
stable_checksum_path="${stable_tarball_path}.sha256"
printf '%s\n' "${stable_checksum}" > "${stable_checksum_path}"

printf 'Packaged %s\n' "${tarball_path}"
printf 'Packaged %s\n' "${stable_tarball_path}"
printf 'SHA256: %s\n' "${checksum_path}"
printf 'SHA256: %s\n' "${stable_checksum_path}"
