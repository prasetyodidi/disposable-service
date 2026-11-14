# Disposable Service Workspace

An on-demand, zero-footprint ephemeral workspace built for privacy-sensitive tasks—starting with **Markdown to PDF conversion via Pandoc & LaTeX**.

Styled with the **Airbnb Inspired Design System** using **Tailwind CSS v4** on the frontend, and powered by a high-performance **Rust (Axum)** backend with an automated **Rust CLI Control Plane**.

---

## 🌟 Key Features

* **Zero-Footprint:** Temporary input and output files are created in isolated temporary sandboxes and destroyed immediately via RAII drop guards upon streaming download.
* **Single Password Auth:** Instance-scoped password setup on first access (Claim on First Access).
* **1-Hour Ephemeral Sessions:** JWT tokens issued in `HttpOnly` cookies that expire after 1 hour.
* **Active Sessions Monitor:** Real-time visibility of active sessions tracked by device, user-agent, and IP address.
* **Automated CLI Control Plane:** Single-command deployment (`--existing`) over SSH to any target Linux server with automated SFTP transfer and Docker Compose orchestration.
* **Pre-Built Docker Container:** Multi-stage image packaging the static Astro frontend, compiled Rust backend, Pandoc, and `pdflatex`.
* **Privacy Micro-Apps:** Metadata Scrubber (JPEG/PNG/PDF), Secret Redactor, and JWT Inspector, all processed in memory with no third-party calls.
* **Burn Workspace:** One-action emergency destruction that wipes in-memory storage, proves 0 bytes remain, and stops the server process.

---

## 🛠 Technology Stack

| Layer | Technology |
| :--- | :--- |
| **Backend** | **Rust** (`axum`, `tokio`, `argon2`, `jsonwebtoken`) |
| **Frontend** | **Astro 5 + React 19** (Client-Side Rendering) |
| **Styling** | **Tailwind CSS v4** (`@tailwindcss/vite`) + **Airbnb Design System** |
| **Document Engine**| **Pandoc** with `pdflatex` (TeX Live) |
| **CLI Control Plane**| **Rust** (`clap`, `ssh2`, `indicatif`, `console`) |
| **Packaging** | Multi-Stage **Docker** & **Docker Compose** |

---

## 🚀 Quick Start Guide

### 1. Build and Run Locally for Development

#### A. Build the Frontend
```bash
cd frontend
npm install
npm run build
cd ..
```

#### B. Run the Rust Backend Server
```bash
# Sets up server and serves static frontend from frontend/dist
cargo run -p disposable-server
```
Visit `http://localhost:8080` (or `http://localhost:80` if running as root).

---

### 2. Building the Pre-built Docker Image

Build the multi-stage Docker image locally:
```bash
./deploy/build-image.sh disposable-workspace:latest
```

Or run directly with Docker Compose:
```bash
docker compose -f deploy/docker-compose.yml up -d
```

---

### 3. Deploying via CLI (Two Operational Modes)

The CLI supports two flexible deployment modes:

#### Multi-Platform Binary Compilation (macOS + Linux)
You can compile release binaries for both macOS and Linux into the `bin/` folder in one command:
```bash
./scripts/build-binaries.sh
```
This produces:
* `bin/disposable-cli-macos`: Native macOS executable.
* `bin/disposable-cli-linux-amd64`: Linux x86_64 ELF executable (runs on Ubuntu, Debian, AWS EC2, etc.).

---

#### Mode A: Remote Deployment (From Developer Laptop via SSH)
Connects from your laptop to the target Linux server via SSH, ensures Docker is ready, uploads configuration, and launches the service.

```bash
# Using compiled macOS binary directly from bin/:
./bin/disposable-cli-macos deploy --existing \
  --host 192.168.1.100 \
  --port 22 \
  --user ubuntu \
  --key ~/.ssh/id_ed25519

# Or auto-install Docker without interactive prompt on a fresh empty server:
./target/release/disposable-cli deploy --existing \
  --host 192.168.1.100 \
  --user ubuntu \
  --key ~/.ssh/id_ed25519 \
  --install-docker

# Or prepare a fresh server by installing Docker & Docker Compose beforehand:
./target/release/disposable-cli install-docker \
  --host 192.168.1.100 \
  --user ubuntu \
  --key ~/.ssh/id_ed25519
```

