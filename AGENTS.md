# AGENTS.md — Developer & AI Agent Guide

Welcome to the **Disposable Service Workspace** repository. This document serves as the architectural overview and operational cheat-sheet for AI agents and human developers maintaining or extending this codebase.

---

## 1. Project Overview & Architecture

**Disposable Service Workspace** is an on-demand, privacy-focused ephemeral workspace tailored for sensitive operations (starting with Pandoc Markdown-to-PDF conversion) with strict zero data retention.

### High-Level Architecture
- **Control Plane (`crates/cli`):** Automated deployment engine built in Rust (`clap`, `ssh2`). Supports dual-mode execution: **Remote Mode** (connects to target servers via SSH/SFTP) and **Local Mode** (`--local`, runs directly on the server without SSH). Handles Docker & Docker Compose setup, transfers/writes compose specs, pulls images, and executes `docker compose up -d` / `teardown`.
- **Core Server (`crates/server`):** High-performance Rust backend built on `axum` and `tokio`:
  - **Auth System:** Single password configured on first access (`/api/auth/setup` with Argon2).
  - **Session Management:** Ephemeral JWT stored in `HttpOnly` cookies with a hardcoded 1-hour expiration.
  - **In-Memory Session Tracker:** Tracks active sessions by IP address and User-Agent (`/api/sessions`).
  - **Document Engine (`/api/apps/pandoc`):** Subprocess wrapper around `pandoc` + `pdflatex` with an RAII Drop guard (`TempDirGuard`) guaranteeing instant cleanup of all temporary files.
  - **Host Monitoring Service (`/api/apps/monitor`):** Real-time read-only host telemetry stream (CPU cores, RAM, swap, disks, top processes) via Server-Sent Events (`/api/apps/monitor/stream`).
  - **Metadata Scrubber (`/api/apps/scrubber`):** In-memory removal of GPS, device serials, author, thumbnails, XMP/IPTC from JPEG/PNG and Info dictionary/XMP from PDF (25 MB max). Returns a Scrub Report (categories/counts only). Clean file via `GET /api/apps/scrubber/download/:token` (Ephemeral Download, 5-min TTL, burn-after-reading). Errors: 401, 413, 415, 422.
  - **Secret Redactor (`POST /api/apps/redactor`):** Replaces JWTs, bearer tokens, API keys, `.env` values, emails, phones, IPs, credit cards, and custom words with `[REDACTED:<category>]` (5 MB max; idempotent; preserves line count).
  - **JWT Inspector (`POST /api/apps/jwt`):** Local decode of header/payload, claim time conversion, `alg: none` warning, optional HS256/384/512 verification and identity-claim redaction (8,192 chars max).
  - **Burn Workspace (`POST /api/burn`, `GET /api/burn/events`):** Emergency destruction (see Section 4.5).
  - **Static File Serving:** Serves compiled Astro frontend assets (`dist/`) directly on port 80.
- **Frontend (`frontend/`):** Built with **Astro 5** + **React 19** (CSR / React Islands) styled using **Tailwind CSS v4** (`@tailwindcss/vite`) adhering to the **Airbnb Inspired Design System** (Rausch `#ff385c`, clean white canvas, ink text `#222222`, 1-tier subtle shadow).
- **Deployment Packaging (`deploy/`):** Multi-stage `Dockerfile` and `docker-compose.yml`.

---

## 2. Directory Layout

```text
.
├── Cargo.toml                    # Root Cargo workspace configuration
├── Cargo.lock                    # Dependency lockfile
├── prd.md                        # Product Requirements Document
├── tasks.md / taks.md            # Implementation tasks and milestone roadmap
├── README.md                     # User and operational documentation
├── AGENTS.md                     # This agent/developer reference document
├── crates/
│   ├── common/                   # Shared DTOs, request/response models (disposable-common)
│   ├── server/                   # Axum backend web service & pandoc runner (disposable-server)
│   │   └── src/
│   │       ├── main.rs           # Server entrypoint & static asset fallback
│   │       ├── state.rs          # AppState (in-memory sessions & password hash)
│   │       ├── auth.rs           # Argon2 hashing, JWT verification, Axum extractor
│   │       └── routes/           # Auth and Pandoc API handlers
│   └── cli/                      # Deployment CLI control plane (disposable-cli)
│       └── src/
│           ├── main.rs           # CLI arguments & subcommand parser
│           ├── ssh.rs            # SSH2 & SFTP wrapper
│           └── deployer.rs       # Remote docker-compose orchestration
├── frontend/                     # Astro 5 + React 19 + Tailwind CSS v4
│   ├── astro.config.mjs          # Astro config with @tailwindcss/vite & React plugin
│   ├── package.json              # NPM dependencies and scripts
│   ├── src/
│   │   ├── styles/global.css     # Tailwind v4 theme & Airbnb tokens (@theme)
│   │   ├── components/           # React CSR components (Navbar, PandocConverter, etc.)
│   │   ├── layouts/Layout.astro  # Astro base HTML template
│   │   └── pages/                # Static routes (index, login, setup, pandoc)
└── deploy/
    ├── Dockerfile                # Multi-stage image build (Frontend + Rust + Pandoc/LaTeX)
    ├── docker-compose.yml        # Target server Compose deployment specification
    └── build-image.sh            # Local Docker image build script
```

