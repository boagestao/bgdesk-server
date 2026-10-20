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

## Instalação com um comando

Requisitos: servidor **Linux**, Bash, `curl`, Docker em execução, Docker Compose
(plugin `docker compose` ou `docker-compose`) e `unzip` ou `python3`. O usuário
precisa ter permissão para executar Docker. Arquiteturas suportadas: amd64,
arm64 e armv7.

Execute no servidor:

```bash
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/boagestao/bgdesk-server/refs/heads/main/install.sh | bash'
```

O comando baixa a versão publicada mais recente para a arquitetura do servidor,
monta a imagem Docker local e **inicia automaticamente** os serviços `hbbs` e
`hbbr`, configurados para reiniciar com o Docker. Não é necessário clonar o
repositório nem compilar Rust.

Os arquivos ficam em `~/bgdesk/`: configuração em `docker-compose.yml` e banco
de dados e chaves em `data/`. A imagem padrão é
`boagestao/bgdesk-server:latest`. Após a primeira inicialização, a chave pública
para configurar os clientes estará em `~/bgdesk/data/id_ed25519.pub`.

Libere no firewall do servidor as portas TCP `21115–21119` e UDP `21116` para
acesso dos clientes. Os serviços usam a rede do host.

## Atualização com um comando

Execute no mesmo servidor, com o mesmo usuário da instalação:

```bash
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/boagestao/bgdesk-server/refs/heads/main/install.sh | bash -s -- --update'
```

O comando baixa a versão mais recente, reconstrói a imagem local e recria os
serviços automaticamente. O arquivo Compose existente, o banco e as chaves são
preservados; as conexões podem ser interrompidas brevemente durante a reinicialização.
A atualização exige uma instalação existente no diretório indicado. Se o
download ou a montagem da imagem falhar, os serviços em execução continuam ativos.

### Opções

As mesmas variáveis opcionais funcionam na instalação e na atualização:

- `BGDESK_INSTALL_DIR` — diretório da instalação (padrão: `~/bgdesk`). Use o mesmo diretório ao atualizar.
- `BGDESK_REPO` — repositório das versões (padrão: `boagestao/bgdesk-server`).
- `BGDESK_TAG` — tag de uma versão específica (padrão: versão publicada mais recente).
- `BGDESK_IMAGE` — nome da imagem local na instalação. Ao atualizar, a imagem é obtida do Compose existente; os dois serviços devem usar a mesma imagem.

Por exemplo, para instalar em `/opt/bgdesk`:

```bash
BGDESK_INSTALL_DIR=/opt/bgdesk bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/boagestao/bgdesk-server/refs/heads/main/install.sh | bash'
```

Para atualizar essa instalação:

```bash
BGDESK_INSTALL_DIR=/opt/bgdesk bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/boagestao/bgdesk-server/refs/heads/main/install.sh | bash -s -- --update'
```

Para consultar o estado e os logs de uma instalação no diretório padrão:

```bash
cd ~/bgdesk && docker compose ps
cd ~/bgdesk && docker compose logs --tail=100 hbbs hbbr
```

Se estiver usando o Compose legado, substitua `docker compose` por `docker-compose`.
Para mais detalhes, consulte a [documentação](https://bgdesk.com/docs/en/self-host/bgdesk-server-oss/).

## Origem, copyright e licença

O **BGDesk Server** é um projeto derivado (fork) do
[RustDesk Server](https://github.com/rustdesk/rustdesk-server), parte do projeto
[RustDesk](https://github.com/rustdesk/rustdesk).

Copyright © 2022 open-trade, conforme o aviso em [debian/copyright](debian/copyright).
Os créditos pelo código original pertencem aos seus autores e contribuidores do
RustDesk. As modificações do BGDesk pertencem aos seus respectivos autores,
preservando os avisos de copyright do projeto original.

Este projeto é distribuído sob a **GNU Affero General Public License, versão 3
ou posterior (AGPL-3.0-or-later)**. Consulte o arquivo [LICENSE](LICENSE) para o
texto completo da licença.
