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

You can find updated binaries on the [Releases](https://github.com/bgdesk/bgdesk-server/releases) page.

If you want extra features, [BGDesk Server Pro](https://bgdesk.com/pricing.html) might suit you better.

If you want to develop your own server, [bgdesk-server-demo](https://github.com/bgdesk/bgdesk-server-demo) might be a better and simpler start for you than this repo.

## Installation

Run this command on your Linux server to download the latest release for your platform, build the Docker image, and prepare everything for `docker compose up`:

```bash
curl -fsSL https://raw.githubusercontent.com/bgdesk/bgdesk-server/main/install.sh | bash
```

This creates a `~/bgdesk/` directory with `docker-compose.yml`, a `data/` volume, and a local `bgdesk/bgdesk-server:latest` image.

Optional environment variables:

- `BGDESK_INSTALL_DIR` — installation directory (default: `~/bgdesk`)
- `BGDESK_RELAY_HOST` — public IP or domain used in `hbbs -r` (default: auto-detected)
- `BGDESK_REPO` — GitHub repository for releases (default: `bgdesk/bgdesk-server`)

Start the server after installation:

```bash
cd ~/bgdesk
docker compose up -d
```

For more details, please follow this [doc](https://bgdesk.com/docs/en/self-host/bgdesk-server-oss/)