#### Mode B: Local Deployment (Directly on Target Server)
On a fresh Linux server, simply clone the repository and run the CLI directly without requiring SSH keys or network parameters:

```bash
# On the target server:
git clone https://github.com/<YOUR_ORG>/disposable-service.git
cd disposable-service

# Deploy locally (auto-installs Docker if server is fresh & launches the service)
cargo run -p disposable-cli -- deploy --local

# Or with pre-compiled release binary:
./target/release/disposable-cli deploy --local

# Or prepare Docker locally before deploying:
cargo run -p disposable-cli -- install-docker --local
```

Once deployment completes, the CLI will output the direct access URL:
```text
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🚀 Disposable Service Workspace is LIVE!
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Access URL:  http://50.19.154.110:80
  First Step:  Open the URL in your browser to claim and set your Single Password.
  Retention:   Strict Zero-Footprint ephemeral storage.
```

---

### 4. Teardown and Cleanup (Zero Trace)

When you are finished with the workspace, tear down all containers and remove all files cleanly:

#### Mode A: Remote Teardown (From Laptop)
```bash
./target/release/disposable-cli teardown \
  --host 192.168.1.100 \
  --user ubuntu \
  --key ~/.ssh/id_ed25519
```

#### Mode B: Local Teardown (Directly on Server)
```bash
./target/release/disposable-cli teardown --local
# or via cargo:
cargo run -p disposable-cli -- teardown --local
```

#### Option C: Manual via SSH
```bash
ssh ubuntu@192.168.1.100 "cd /opt/disposable-service && docker compose down -v && sudo rm -rf /opt/disposable-service"
```

---

## 🤖 Model Context Protocol (MCP) Integration

