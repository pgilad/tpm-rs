#!/usr/bin/env sh

set -eu

REPO="${TPM_INSTALL_REPO:-pgilad/tpm-rs}"
BASE_URL="${TPM_INSTALL_BASE_URL:-https://github.com/${REPO}/releases}"
INSTALL_VERSION="${TPM_INSTALL_VERSION:-${TPM_INSTALL_RELEASE_DEFAULT_VERSION:-latest}}"
INSTALL_DIR="${TPM_INSTALL_DIR:-${HOME:-}/.local/bin}"
TARGET="${TPM_INSTALL_TARGET:-}"
SUPPORTED_TARGETS="x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu, x86_64-apple-darwin, aarch64-apple-darwin"

usage() {
  cat <<'EOF'
Usage:
  install.sh [--version <tag>] [--dir <path>] [--target <triple>]

Options:
  --version  Install a specific release tag instead of the latest release
  --dir      Install directory for the `tpm` binary (default: ~/.local/bin)
  --target   Override auto-detected target triple
  --help     Show this help text

Environment:
  TPM_INSTALL_VERSION   Same as --version
  TPM_INSTALL_DIR       Same as --dir
  TPM_INSTALL_TARGET    Same as --target
  TPM_INSTALL_BASE_URL  Override the releases base URL
  TPM_INSTALL_REPO      Override the GitHub repo owner/name
EOF
}

if [ -t 1 ] && [ "${NO_COLOR:-}" = "" ] && [ "${TERM:-}" != "dumb" ]; then
  ESC="$(printf '\033')"
  BOLD="${ESC}[1m"
  GREEN="${ESC}[32m"
  YELLOW="${ESC}[33m"
  RED="${ESC}[31m"
  CYAN="${ESC}[36m"
  RESET="${ESC}[0m"
else
  BOLD=""
  GREEN=""
  YELLOW=""
  RED=""
  CYAN=""
  RESET=""
fi

say() {
  printf '%s\n' "$*"
}

detail() {
  printf '  %s\n' "$*"
}

section() {
  printf '%s %s\n' "${CYAN}==>${RESET}" "$*"
}

success() {
  printf '%s %s\n' "${GREEN}installed:${RESET}" "$*"
}

warn() {
  printf '%s %s\n' "${YELLOW}warning:${RESET}" "$*" >&2
}

fail() {
  printf '%s %s\n' "${RED}error:${RESET}" "$*" >&2
  exit 1
}

fail_unsupported_platform() {
  details="$1"
  fail "${details}. Supported release targets: ${SUPPORTED_TARGETS}"
}

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail "missing required command: $1"
  fi
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

download() {
  url="$1"
  destination="$2"

  if have_cmd curl; then
    if curl -fsSL "$url" -o "$destination"; then
      return
    fi
    fail "failed to download ${url} with curl"
  fi

  if have_cmd wget; then
    if wget -qO "$destination" "$url"; then
      return
    fi
    fail "failed to download ${url} with wget"
  fi

  fail "missing required downloader: curl or wget"
}

need_checksum_tool() {
  if have_cmd shasum || have_cmd sha256sum; then
    return
  fi

  fail "missing required checksum tool: shasum or sha256sum"
}

checksum_file() {
  file_path="$1"

  if have_cmd shasum; then
    shasum -a 256 "$file_path" | awk '{print $1}'
    return
  fi

  if have_cmd sha256sum; then
    sha256sum "$file_path" | awk '{print $1}'
    return
  fi

  fail "missing required checksum tool: shasum or sha256sum"
}

verify_checksum() {
  archive_path="$1"
  checksum_url="$2"
  checksum_path="$3"

  checksum="$(checksum_file "$archive_path")"

  download "$checksum_url" "$checksum_path"

  expected_checksum="$(tr -d '[:space:]' < "$checksum_path")"
  if [ -z "$expected_checksum" ]; then
    fail "downloaded checksum file from ${checksum_url} was empty"
  fi

  if [ "$checksum" != "$expected_checksum" ]; then
    fail "checksum mismatch for downloaded archive"
  fi
}

detect_os_suffix() {
  case "$(uname -s)" in
    Linux)
      case "$(detect_linux_libc)" in
        musl)
          fail_unsupported_platform "musl-based Linux is not supported by the published release assets; install from source or use a glibc-based environment"
          ;;
        *)
          ;;
      esac
      printf '%s\n' 'unknown-linux-gnu'
      ;;
    Darwin)
      printf '%s\n' 'apple-darwin'
      ;;
    *)
      fail_unsupported_platform "unsupported operating system: $(uname -s)"
      ;;
  esac
}

detect_linux_libc() {
  if have_cmd ldd; then
    ldd_output="$(ldd --version 2>&1 || true)"
    case "$ldd_output" in
      *musl*)
        printf '%s\n' 'musl'
        return
        ;;
      *GNU\ libc*|*GLIBC*|*glibc*)
        printf '%s\n' 'gnu'
        return
        ;;
    esac

    ldd_output="$(ldd /bin/sh 2>&1 || true)"
    case "$ldd_output" in
      *musl*)
        printf '%s\n' 'musl'
        return
        ;;
      *GNU\ libc*|*GLIBC*|*glibc*)
        printf '%s\n' 'gnu'
        return
        ;;
    esac
  fi

  if have_cmd getconf && getconf GNU_LIBC_VERSION >/dev/null 2>&1; then
    printf '%s\n' 'gnu'
    return
  fi

  printf '%s\n' 'unknown'
}

