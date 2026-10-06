# AGENTS.md — Developer & AI Agent Guide

Welcome to the **Disposable Service Workspace** repository. This document serves as the architectural overview and operational cheat-sheet for AI agents and human developers maintaining or extending this codebase.

---

## 1. Project Overview & Architecture

**Disposable Service Workspace** is an on-demand, privacy-focused ephemeral workspace tailored for sensitive operations (starting with Pandoc Markdown-to-PDF conversion) with strict zero data retention.

### High-Level Architecture
- **Control Plane (`crates/cli`):** Automated deployment engine built in Rust (`clap`, `ssh2`). Connects to Linux target servers via SSH, transfers compose specs, pulls images, and executes `docker compose up -d` / `teardown`.
- **Core Server (`crates/server`):** High-performance Rust backend built on `axum` and `tokio`:
  - **Auth System:** Single password configured on first access (`/api/auth/setup` with Argon2).
  - **Session Management:** Ephemeral JWT stored in `HttpOnly` cookies with a hardcoded 1-hour expiration.
  - **In-Memory Session Tracker:** Tracks active sessions by IP address and User-Agent (`/api/sessions`).
  - **Document Engine (`/api/apps/pandoc`):** Subprocess wrapper around `pandoc` + `pdflatex` with an RAII Drop guard (`TempDirGuard`) guaranteeing instant cleanup of all temporary files.
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

---

## 5. Upcoming Milestones (Post-MVP / Phase 8)
- **AWS EC2 Mode (`--new`):**
  - Implement dynamic VM creation using `aws-sdk-ec2`.
  - Automatically create temporary Security Groups (ports 22 and 80).
  - Wait for Public IP and SSH availability, then hand over to `deploy_existing`.
- **Automated Teardown (TTL):**
  - Optional built-in timer to automatically trigger `docker compose down` after a fixed duration (e.g. 2 hours).