---

## 3. Essential Commands & Workflows

### Rust Workspace
- Run all workspace tests:
  ```bash
  cargo test --workspace
  ```
- Build debug:
  ```bash
  cargo check --workspace
  ```
- Build release binaries:
  ```bash
  cargo build --release
  ```
  Binaries output to `target/release/disposable-cli` and `target/release/disposable-server`.

### Frontend
All frontend commands **must** be run from within the `frontend/` directory:
```bash
cd frontend
npm install
npm run build      # Builds output into frontend/dist/
npm run dev        # Starts Astro dev server on port 3000
```

### Local Integrated Testing
To run the server serving the compiled frontend:
```bash
# 1. Build frontend
(cd frontend && npm run build)

# 2. Run backend
cargo run -p disposable-server
```
Then navigate to `http://localhost:8080`.

---

## 4. Key Implementation Rules & Conventions

1. **Zero-Footprint Invariant:**
   - Any new tool or processing service added to `crates/server/src/routes/` MUST use an RAII guard (such as `TempDirGuard`) or explicit `defer` cleanup to delete all input/output files from disk immediately after transmission.
   - Do NOT introduce persistent databases (SQLite/Postgres/MySQL) to the core workspace container unless explicitly instructed.
2. **Authentication Flow:**
   - Authentication relies strictly on the Single Password model. Do not implement username/password registries.
   - Session tokens must remain in `HttpOnly; SameSite=Lax` cookies.
