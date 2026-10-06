#!/usr/bin/env bash
set -euo pipefail

IMAGE_NAME="${1:-disposable-workspace:latest}"

echo "==> Building Disposable Workspace Multi-Stage Docker Image: $IMAGE_NAME..."
docker build -t "$IMAGE_NAME" -f deploy/Dockerfile .

echo "==> Successfully built $IMAGE_NAME"
