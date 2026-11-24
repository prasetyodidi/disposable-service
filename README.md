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
* **PDF to Markdown:** Converts text-layer PDFs to Markdown in memory (page `/pdf-to-markdown`, MCP tool `pdf_to_markdown`). See [PDF to Markdown](#-pdf-to-markdown).
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

Disposable Service Workspace exposes a remote [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server. AI agent environments such as Kiro, Claude Desktop, Cursor and custom agents can call its seven tools: document conversion (Markdown ↔ PDF), metadata scrubbing, secret redaction, JWT inspection, read-only host telemetry, and the emergency Burn Workspace. All processing happens in memory or in temporary directories that are deleted after each call.

### Protocol & Transport

The server speaks JSON-RPC 2.0, protocol version `2024-11-05`. Two transports are available on the same server:

| Transport | Endpoints | Use it when |
| :--- | :--- | :--- |
| **HTTP POST (request/response)** | `POST /api/mcp/sse` or `POST /api/mcp` | Default for most clients, and the **only one that works on AWS Lambda**. The JSON-RPC response is returned in the HTTP response body. |
| **SSE (legacy)** | `GET /api/mcp/sse` opens a stream and sends an `endpoint` event with `/api/mcp/message?sessionId=<id>`; requests are then sent with `POST /api/mcp/message?sessionId=<id>` and answered on the stream | Long-running servers (Docker). Not usable on Lambda: the Function URL runs in `BUFFERED` mode, so the stream cannot be delivered. |

Supported JSON-RPC methods: `initialize`, `ping`, `tools/list`, `tools/call`, and the notification `notifications/initialized`. Any other method returns `-32601` (method not found).

Only **tools** are implemented. MCP **resources** (`resources/list`, `resources/read`) and **prompts** (`prompts/list`, `prompts/get`) are not supported.

### Authentication

Every MCP request needs a token in the `Authorization` header. Requests without a valid token get HTTP `401` before any tool runs.

```http
Authorization: Bearer <TOKEN>
```

Two kinds of tokens work:

* **MCP access token (recommended):** created from the Dashboard (see below). Lifetime 1 to 365 days.
* **Login session token:** issued by `POST /api/auth/login` (the `disposable_session` cookie value). Valid for 1 hour.

### Creating a token from the Dashboard

Open the Dashboard and use the **MCP access tokens** panel: enter a label, pick a lifetime (1 to 365 days) and press **Create token**. The token is shown once, together with a ready-to-paste client configuration. The same operation is available as `POST /api/auth/token` with `{"lifetime_seconds": 2592000, "label": "Kiro"}` (label max 64 characters, no control characters). Tokens cannot be revoked yet: they stay valid until they expire or `JWT_SECRET` changes, so use short lifetimes.

### Client Configuration

Add the server to your MCP client (Kiro: `.kiro/settings/mcp.json`; Claude Desktop and Cursor use the same `mcpServers` shape):

```json
{
  "mcpServers": {
    "disposable-service": {
      "url": "https://<YOUR-SERVER-DOMAIN-OR-IP>/api/mcp/sse",
      "headers": {
        "Authorization": "Bearer <TOKEN>"
      }
    }
  }
}
```

Quick manual test (HTTP POST transport):

```bash
curl -s -X POST "https://<HOST>/api/mcp" \
  -H "Authorization: Bearer <TOKEN>" -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

### Size Limits for Tool Inputs

Files are passed as base64 inside the JSON-RPC request, which makes them about 33% larger.

| Limit | Value |
| :--- | :--- |
| MCP request body | 36 MiB |
| AWS Lambda Function URL request payload | about 6 MB, so in practice a file of at most about 4.4 MB fits in one call on Lambda |
| `scrub_metadata` file | 25 MB decoded |
| `pdf_to_markdown` file | The PDF to Markdown upload limit (25 MB, 5 MB on Lambda; see the PDF to Markdown environment variables), 300 pages |
| `redact_secrets` text | 1,048,576 characters |
| `inspect_jwt` token / secret | 8,192 characters / 1,024 bytes |

### Ephemeral Download Links

`convert_markdown_to_pdf`, `scrub_metadata` and long `pdf_to_markdown` results return a **relative** download path. Prefix it with the server's base URL. Each link works **once** and expires after **5 minutes**.

| Tool | Path | Needs `Authorization` |
| :--- | :--- | :--- |
| `convert_markdown_to_pdf` | `/api/apps/pandoc/download/<token>` | No (the token is the secret) |
| `scrub_metadata` | `/api/apps/scrubber/download/<token>` | No (the token is the secret) |
| `pdf_to_markdown` (over 100,000 characters) | `/api/apps/pdf-to-markdown/download/<token>` | Yes |

### Error Handling

| Situation | Response |
| :--- | :--- |
| Missing or invalid token | HTTP `401` |
| Burn Workspace running or finished | HTTP `503` with `"code": "burn_in_progress"` |
| Invalid arguments (missing, wrong type, out of range, bad base64, unknown category or argument) | JSON-RPC error `-32602`; the message names the argument. Nothing runs and nothing is stored |
| `pdf_to_markdown` document problem | JSON-RPC error `-32000` with `data.code`: `not_a_pdf`, `corrupt_pdf`, `encrypted_pdf`, `no_text_layer`, `too_many_pages`, `page_from_out_of_range`, `output_too_large`, `timeout`, `engine_unavailable` |
| Internal failure (including a failed burn) | JSON-RPC error `-32603` |
| Content problems in the other tools (unsupported file type, malformed JWT, Pandoc failure) | A normal result with `isError: true` and a text message |
| Unknown tool name | Result with `isError: true` ("Unknown tool") |
| Unknown method | JSON-RPC error `-32601` |

### Available Tools

| # | Tool | What it does | Destructive |
| :--- | :--- | :--- | :--- |
| 1 | `convert_markdown_to_pdf` | Markdown → PDF with Pandoc | No |
| 2 | `pdf_to_markdown` | Text-layer PDF → Markdown | No |
| 3 | `scrub_metadata` | Remove metadata from JPEG, PNG, PDF | No |
| 4 | `redact_secrets` | Mask secrets in text, `.env` and logs | No |
| 5 | `inspect_jwt` | Decode and optionally verify a JWT | No |
| 6 | `get_host_metrics` | Read-only host telemetry | No |
| 7 | `burn_workspace_now` | Wipe the workspace and stop the server | **Yes, irreversible** |

#### 1. `convert_markdown_to_pdf`
Renders Markdown into a PDF with Pandoc (LaTeX in the Docker image, Typst in the Lambda image) in a temporary directory.

* **Parameters:**
  * `markdown` (string, **required**, not empty): Markdown text.
  * `filename` (string, optional, default `document.pdf`): output name; `.pdf` is added if missing.
  * `preset` (object, optional): [Pandoc Preset Specs](#-pandoc-preset-specs) overrides. Fields: `paper_size` (`a3`, `a4`, `a5`, `letter`, `legal`), `margin_x`, `margin_y` (number + `cm`/`mm`/`in`/`pt`), `font_size` (`6pt` to `24pt`), `line_height` (1.0 to 2.5), `link_color` (`navy`, `black`, `blue`, `red`, `green`, `purple`, `teal`, `gray`), `main_font`. Unset fields use the server defaults.
* **Output:** a text item with file name, size and the download path, plus a `resource` item containing the PDF as base64 (`application/pdf`).
* **Errors:** empty `markdown` or a Pandoc failure returns `isError: true`; an invalid `preset` returns `isError: true` with the reason.

#### 2. `pdf_to_markdown`
Converts a PDF that has a text layer into Markdown: headings (by font size), paragraphs, lists, code blocks, bold/italic; repeated headers, footers and page numbers are removed. No OCR, tables, multi-column layout or images.

* **Parameters:**
  * `file_base64` (string, **required**): the PDF, base64-encoded.
  * `page_from`, `page_to` (integers ≥ 1, optional): page range, inclusive. Defaults: first to last page. `page_to` beyond the last page is clamped with a warning.
  * `page_breaks` (boolean, optional, default `false`): insert `---` between pages.
  * `strip_headers_footers` (boolean, optional, default `true`).
  * Unknown arguments are rejected with `-32602`.
* **Output (JSON in a text item):** `markdown`, `pages`, `headings`, `warnings`, `truncated`. When the Markdown exceeds 100,000 characters, `markdown` is replaced by a 2,000-character `preview` and a `download_url`, and `truncated` is `true`. Output is identical to `POST /api/apps/pdf-to-markdown`.
* **Errors:** see the `-32000` codes above. A scanned PDF returns `no_text_layer`.

#### 3. `scrub_metadata`
Removes GPS, device and lens serials, author, creator software, embedded thumbnails and XMP/IPTC from JPEG and PNG, and the Info dictionary and XMP stream from PDF. Pixels and PDF text are kept.

* **Parameters:**
  * `file_base64` (string, **required**): JPEG, PNG or PDF, base64-encoded (1 byte to 25 MB decoded).
  * `filename` (string, optional): name for the cleaned file; only the base name is kept. Default `scrubbed.<ext>`.
* **Output (JSON):** `download_url`, `filename`, `report` (categories and counts removed, never the values) and the expiry.
* **Errors:** unsupported type, unreadable file or encrypted PDF returns `isError: true`.

#### 4. `redact_secrets`
Replaces secrets with `[REDACTED:<category>]` (same engine as `POST /api/apps/redactor`). Line count is preserved and running it twice changes nothing.

* **Parameters:**
  * `text` (string, **required**, 1 to 1,048,576 characters).
  * `categories` (array of strings, optional, default all): `jwt`, `bearer`, `api_key`, `env_value`, `email`, `phone`, `ipv4`, `ipv6`, `credit_card`, `custom`. Unknown values return `-32602`.
  * `custom_words` (array of strings, optional): up to 100 words, each 1 to 256 characters, matched exactly (case-sensitive). Applied when `custom` is active.
  * `format` (string, optional, default `auto`): `auto`, `env` or `text`. `env` keeps variable names and replaces values.
* **Output (JSON):** `text` (redacted) and `counts` per active category (0 when nothing matched).

#### 5. `inspect_jwt`
Decodes a JWT on the server without contacting third parties (same engine as `POST /api/apps/jwt`).

* **Parameters:**
  * `token` (string, **required**, 1 to 8,192 characters).
  * `secret` (string, optional, 1 to 1,024 bytes): verifies HS256, HS384 or HS512 signatures.
  * `redact` (boolean, optional, default `false`): replaces `email`, `phone_number`, `phone`, `sub`, `name` and `preferred_username` in the payload with `[REDACTED:...]`.
* **Output (JSON):** header and payload (raw and pretty), `exp`/`iat`/`nbf` as ISO 8601 UTC, `status` (`active`, `expired`, `not_yet_valid`), `signature` (`not_requested`, `valid`, `invalid`, `unsupported`), and `warnings` (for example `alg: none`).
* **Errors:** a malformed token returns `isError: true` naming the broken segment.

#### 6. `get_host_metrics`
Returns a read-only snapshot of the host.

* **Parameters:** none.
* **Output (JSON):** CPU usage per core, RAM and swap, disks (mount point, total, used), uptime, and the top processes by CPU and memory. On Lambda this describes the Lambda execution environment.

#### 7. `burn_workspace_now`
**Permanent and irreversible.** Wipes all sessions, ephemeral downloads, MCP sessions and temporary files, returns the Burn Audit Report, then stops the server process. Agents should call it only when the user explicitly asks for it.

* **Parameters:** `confirm` (boolean, **required**, must be exactly `true`). Anything else returns `-32602` and nothing is deleted.
* **Output:** the Burn Audit Report (see [Burn Workspace](#-privacy-micro-apps--burn-workspace)). Over SSE the report is sent as an event, then the stream closes.
* **Errors:** a burn already in progress returns an error ("penghancuran sedang berlangsung"); a failed burn returns `-32603` and the process keeps running.

### Security & Ephemeral Guarantees

* **Zero footprint:** no document, PDF, Markdown or telemetry snapshot is stored in a database or on persistent disk.
* **Automatic eviction:** unread downloads are deleted after 5 minutes; Burn Workspace deletes them immediately.
* **No content in logs:** file names, file contents, tokens, secrets and redacted text are never logged.
* **Session lifecycle:** closing an SSE connection removes its session state.


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

`redact_secrets`, `inspect_jwt`, `scrub_metadata` and `burn_workspace_now`, plus `pdf_to_markdown`, `convert_markdown_to_pdf` and `get_host_metrics` (see [Available Tools](#available-tools)).

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

## 📑 PDF to Markdown

Upload a PDF with a text layer on `/pdf-to-markdown` and get Markdown back: headings, paragraphs, lists, code blocks, bold/italic, with repeated headers/footers and page numbers removed. The PDF is processed in memory and in a temporary directory that is deleted right after the request. The document never leaves the workspace.

### API Routes

All routes require an authenticated session (`401` otherwise) and return `503` while a burn is running.

| Method | Route | Purpose |
| :--- | :--- | :--- |
| `POST` | `/api/apps/pdf-to-markdown` | Multipart `file` plus optional `options` JSON (`page_from`, `page_to`, `page_breaks`, `strip_headers_footers`). Add `?download=true` to get a `download_token`. Errors: `400`, `413`, `415`, `422`, `504` |
| `GET` | `/api/apps/pdf-to-markdown/download/:token` | One-time Markdown download (5-minute TTL, burn-after-reading) |
| `GET` | `/api/apps/pdf-to-markdown/limits` | Effective `max_bytes` and `max_pages` |

The MCP tool is `pdf_to_markdown` (see [Available Tools](#available-tools)).

### Limits

* **Upload size:** 25 MB (5 MB on the Lambda image).
* **Pages:** 300 per document.
* **Time:** 30 seconds per conversion.
* **Response size:** unlimited on a regular server; 5,767,168 bytes on Lambda. Larger results return `422 output_too_large`; use `page_from`/`page_to`.

### Phase 1 Scope and Limitations

* Not supported: tables, OCR (scanned PDFs without a text layer), multi-column layouts, and image extraction.
* Headings are detected by font size. A heading whose text is entirely bold renders as `# **Title**`, and headings distinguished only by bold (same size as body text) are not detected.
* Hyphenated line ends: some typesetters (for example Typst with hyphenation enabled) break a word at the end of a line without keeping the hyphen glyph in the PDF text layer. Those words come out split in two in the Markdown, and no safe heuristic can rejoin them. PDFs whose text layer keeps the `-` are joined correctly.

### Environment Variables

| Variable | Default | Description |
| :--- | :--- | :--- |
| `PDF_MD_ENGINE` | `poppler` | Extraction engine. Unknown values stop the server at startup before the port opens |
| `PDF_MD_MAX_BYTES` | `26214400` (25 MB; `5242880` / 5 MB in the Lambda image) | Max upload size in bytes. Must be a positive integer; values above 25 MB are capped at 25 MB |
| `PDF_MD_MAX_RESPONSE_BYTES` | unlimited (`5767168` in the Lambda image) | Max serialized JSON response size. Empty string counts as unset |

### Licensing

Extraction uses `pdftohtml` from poppler-utils (GPL). It runs only as a separate subprocess and is not linked into the Rust binary.

---

## 🧪 Running Poppler-Dependent Tests in Docker

The PDF to Markdown tests need `pdftohtml` and `pdfinfo` (poppler-utils), plus Pandoc and Typst for the Markdown → PDF → Markdown round trip. `scripts/test-docker.sh` runs the workspace tests in a container (`deploy/Dockerfile.test`) that has all of them, so you do not have to install them on your machine.

```bash
# Run the whole workspace test suite
scripts/test-docker.sh

# Pass a test name filter through to cargo test
scripts/test-docker.sh pdf2md

# Open an interactive bash shell in the same container
scripts/test-docker.sh --shell

# Force a platform, e.g. when a tool is not available for your native architecture
DOCKER_PLATFORM=linux/amd64 scripts/test-docker.sh pdf2md
```

* **First run:** The first build of the image and the first compile take several minutes. Later runs reuse the named volumes `disposable-cargo-registry` and `disposable-cargo-target`.
* **`DOCKER_PLATFORM`:** Optional. When set, it is passed to `docker build` and `docker run` as `--platform`. By default the native architecture is used.
* **Exit code:** The exit code of `cargo test` is returned unchanged.
* **Plain `cargo test`:** Running `cargo test --workspace` on your machine still runs every test that does not need poppler. The remaining tests are skipped and print a `SKIP` message.
* **Inside the container:** `PDF_MD_REQUIRE_TOOLS=1` is set, so a test that would be skipped because a tool is missing fails instead.

---

## 🤖 CI (GitHub Actions)

`.github/workflows/ci.yml` runs two parallel jobs on `ubuntu-latest` (amd64 only):

* **`backend`:** builds `deploy/Dockerfile.test` and runs `scripts/test-docker.sh`. The image sets `PDF_MD_REQUIRE_TOOLS=1`, so a poppler-dependent test that would be skipped fails instead.
* **`frontend`:** in `frontend/`, runs `npm ci`, `npx tsc --noEmit`, `npm test` and `npm run build`.

There is no clippy and no `cargo fmt --check` step.

**`TEST_CACHE_DIR`:** an absolute path. When set, `scripts/test-docker.sh` bind-mounts `$TEST_CACHE_DIR/cargo-registry` and `$TEST_CACHE_DIR/cargo-target` instead of the named Docker volumes. CI sets it to `${{ runner.temp }}/test-cache` so `actions/cache` can save and restore the cargo registry and build output.

**Run the same checks locally:**

```bash
# backend job
scripts/test-docker.sh
TEST_CACHE_DIR=/tmp/disposable-test-cache scripts/test-docker.sh   # same, with a host cache directory

# frontend job
cd frontend && npm ci && npx tsc --noEmit && npm test && npm run build
```

### Gating with `publish-image.yml`

`publish-image.yml` calls `ci.yml` as job `ci`.

* **Push to `main`, tag `v*.*.*`, `workflow_dispatch`:** `build-and-push` waits for `ci` to succeed.
* **Pull request:** `ci` and `build-check` (builds the image without pushing) run in parallel. Nothing is pushed.

**Emergency `skip_ci` input:** a manual `workflow_dispatch` run can set `skip_ci` to skip `ci` and still run `build-and-push`. The run summary then shows the warning "CI skipped via skip_ci". Only maintainers with write access can dispatch it, and it is meant for emergencies only.

**Branch protection (manual step for the repository owner):** this is a repository setting and cannot be done from code. Protect `main` and require the status checks `ci / backend`, `ci / frontend` and `build-check`. Confirm the exact check names after the first run.

**Re-running failures:** run `gh run rerun --failed <run-id>`, or click "Re-run failed jobs" in the GitHub UI, to re-run only the failed jobs. If a timing-sensitive or flaky test shows instability, loosen it through its environment variable.

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