3. **Design System & Styling:**
   - Always use Tailwind CSS v4 utility classes and tokens defined in `frontend/src/styles/global.css`:
     - Primary brand: `text-rausch`, `bg-rausch`, `hover:bg-rausch-active`.
     - Text: `text-ink` (#222222), `text-muted` (#6a6a6a).
     - Surface: `bg-white`, `bg-surface-soft` (#f7f7f7), `bg-surface-strong` (#f2f2f2).
     - Shadows: `shadow-airbnb`, `shadow-airbnb-hover`, `shadow-airbnb-modal`.
     - Corner rounding: `rounded-sm` (8px), `rounded-md` (14px), `rounded-full` (9999px).
4. **CLI Remote Operations:**
   - The CLI embeds the compose file using `include_str!`. Keep `deploy/docker-compose.yml` self-contained and clean.
5. **Privacy Logging Rule:**
   - Never log file names, file contents, metadata values, redactor text, custom words, JWTs, payloads, or HMAC secrets (including error paths). Scrubber, Redactor, and JWT handlers must pass through the same auth extractor (401 when unauthenticated).

### 4.5 Privacy Micro-Apps & Burn Workspace

**API routes** (all registered in `create_api_router`, `crates/server/src/routes/mod.rs`):

| Method | Route | Purpose |
|--------|-------|---------|
| `POST` | `/api/apps/scrubber` | Scrub metadata from JPEG/PNG/PDF |
| `GET` | `/api/apps/scrubber/download/:token` | One-time scrubbed file download |
| `POST` | `/api/apps/redactor` | Redact secrets in text |
| `POST` | `/api/apps/jwt` | Inspect a JWT |
| `POST` | `/api/burn` | Trigger Burn Workspace, returns Audit Report |
| `GET` | `/api/burn/events` | SSE `burn` event carrying the Audit Report |

**Frontend pages:** `/scrubber` (Metadata Scrubber) and `/redactor` (tabs "JWT Inspector" and "Log & .env Redactor"), linked from Navbar and DashboardView. The Navbar shows the Burn button (confirmation dialog, or 3-second cancellable countdown in Demo mode), then an audit screen.

**MCP tools** (alongside `convert_markdown_to_pdf` and `get_host_metrics`):
- `redact_secrets`: same output as the redactor endpoint.
- `inspect_jwt`: same output as the JWT endpoint.
- `scrub_metadata`: base64 input, returns Ephemeral Download URL + Scrub Report.
- `burn_workspace_now` ("Burn Workspace Now"): requires `confirm: true` (boolean). Otherwise JSON-RPC `-32602`; a burn already running returns an in-progress error; failure returns `-32603` and the process keeps running.

**Burn Workspace behavior:**
- **Managed_Storage scope:** `sessions`, `downloads`, `mcp_sessions` in `AppState` plus the workspace temp directory. Download buffers are zeroed before being freed, MCP SSE connections are closed, the JWT cookie is cleared.
- **0-byte audit:** The Audit Report holds per-category entries/bytes before and after, duration (ms), and a UTC ISO 8601 timestamp, with no file names or user data. `verified: true` with `remaining_bytes: 0` is reported only after a successful re-verification (5 s limit); otherwise `verified: false`. Scope is Managed_Storage, not whole-process memory.
- **Concurrency:** Concurrent burns execute once and share one report; micro-app requests return `503` during/after burn. Failed item deletions are listed and mark the report incomplete.
- **Process exit:** After the report is sent the server exits with code `0` (2 s; 5 s if the client disconnects). A failed normal exit is forced with a non-zero code within 3 more seconds.
- **`RESTART_POLICY`:** `deploy/docker-compose.yml` uses `restart: ${RESTART_POLICY:-on-failure}`. With `no` or `on-failure` (default) the container stays stopped after a burn. With `unless-stopped` or `always`, Docker restarts it and the Audit Report carries a warning.
- The CLI `teardown` command must not call or depend on Burn Service.
- Every new feature needs at least one success-path and one failure-path test run by `cargo test --workspace`.

---

## 5. Docker Image — Build, Tag & Publish

### 5.1 Prerequisites
| Tool | Minimum Version | Purpose |
|------|----------------|---------|
| Docker | 24+ (BuildKit enabled) | Build & push image |
| Docker Buildx | bundled with Docker 24 | Multi-platform builds (optional) |
| Registry account | GitHub Container Registry (ghcr.io) or Docker Hub | Store & distribute images |

> **Context:** The `Dockerfile` uses a **3-stage build** — Node 22 (frontend) → Rust 1.85 (backend) → Debian Bookworm Slim (runtime). All `docker` / `docker buildx` commands **must be run from the repository root** (`/path/to/disposable-service/`), not from inside `deploy/`.

---

### 5.2 Configuration Points to Adjust

Before building, review and update the following values to match your registry and environment:

#### A. `deploy/docker-compose.yml` — Image Reference
```yaml
# Line 3 — change the registry/org/image name and tag
image: ${DISPOSABLE_IMAGE:-ghcr.io/<YOUR_ORG>/core-service:latest}
```
The `DISPOSABLE_IMAGE` environment variable lets the CLI or shell override the default at deploy-time without editing the file.

#### B. `deploy/build-image.sh` — Default Image Name
```bash
# Line 4 — default used when script is called without arguments
IMAGE_NAME="${1:-ghcr.io/<YOUR_ORG>/core-service:latest}"
```

#### C. `crates/cli/src/deployer.rs` — Embedded Compose & Pull Target
The CLI calls `docker compose pull` and `docker compose up -d` using the compose file embedded via `include_str!("../../../deploy/docker-compose.yml")`. Ensure the image name in the compose file already points to your registry so the remote server pulls from the correct location.

---

### 5.3 Build the Image Locally

**Option A — Using the helper script (recommended):**
```bash
# From repository root
chmod +x deploy/build-image.sh

# Build with default name
./deploy/build-image.sh

# Build with a custom name/tag
./deploy/build-image.sh ghcr.io/<YOUR_ORG>/core-service:v1.0.0
```

**Option B — Direct `docker build`:**
```bash
# From repository root — IMPORTANT: context must be `.` not `./deploy`
docker build \
  -t ghcr.io/<YOUR_ORG>/core-service:latest \
  -f deploy/Dockerfile \
  .
```

**Option C — Multi-platform (linux/amd64 + linux/arm64) via Buildx:**
```bash
docker buildx create --use --name multi-builder
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t ghcr.io/<YOUR_ORG>/core-service:latest \
  -f deploy/Dockerfile \
  --push \
  .
```
> `--push` uploads directly without a separate `docker push` step.

---

### 5.4 Authenticate & Push to Registry

#### GitHub Container Registry (ghcr.io)
```bash
# 1. Create a Personal Access Token with `write:packages` scope at:
#    https://github.com/settings/tokens

# 2. Authenticate Docker
echo "<YOUR_GITHUB_PAT>" | docker login ghcr.io -u <YOUR_GITHUB_USERNAME> --password-stdin

# 3. Push the image
docker push ghcr.io/<YOUR_ORG>/core-service:latest

# 4. (Optional) Also push a versioned tag
docker tag ghcr.io/<YOUR_ORG>/core-service:latest ghcr.io/<YOUR_ORG>/core-service:v1.0.0
docker push ghcr.io/<YOUR_ORG>/core-service:v1.0.0
```

#### Docker Hub
```bash
# 1. Authenticate
docker login -u <YOUR_DOCKERHUB_USERNAME>

# 2. Build with Docker Hub naming convention
docker build -t <YOUR_DOCKERHUB_USERNAME>/disposable-service:latest -f deploy/Dockerfile .

# 3. Push
docker push <YOUR_DOCKERHUB_USERNAME>/disposable-service:latest
```

---

### 5.5 Make the Image Publicly Accessible (ghcr.io)

By default, packages pushed to `ghcr.io` inherit the repository visibility. To make the image public so that the remote server can pull without credentials:

1. Go to `https://github.com/<YOUR_ORG>?tab=packages`.
2. Click on `core-service` → **Package settings**.
3. Under **Danger Zone**, set visibility to **Public**.

If kept private, configure Docker credentials on the target server:
```bash
# On the remote server before running `disposable-cli deploy`
echo "<PAT>" | docker login ghcr.io -u <USER> --password-stdin
```

---

### 5.6 Full End-to-End Release Workflow

```bash
# 1. Set your registry image name
export IMAGE="ghcr.io/<YOUR_ORG>/core-service:v1.0.0"

# 2. Build
./deploy/build-image.sh "$IMAGE"

# 3. Push
docker push "$IMAGE"

# 4. Deploy to existing server (CLI will pull & restart)
cargo run -p disposable-cli -- deploy \
  --host <SERVER_IP> \
  --user <SSH_USER> \
  --key ~/.ssh/id_ed25519 \
  --image "$IMAGE"
```

The `disposable-cli deploy` command will:
1. SSH into the target server.
2. Verify Docker and Docker Compose runtime. If absent on a blank server, it automatically prompts to install them (or uses `--install-docker` / `-i` flag).
3. Copy `docker-compose.yml` via SFTP.
4. Run `DISPOSABLE_IMAGE=$IMAGE docker compose pull`.
5. Run `docker compose up -d --remove-orphans`.

You can also prepare a blank server in advance without deploying the app yet:
```bash
cargo run -p disposable-cli -- install-docker \
  --host <SERVER_IP> \
  --user <SSH_USER> \
  --key ~/.ssh/id_ed25519
```

#### Direct Local Server Deployment (`--local`)
If cloning directly on the target server, no SSH connection is needed:
```bash
# 1. Clone repository on the server
git clone https://github.com/<YOUR_ORG>/disposable-service.git
cd disposable-service

# 2. Deploy locally (auto-installs Docker if needed and launches workspace)
cargo run -p disposable-cli -- deploy --local

# 3. Teardown locally when done
cargo run -p disposable-cli -- teardown --local
```



---

### 5.7 Environment Variables Reference (Runtime)

These are set in `docker-compose.yml` and can be overridden at deploy-time:

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `80` | HTTP listening port inside the container |
| `HOST` | `0.0.0.0` | Bind address |
| `STATIC_DIR` | `/app/frontend/dist` | Path to compiled Astro assets |
| `PANDOC_BIN` | `pandoc` | Path/name of the Pandoc binary |
| `PANDOC_PDF_ENGINE` | `pdflatex` | LaTeX engine used by Pandoc for PDF output |
| `DISPOSABLE_IMAGE` | _(from compose)_ | Override image reference at pull-time |
| `RESTART_POLICY` | `on-failure` | Docker restart policy; `unless-stopped`/`always` restart the container after a burn |

---

## 6. Upcoming Milestones (Post-MVP / Phase 8)
- **AWS EC2 Mode (`--new`):**
  - Implement dynamic VM creation using `aws-sdk-ec2`.
  - Automatically create temporary Security Groups (ports 22 and 80).
  - Wait for Public IP and SSH availability, then hand over to `deploy_existing`.
- **Automated Teardown (TTL):**
  - Optional built-in timer to automatically trigger `docker compose down` after a fixed duration (e.g. 2 hours).
