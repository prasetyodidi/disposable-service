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

### 3. Deploying to an Existing Server via CLI

The CLI connects to your target Linux server via SSH, ensures Docker is ready, uploads `docker-compose.yml`, pulls the image, and launches the service.

```bash
# Compile CLI binary
cargo build --release --bin disposable-cli

# Deploy to existing target host
./target/release/disposable-cli deploy --existing \
  --host 192.168.1.100 \
  --port 22 \
  --user root \
  --key ~/.ssh/id_ed25519
```

Once deployment completes, the CLI will output the direct access URL:
```text
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🚀 Disposable Service Workspace is LIVE!
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Access URL:  http://192.168.1.100:80
  First Step:  Open the URL in your browser to claim and set your Single Password.
  Retention:   Strict Zero-Footprint ephemeral storage.
```

---

### 4. Teardown and Cleanup (Zero Trace)

When you are finished with the workspace, tear down all containers and remove files:

#### Option A: Via CLI
```bash
./target/release/disposable-cli teardown \
  --host 192.168.1.100 \
  --user root \
  --key ~/.ssh/id_ed25519
```

#### Option B: Manual via SSH
```bash
ssh root@192.168.1.100 "cd /opt/disposable-service && docker compose down -v && rm -rf /opt/disposable-service"
```

---

## 🎨 Airbnb Inspired Design Tokens

Custom design tokens are integrated using Tailwind CSS v4 in `frontend/src/styles/global.css`:

* **Brand Voltage (Rausch):** `#ff385c` (Active: `#e00b41`, Disabled: `#ffd1da`)
* **Surfaces:** Pure canvas `#ffffff`, Soft floor `#f7f7f7`, Strong `#f2f2f2`
* **Typography:** `#222222` (Ink), `#6a6a6a` (Muted), Inter font family
* **Elevation Shadow:** `rgba(0, 0, 0, 0.02) 0 0 0 1px, rgba(0, 0, 0, 0.04) 0 2px 6px, rgba(0, 0, 0, 0.1) 0 4px 8px`
* **Border Radii:** `8px` (inputs & buttons), `14px` (cards & modals), `9999px` (pills & badges)
