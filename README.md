# BGDesk Server Program

[![build](https://github.com/bgdesk/bgdesk-server/actions/workflows/build.yaml/badge.svg)](https://github.com/bgdesk/bgdesk-server/actions/workflows/build.yaml)

[**Download**](https://github.com/bgdesk/bgdesk-server/releases)

[**Manual**](https://bgdesk.com/docs/en/self-host/)

[**FAQ**](https://github.com/bgdesk/bgdesk/wiki/FAQ)

[**How to migrate OSS to Pro**](https://bgdesk.com/docs/en/self-host/bgdesk-server-pro/installscript/#convert-from-open-source)

Self-host your own BGDesk server, it is free and open source.

## How to build manually

```bash
cargo build --release
```

Three executables will be generated in target/release.

- hbbs - BGDesk ID/Rendezvous server
- hbbr - BGDesk relay server
- bgdesk-utils - BGDesk CLI utilities

## Custom device IDs

Clients can use **Change ID** to replace their numeric or custom ID. New IDs must
contain 6–16 ASCII characters, start with a letter (`A–Z` or `a–z`), and use only
letters, digits, `-`, or `_`. IDs are case-sensitive, as with existing device IDs.

The server handles the client's existing `RegisterPk` request over TCP/WebSocket
using `old_id`, `id`, and the machine's `uuid`; no client or protocol update is
needed. It checks ownership and availability, then renames the existing database
record while preserving its GUID, public key, metadata, and live address. The old
ID becomes available, and the new ID persists across server restarts. No database
migration or additional configuration is required.

Successful changes are limited to one per device every 60 seconds while the server
is running, in addition to the existing IP request limits. Retrying a completed
request for the same device/ID succeeds without renaming it again. The server
returns the protocol's existing errors for invalid format, occupied IDs, machine
UUID mismatch, excessive requests, and storage failures.

You can find updated binaries on the [Releases](https://github.com/bgdesk/bgdesk-server/releases) page.

If you want extra features, [BGDesk Server Pro](https://bgdesk.com/pricing.html) might suit you better.

If you want to develop your own server, [bgdesk-server-demo](https://github.com/bgdesk/bgdesk-server-demo) might be a better and simpler start for you than this repo.

## Installation

Run this command on your Linux server to download the latest release for your platform, build the Docker image, and prepare everything for `docker compose up`:

```bash
curl -fsSL https://raw.githubusercontent.com/boagestao/bgdesk-server/refs/heads/main/install.sh | bash
```

This creates a `~/bgdesk/` directory with `docker-compose.yml`, a `data/` volume, and a local `boagestao/bgdesk-server:latest` image.

Optional environment variables:

- `BGDESK_INSTALL_DIR` — installation directory (default: `~/bgdesk`)
- `BGDESK_REPO` — GitHub repository for releases (default: `boagestao/bgdesk-server`)
- `BGDESK_TAG` — install a specific release tag (default: latest published release)

Start the server after installation:

```bash
cd ~/bgdesk
docker compose up -d
```

For more details, please follow this [doc](https://bgdesk.com/docs/en/self-host/bgdesk-server-oss/)
