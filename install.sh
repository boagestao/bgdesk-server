#!/usr/bin/env bash
set -euo pipefail

REPO="${BGDESK_REPO:-boagestao/bgdesk-server}"
INSTALL_DIR="${BGDESK_INSTALL_DIR:-${HOME}/bgdesk}"
IMAGE_NAME="${BGDESK_IMAGE:-boagestao/bgdesk-server:latest}"
RELAY_HOST="${BGDESK_RELAY_HOST:-}"

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

get_latest_release_tag() {
  local response tag
  response="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest")" || \
    error "failed to fetch latest release from ${REPO}"

  tag="$(printf '%s' "$response" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
  [ -n "$tag" ] || error "no published releases found for ${REPO}"
  printf '%s' "$tag"
}

get_release_asset_url() {
  local tag="$1"
  local arch="$2"
  local response url

  response="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/tags/${tag}")" || \
    error "failed to fetch release metadata for tag ${tag}"

  url="$(printf '%s' "$response" | sed -n "s/.*\"browser_download_url\":[[:space:]]*\"\\([^\"]*bgdesk-server-linux-${arch}\\.zip\\)\".*/\\1/p" | head -n1)"
  [ -n "$url" ] || error "release ${tag} does not contain bgdesk-server-linux-${arch}.zip"
  printf '%s' "$url"
}

detect_relay_host() {
  if [ -n "$RELAY_HOST" ]; then
    printf '%s' "$RELAY_HOST"
    return
  fi

  curl -fsSL --max-time 5 https://api.ipify.org 2>/dev/null || \
    curl -fsSL --max-time 5 https://ifconfig.me 2>/dev/null || \
    hostname -I 2>/dev/null | awk '{print $1}' || \
    printf 'bgdesk.example.com'
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
  local relay_host="$2"

  cat >"${dir}/docker-compose.yml" <<EOF
version: '3'

networks:
  bgdesk-net:
    external: false

services:
  hbbs:
    container_name: hbbs
    ports:
      - 21115:21115
      - 21116:21116
      - 21116:21116/udp
      - 21118:21118
    image: ${IMAGE_NAME}
    command: hbbs -r ${relay_host}:21117
    volumes:
      - ./data:/root
    networks:
      - bgdesk-net
    depends_on:
      - hbbr
    restart: unless-stopped

  hbbr:
    container_name: hbbr
    ports:
      - 21117:21117
      - 21119:21119
    image: ${IMAGE_NAME}
    command: hbbr
    volumes:
      - ./data:/root
    networks:
      - bgdesk-net
    restart: unless-stopped
EOF
}

main() {
  local arch tag asset_url relay_host tmp_dir build_dir zip_file

  check_dependencies

  arch="$(detect_arch)"
  tag="$(get_latest_release_tag)"
  asset_url="$(get_release_asset_url "$tag" "$arch")"
  relay_host="$(detect_relay_host)"

  info "installing BGDesk Server ${tag} (${arch}) into ${INSTALL_DIR}"

  mkdir -p "${INSTALL_DIR}/data"

  tmp_dir="$(mktemp -d)"
  build_dir="${tmp_dir}/docker-build"
  mkdir -p "$build_dir"
  trap 'rm -rf "$tmp_dir"' EXIT

  zip_file="${tmp_dir}/bgdesk-server-linux-${arch}.zip"
  info "downloading ${asset_url}"
  curl -fsSL "$asset_url" -o "$zip_file"

  info "extracting release archive"
  extract_zip "$zip_file" "${tmp_dir}/extracted"

  cp "$(find_binary hbbs "${tmp_dir}/extracted")" "${build_dir}/hbbs"
  cp "$(find_binary hbbr "${tmp_dir}/extracted")" "${build_dir}/hbbr"
  chmod +x "${build_dir}/hbbs" "${build_dir}/hbbr"

  write_dockerfile "$build_dir"

  info "building docker image ${IMAGE_NAME}"
  docker build -t "$IMAGE_NAME" "$build_dir"

  write_compose_file "$INSTALL_DIR" "$relay_host"

  info "installation complete"
  printf '\n'
  printf 'Directory: %s\n' "$INSTALL_DIR"
  printf 'Version:   %s\n' "$tag"
  printf 'Relay:     %s:21117\n' "$relay_host"
  printf '\n'
  printf 'Start the server with:\n'
  printf '  cd %s\n' "$INSTALL_DIR"
  printf '  %s up -d\n' "${COMPOSE_CMD[*]}"
  printf '\n'
  printf 'After the first start, check ./data/id_ed25519.pub for your public key.\n'
  if [ "$relay_host" = "bgdesk.example.com" ]; then
    printf '\n'
    printf 'Warning: relay host could not be detected automatically.\n'
    printf 'Edit docker-compose.yml and replace bgdesk.example.com with your public IP or domain.\n'
  fi
}

main "$@"
