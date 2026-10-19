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

## 🎨 Airbnb Inspired Design Tokens

Custom design tokens are integrated using Tailwind CSS v4 in `frontend/src/styles/global.css`:

* **Brand Voltage (Rausch):** `#ff385c` (Active: `#e00b41`, Disabled: `#ffd1da`)
* **Surfaces:** Pure canvas `#ffffff`, Soft floor `#f7f7f7`, Strong `#f2f2f2`
* **Typography:** `#222222` (Ink), `#6a6a6a` (Muted), Inter font family
* **Elevation Shadow:** `rgba(0, 0, 0, 0.02) 0 0 0 1px, rgba(0, 0, 0, 0.04) 0 2px 6px, rgba(0, 0, 0, 0.1) 0 4px 8px`
* **Border Radii:** `8px` (inputs & buttons), `14px` (cards & modals), `9999px` (pills & badges)