is_rosetta_translated() {
  [ "$(uname -s)" = 'Darwin' ] || return 1
  have_cmd sysctl || return 1
  [ "$(sysctl -in sysctl.proc_translated 2>/dev/null || true)" = '1' ] || return 1
  [ "$(sysctl -in hw.optional.arm64 2>/dev/null || true)" = '1' ]
}

detect_arch_prefix() {
  case "$(uname -m)" in
    x86_64|amd64)
      if is_rosetta_translated; then
        printf '%s\n' 'aarch64'
        return
      fi
      printf '%s\n' 'x86_64'
      ;;
    aarch64|arm64)
      printf '%s\n' 'aarch64'
      ;;
    *)
      fail_unsupported_platform "unsupported architecture: $(uname -m)"
      ;;
  esac
}

detect_target() {
  printf '%s-%s\n' "$(detect_arch_prefix)" "$(detect_os_suffix)"
}

release_download_prefix() {
  if [ "$INSTALL_VERSION" = "latest" ]; then
    printf '%s\n' "${BASE_URL}/latest/download"
    return
  fi

  printf '%s\n' "${BASE_URL}/download/${INSTALL_VERSION}"
}

is_dir_on_path() {
  case ":${PATH:-}:" in
    *:"$1":*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

detect_shell_name() {
  shell_name="${SHELL:-}"
  shell_name="${shell_name##*/}"

  case "$shell_name" in
    bash|zsh|fish)
      printf '%s\n' "$shell_name"
      ;;
    *)
      printf '%s\n' 'sh'
      ;;
  esac
}

shell_rc_file() {
  case "$1" in
    bash)
      printf '%s\n' '~/.bashrc'
      ;;
    zsh)
      printf '%s\n' '~/.zshrc'
      ;;
    fish)
      printf '%s\n' '~/.config/fish/config.fish'
      ;;
    *)
      printf '%s\n' '~/.profile'
      ;;
  esac
}

print_install_summary() {
  say "${BOLD}tpm installer${RESET}"
  detail "version: ${INSTALL_VERSION}"
  detail "target: ${TARGET}"
  detail "install: ${install_path}"
  say ""
}

post_install_command() {
  if is_dir_on_path "$INSTALL_DIR"; then
    printf '%s\n' 'tpm --version'
    return
  fi

  printf '%s\n' "${install_path} --version"
}

print_path_hint() {
  shell_name="$(detect_shell_name)"
  rc_file="$(shell_rc_file "$shell_name")"

  say "Add this to ${rc_file}:"
  case "$shell_name" in
    fish)
      say "  fish_add_path ${INSTALL_DIR}"
      ;;
    *)
      say "  export PATH=\"${INSTALL_DIR}:\$PATH\""
      ;;
  esac
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || fail "--version requires a release tag"
      INSTALL_VERSION="$2"
      shift 2
      ;;
    --dir)
      [ "$#" -ge 2 ] || fail "--dir requires a path"
      INSTALL_DIR="$2"
      shift 2
      ;;
    --target)
      [ "$#" -ge 2 ] || fail "--target requires a target triple"
      TARGET="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      fail "unknown option: $1"
      ;;
  esac
done

[ -n "${HOME:-}" ] || fail "HOME must be set"
[ -n "$INSTALL_DIR" ] || fail "install directory could not be determined"

need_cmd awk
need_cmd chmod
need_cmd cp
need_cmd mkdir
need_cmd mktemp
need_cmd mv
need_cmd rm
need_cmd tar
need_cmd tr
need_cmd uname
need_checksum_tool

if [ -z "$TARGET" ]; then
  TARGET="$(detect_target)"
fi

download_prefix="$(release_download_prefix)"
archive_name="tpm-${TARGET}.tar.gz"
archive_url="${download_prefix}/${archive_name}"
checksum_url="${archive_url}.sha256"
install_path="${INSTALL_DIR}/tpm"

tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/tpm-install.XXXXXX")"
install_tmp_path=""
cleanup() {
  if [ -n "$install_tmp_path" ] && [ -e "$install_tmp_path" ]; then
    rm -f "$install_tmp_path"
  fi
  rm -rf "$tmpdir"
}
trap cleanup EXIT INT TERM HUP

archive_path="${tmpdir}/${archive_name}"
checksum_path="${archive_path}.sha256"

print_install_summary

section "Downloading tpm for ${TARGET}"
detail "from: ${archive_url}"
download "$archive_url" "$archive_path"
section "Verifying checksum"
verify_checksum "$archive_path" "$checksum_url" "$checksum_path"

section "Installing tpm"
tar -xzf "$archive_path" -C "$tmpdir"

binary_path="${tmpdir}/tpm-${TARGET}/tpm"
[ -f "$binary_path" ] || fail "downloaded archive did not contain the expected tpm-${TARGET}/tpm path"

mkdir -p "$INSTALL_DIR"
install_tmp_path="$(mktemp "${INSTALL_DIR}/.tpm.tmp.XXXXXX")"
cp "$binary_path" "$install_tmp_path"
chmod 755 "$install_tmp_path"
mv -f "$install_tmp_path" "$install_path"
install_tmp_path=""

success "${install_path}"
detail "Run: $(post_install_command)"

if ! is_dir_on_path "$INSTALL_DIR"; then
  say ""
  warn "${INSTALL_DIR} is not on PATH"
  print_path_hint
fi
