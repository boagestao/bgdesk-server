#!/usr/bin/env bash
set -euo pipefail

REPO="${BGDESK_REPO:-boagestao/bgdesk-server}"
INSTALL_DIR="${BGDESK_INSTALL_DIR:-${HOME}/bgdesk}"
IMAGE_NAME="${BGDESK_IMAGE:-boagestao/bgdesk-server:latest}"

info() {
  printf '==> %s\n' "$*"
}

error() {
  printf 'Error: %s\n' "$*" >&2
  exit 1
}

detect_arch() {
  local machine
  machine="$(uname -m)"
  case "$machine" in
    x86_64 | amd64) echo "amd64" ;;
    aarch64 | arm64) echo "arm64v8" ;;
    armv7l | armv6l) echo "armv7" ;;
    *) error "unsupported architecture: ${machine}" ;;
  esac
}

check_dependencies() {
  local missing=()
  for cmd in curl docker; do
    command -v "$cmd" >/dev/null 2>&1 || missing+=("$cmd")
  done

  if [ "${#missing[@]}" -gt 0 ]; then
    error "missing required commands: ${missing[*]}"
  fi

  if ! docker info >/dev/null 2>&1; then
    error "docker daemon is not running or current user cannot access it"
  fi

  if docker compose version >/dev/null 2>&1; then
    COMPOSE_CMD=(docker compose)
  elif command -v docker-compose >/dev/null 2>&1; then
    COMPOSE_CMD=(docker-compose)
  else
    error "docker compose is required (install the Docker Compose plugin or docker-compose)"
  fi
}

parse_json_field() {
  local json="$1"
  local field="$2"
  printf '%s' "$json" | tr ',' '\n' | sed -n "s/.*\"${field}\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" | head -n1
}

get_latest_git_tag() {
  local response
  response="$(curl -fsSL "https://api.github.com/repos/${REPO}/tags?per_page=1" 2>/dev/null)" || return 1
  parse_json_field "$response" "name"
}

resolve_release_tag() {
  local response tag latest_git_tag

  if [ -n "${BGDESK_TAG:-}" ]; then
    printf '%s' "$BGDESK_TAG"
    return
  fi

  if response="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null)"; then
    tag="$(parse_json_field "$response" "tag_name")"
    if [ -n "$tag" ]; then
      printf '%s' "$tag"
      return
    fi
  fi

  if response="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases?per_page=5" 2>/dev/null)"; then
    tag="$(parse_json_field "$response" "tag_name")"
    if [ -n "$tag" ]; then
      printf '%s' "$tag"
      return
    fi
  fi

  latest_git_tag="$(get_latest_git_tag || true)"
  if [ -n "$latest_git_tag" ] && \
    response="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/tags/${latest_git_tag}" 2>/dev/null)"; then
    tag="$(parse_json_field "$response" "tag_name")"
    if [ -n "$tag" ]; then
      printf '%s' "$tag"
      return
    fi
  fi

  if [ -n "$latest_git_tag" ]; then
    error "no published releases for ${REPO} yet. Tag ${latest_git_tag} exists but assets are still being built. Wait for GitHub Actions to finish, then retry: https://github.com/${REPO}/actions"
  fi

  error "no published releases found for ${REPO}. Check https://github.com/${REPO}/releases"
}

get_release_asset_url() {
  local tag="$1"
  local arch="$2"
  printf 'https://github.com/%s/releases/download/%s/bgdesk-server-linux-%s.zip' "$REPO" "$tag" "$arch"
}

extract_zip() {
  local zip_file="$1"
  local dest_dir="$2"

  if command -v unzip >/dev/null 2>&1; then
    unzip -qo "$zip_file" -d "$dest_dir"
    return
  fi

  if command -v python3 >/dev/null 2>&1; then
    python3 - "$zip_file" "$dest_dir" <<'PY'
import sys
import zipfile

zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])
PY
    return
  fi

  error "unzip or python3 is required to extract the release archive"
}

find_binary() {
  local name="$1"
  local root="$2"
  local path

  path="$(find "$root" -type f -name "$name" | head -n1)"
  [ -n "$path" ] || error "binary ${name} not found in release archive"
  printf '%s' "$path"
}

write_dockerfile() {
  local dir="$1"
  cat >"${dir}/Dockerfile" <<'EOF'
FROM scratch
COPY hbbs /usr/bin/hbbs
COPY hbbr /usr/bin/hbbr
WORKDIR /root
ENV HOME=/root
EOF
}

write_compose_file() {
  local dir="$1"

  cat >"${dir}/docker-compose.yml" <<EOF
services:
  hbbs:
    container_name: hbbs
    image: ${IMAGE_NAME}
    command: hbbs
    volumes:
      - ./data:/root
    network_mode: "host"
    depends_on:
      - hbbr
    restart: unless-stopped

  hbbr:
    container_name: hbbr
    image: ${IMAGE_NAME}
    command: hbbr
    volumes:
      - ./data:/root
    network_mode: "host"
    restart: unless-stopped
EOF
}

main() {
  local arch tag asset_url tmp_dir build_dir zip_file

  check_dependencies

  arch="$(detect_arch)"
  tag="$(resolve_release_tag)"
  asset_url="$(get_release_asset_url "$tag" "$arch")"

  info "installing BGDesk Server ${tag} (${arch}) into ${INSTALL_DIR}"

  mkdir -p "${INSTALL_DIR}/data"

  tmp_dir="$(mktemp -d)"
  build_dir="${tmp_dir}/docker-build"
  mkdir -p "$build_dir"
  trap 'rm -rf "$tmp_dir"' EXIT

  zip_file="${tmp_dir}/bgdesk-server-linux-${arch}.zip"
  info "downloading ${asset_url}"
  if ! curl -fsSL "$asset_url" -o "$zip_file"; then
    error "failed to download ${asset_url}. Release ${tag} may not include linux-${arch} assets yet."
  fi

  info "extracting release archive"
  extract_zip "$zip_file" "${tmp_dir}/extracted"

  cp "$(find_binary hbbs "${tmp_dir}/extracted")" "${build_dir}/hbbs"
  cp "$(find_binary hbbr "${tmp_dir}/extracted")" "${build_dir}/hbbr"
  chmod +x "${build_dir}/hbbs" "${build_dir}/hbbr"

  write_dockerfile "$build_dir"

  info "building docker image ${IMAGE_NAME}"
  docker build -t "$IMAGE_NAME" "$build_dir"

  write_compose_file "$INSTALL_DIR"

  info "installation complete"
  printf '\n'
  printf 'Directory: %s\n' "$INSTALL_DIR"
  printf 'Version:   %s\n' "$tag"
  printf '\n'
  printf 'Start the server with:\n'
  printf '  cd %s\n' "$INSTALL_DIR"
  printf '  %s up -d\n' "${COMPOSE_CMD[*]}"
  printf '\n'
  printf 'After the first start, check ./data/id_ed25519.pub for your public key.\n'
}

main "$@"