Disposable Service Workspace exposes a remote [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server over HTTP Server-Sent Events (SSE). This enables AI agent environments—such as Claude Desktop, Cursor, and custom agent systems—to securely leverage isolated pandoc document compilation and read-only host telemetry without retaining sensitive data.

### Protocol & Transport

* **Transport:** HTTP Server-Sent Events (SSE)
* **Handshake Stream:** `GET /api/mcp/sse` (establishes SSE stream and advertises session message endpoint)
* **Message Dispatch:** `POST /api/mcp/message?sessionId=<SESSION_ID>` (JSON-RPC 2.0 transport)
* **Protocol Version:** `2024-11-05`

### Authentication

All MCP endpoints enforce session authentication. Provide the instance JWT session token using the standard HTTP `Authorization` header:

```http
Authorization: Bearer <YOUR_JWT_SESSION_TOKEN>
```

> **Obtaining a token:** Tokens are issued when logging into the instance via `POST /api/auth/login` (or retrieved from your browser cookie/session). Tokens remain valid for 1 hour.

### Client Configuration

Configure your MCP-compatible client (such as Claude Desktop or Cursor) by adding the server definition:

```json
{
  "mcpServers": {
    "disposable-service": {
      "url": "https://<YOUR-SERVER-DOMAIN-OR-IP>/api/mcp/sse",
      "headers": {
        "Authorization": "Bearer <YOUR_JWT_SESSION_TOKEN>"
      }
    }
  }
}
```

### Available Tools

#### 1. `convert_markdown_to_pdf`
Renders Markdown content into a clean PDF via Pandoc (LaTeX in the Docker image, Typst in the Lambda image) in a temporary sandbox.

* **Parameters:**
  * `markdown` (string, **required**): Raw Markdown text to convert.
  * `filename` (string, *optional*, default: `document.pdf`): Target name for the compiled PDF.
  * `preset` (object, *optional*): Pandoc Preset Specs overrides (see below).
* **Output:**
  Returns an ephemeral, one-time streaming download link:
  ```text
  Successfully generated PDF.
  Filename: document.pdf
  Download URL: /api/apps/pandoc/download/3fa85f64-5717-4562-b3fc-2c963f66afa6
  Note: This is a one-time, burn-after-reading download link expiring in 5 minutes.
  ```
* **Zero-Retention Guarantee:**
  * Pandoc compilation operates in an isolated temporary directory guarded by RAII drop cleanup.
  * The compiled PDF binary is buffered in volatile RAM with a strict 5-minute time-to-live (TTL).
  * **Burn-After-Reading:** As soon as the binary is streamed through the download endpoint, it is permanently deleted from memory.

#### 2. `get_host_metrics`
Retrieves a real-time, read-only snapshot of host resource usage and operating conditions.

* **Parameters:** None.
* **Output:** JSON object containing:
  * **CPU:** Utilization percentage across physical cores.
  * **Memory:** RAM total/used/free and Swap allocation.
  * **Disks:** Mount points, total space, and disk usage percentages.
  * **System:** Host uptime and top active processes sorted by CPU and memory consumption.

#### 3. `redact_secrets`
Masks secrets in text, `.env` content, and logs (same engine as `POST /api/apps/redactor`).

* **Parameters:** text (1 to 1,048,576 characters), plus optional category toggles and custom words.
* **Output:** Redacted text and replacement counts per category (0 for categories without matches).

#### 4. `inspect_jwt`
Decodes a JWT locally (same engine as `POST /api/apps/jwt`).

* **Parameters:** `token` (string, 1 to 8,192 characters), optional HMAC secret and redaction option.
* **Output:** Formatted header and payload, claim times in ISO 8601 UTC, token status, and warnings (e.g. `alg: none`).

#### 5. `scrub_metadata`
Strips sensitive metadata from a JPEG, PNG, or PDF.

* **Parameters:** base64-encoded file (decoded size 1 byte to 25 MB).
* **Output:** A one-time Ephemeral Download URL (`/api/apps/scrubber/download/:token`, 5-minute TTL) and the Scrub Report.

#### 6. `burn_workspace_now`
Permanently destroys the workspace. Cannot be undone.

* **Parameters:** `confirm` (boolean, **required**, must be exactly `true`). Otherwise the call fails with JSON-RPC error `-32602` and nothing is deleted.
* **Output:** The Burn Audit Report, after which the server process exits. See [Burn Workspace](#-privacy-micro-apps--burn-workspace).

### Security & Ephemeral Guarantees

* **Zero Footprint:** No markdown documents, compiled PDFs, or telemetry snapshots are stored in persistent databases or non-volatile disks.
* **Automatic Eviction:** Unread downloads automatically expire and purge after 5 minutes.
* **Session Lifecycle:** Dropping the MCP SSE connection immediately triggers cleanup of associated channel state and background tasks.

---

## 🛡 Privacy Micro-Apps & Burn Workspace

All routes below require an authenticated session (otherwise `401`). Nothing is persisted; user content never reaches server logs.

### Micro-Apps

| Micro-App | Page | Description |
| :--- | :--- | :--- |
| **Metadata Scrubber** | `/scrubber` | Removes GPS, device/lens serials, author, creator app, embedded thumbnails, XMP/IPTC from JPEG/PNG, and the Info dictionary + XMP stream from PDF. Pixels and PDF text are preserved. Max 25 MB. Returns a Scrub Report (categories and counts only, never values). |
| **Secret Redactor** | `/redactor` (tab "Log & .env Redactor") | Replaces JWTs, bearer tokens, API keys, `.env` values, emails, phone numbers, IPv4/IPv6, credit cards, and custom words with `[REDACTED:<category>]`. Max 5 MB. |
| **JWT Inspector** | `/redactor` (tab "JWT Inspector") | Decodes header/payload, converts `exp`/`iat`/`nbf` to ISO 8601, reports status, warns on `alg: none`, optionally verifies HS256/384/512 and redacts identity claims. Max 8,192 characters. |

### API Routes

| Method | Route | Purpose |
| :--- | :--- | :--- |
| `POST` | `/api/apps/scrubber` | Upload a file to scrub (`415` unsupported type, `422` corrupt/encrypted, `413` over 25 MB) |
| `GET` | `/api/apps/scrubber/download/:token` | One-time download of a scrubbed file (5-minute TTL, burn-after-reading) |
| `POST` | `/api/apps/redactor` | Redact secrets in text (`413` over 5 MB, `400` invalid custom words) |
| `POST` | `/api/apps/jwt` | Inspect a JWT (`422` malformed token) |
| `POST` | `/api/burn` | Trigger Burn Workspace and return the Audit Report |
| `GET` | `/api/burn/events` | SSE stream; emits a `burn` event with the Audit Report when a burn starts (e.g. triggered via MCP) |

### MCP Tools

`redact_secrets`, `inspect_jwt`, `scrub_metadata`, and `burn_workspace_now` (see [Available Tools](#available-tools)).

### Burn Workspace Behavior

* **Trigger:** The Burn button in the Navbar (with a confirmation dialog, or a 3-second cancellable countdown in Demo mode), `POST /api/burn`, or the MCP tool `burn_workspace_now`.
* **Managed_Storage scope:** `sessions`, `downloads`, and `mcp_sessions` in AppState, plus the workspace temporary directory. Download buffers are overwritten with zeros before being freed, MCP SSE connections are closed, and the auth cookie is cleared.
* **0-byte audit:** The Audit Report lists entries and bytes per category before and after, duration, and a UTC timestamp. After re-verification, `verified: true` and `remaining_bytes: 0` mean 0 bytes remain; the UI shows "0 bytes remaining". If anything remains or verification is incomplete, `verified` is `false`. The scope is Managed_Storage only, not the whole process memory.
* **Concurrency:** Concurrent burns run once and share one report. While burning, micro-app requests return `503`.
* **Process exit:** After the report is sent, the server exits with code `0` (within 2 seconds, or 5 seconds if the client disconnects). If a normal exit fails, it is forced with a non-zero code.

### Container Restart Policy (`RESTART_POLICY`)

`deploy/docker-compose.yml` uses `restart: ${RESTART_POLICY:-on-failure}`. The default is `on-failure`, so the container stays stopped after a burn (exit code `0`). `no` behaves the same. With `unless-stopped` or `always`, Docker **restarts the container after a burn**, and the Audit Report includes a warning about it. Keep the default if you want the workspace to stay gone.

```bash
RESTART_POLICY=on-failure docker compose -f deploy/docker-compose.yml up -d
```

> The CLI `teardown` command is unchanged and does not use Burn Workspace.

---

## 🎨 Airbnb Inspired Design Tokens

Custom design tokens are integrated using Tailwind CSS v4 in `frontend/src/styles/global.css`:

* **Brand Voltage (Rausch):** `#ff385c` (Active: `#e00b41`, Disabled: `#ffd1da`)
* **Surfaces:** Pure canvas `#ffffff`, Soft floor `#f7f7f7`, Strong `#f2f2f2`
* **Typography:** `#222222` (Ink), `#6a6a6a` (Muted), Inter font family
* **Elevation Shadow:** `rgba(0, 0, 0, 0.02) 0 0 0 1px, rgba(0, 0, 0, 0.04) 0 2px 6px, rgba(0, 0, 0, 0.1) 0 4px 8px`
* **Border Radii:** `8px` (inputs & buttons), `14px` (cards & modals), `9999px` (pills & badges)


## 📄 Pandoc Preset Specs

The Markdown to PDF converter uses a preset that can be adjusted per request. The active engine is `typst` when `DOC_ENGINE=typst` (Lambda image) and the LaTeX engine from `PANDOC_PDF_ENGINE` (default `pdflatex`) otherwise.

| Field | Default | Allowed values |
| :--- | :--- | :--- |
| `paper_size` | `a4` | `a3`, `a4`, `a5`, `letter`, `legal` |
| `margin_x` (left/right) | `2cm` | number + `cm`/`mm`/`in`/`pt` (max 10cm, 100mm, 4in, 300pt) |
| `margin_y` (top/bottom) | `1.8cm` | same as `margin_x` |
| `font_size` | `10.5pt` | `6pt` to `24pt` |
| `line_height` | `1.25` | `1.0` to `2.5` (Typst: paragraph leading = line height - 0.6 em) |
| `link_color` | `navy` | `navy`, `black`, `blue`, `red`, `green`, `purple`, `teal`, `gray` |
| `main_font` | `PANDOC_MAIN_FONT` or engine default | letters, digits, spaces, hyphens (max 64) |

* `GET /api/apps/pandoc/preset`: returns the active engine, the defaults, and the allowed values. The web UI card "Pandoc Preset Specs" reads this and lets you edit the values for the next conversion.
* `POST /api/apps/pandoc/convert`: JSON body accepts an optional `preset` object with any of the fields above.
* `POST /api/apps/pandoc/convert-upload`: multipart accepts an optional `preset` field containing the same JSON.
* An invalid value returns `400` with `Invalid preset: ...`. Values are validated before they reach Pandoc, so they cannot inject options.
