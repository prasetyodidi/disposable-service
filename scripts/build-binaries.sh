#!/usr/bin/env bash
set -e

# ==============================================================================
# Disposable Service Workspace — Multi-Platform Binary Builder
# Produces release binaries for both macOS and Linux into the `bin/` directory.
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
OUTPUT_DIR="${PROJECT_ROOT}/bin"

mkdir -p "${OUTPUT_DIR}"

echo "=========================================================="
echo " 🔨 Building Disposable CLI for macOS & Linux"
echo "=========================================================="

# 1. Build Native macOS Binary
echo -e "\n[1/2] Compiling native macOS binary..."
cargo build --release --bin disposable-cli
cp "${PROJECT_ROOT}/target/release/disposable-cli" "${OUTPUT_DIR}/disposable-cli-macos"
chmod +x "${OUTPUT_DIR}/disposable-cli-macos"
echo "✓ macOS binary ready: bin/disposable-cli-macos"

# 2. Build Linux (x86_64) Binary via Docker
echo -e "\n[2/2] Compiling Linux (x86_64 / amd64) binary via Docker..."
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    docker run --rm \
        --platform linux/amd64 \
        -v "${PROJECT_ROOT}":/app \
        -w /app \
        rust:1-slim-bookworm \
        sh -c "apt-get update -qq && apt-get install -y -qq pkg-config libssl-dev >/dev/null 2>&1 && cargo build --release --bin disposable-cli --target-dir /tmp/target && cp /tmp/target/release/disposable-cli /app/bin/disposable-cli-linux-amd64"
    chmod +x "${OUTPUT_DIR}/disposable-cli-linux-amd64"
    echo "✓ Linux binary ready: bin/disposable-cli-linux-amd64"
else
    echo "⚠️  Docker is not running locally. Cannot cross-compile Linux binary."
    echo "   Please start Docker Desktop and run this script again."
    exit 1
fi

echo -e "\n=========================================================="
echo " 🎉 Build Complete! Binaries saved in: ${OUTPUT_DIR}"
echo "=========================================================="
ls -lh "${OUTPUT_DIR}"
