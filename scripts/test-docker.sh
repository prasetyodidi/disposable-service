#!/usr/bin/env bash
# Run the workspace tests inside a Docker container that has poppler-utils,
# Pandoc and Typst installed (deploy/Dockerfile.test).
#
# Usage:
#   scripts/test-docker.sh [CARGO_TEST_ARGS...]   run `cargo test --workspace [args]`
#   scripts/test-docker.sh pdf2md                 only tests whose name matches "pdf2md"
#   scripts/test-docker.sh --shell                interactive bash in the same container
#   scripts/test-docker.sh --help                 show this help
#
# Environment:
#   DOCKER_PLATFORM   optional, e.g. linux/amd64. Passed to `docker build --platform`
#                     and `docker run --platform` only when set (default: native arch).
#   TEST_CACHE_DIR    optional absolute path. When set, $TEST_CACHE_DIR/cargo-registry and
#                     $TEST_CACHE_DIR/cargo-target are created and bind-mounted instead of the
#                     named volumes (lets actions/cache save and restore them in CI).
#
# Notes:
#   - The repository is bind-mounted read-write at /app.
#   - Named volumes `disposable-cargo-registry` (cargo registry) and
#     `disposable-cargo-target` (CARGO_TARGET_DIR=/cargo-target) keep dependencies
#     and build output between runs. A bind-mounted target/ is very slow on macOS.
#   - The first run builds the image and compiles everything; it takes several minutes.
#   - The exit code of `cargo test` is passed through unchanged.
set -euo pipefail

IMAGE="disposable-test"
REGISTRY_VOLUME="disposable-cargo-registry"
TARGET_VOLUME="disposable-cargo-target"

usage() {
  sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
}

SHELL_MODE=0
case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
  --shell)
    SHELL_MODE=1
    shift
    ;;
esac

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker is not installed or not in PATH" >&2
  exit 1
fi

PLATFORM_ARGS=()
if [ -n "${DOCKER_PLATFORM:-}" ]; then
  PLATFORM_ARGS=(--platform "$DOCKER_PLATFORM")
fi

# Dockerfile.test copies no application code, so the build context is limited to
# deploy/ to avoid sending target/, node_modules/ and .git/ to the daemon.
echo ">> Building image $IMAGE (first build takes several minutes)..." >&2
docker build ${PLATFORM_ARGS[@]+"${PLATFORM_ARGS[@]}"} \
  -f deploy/Dockerfile.test -t "$IMAGE" deploy

# First step inside the container: verify the required external tools.
# pdftohtml -v and pdfinfo -v print to stderr and may exit with 0 or 99.
read -r -d '' CHECK_TOOLS <<'EOF' || true
missing=""
check() {
  name="$1"; shift
  "$@" >/dev/null 2>&1
  rc=$?
  if [ "$rc" -ne 0 ] && { [ "$allow99" != "1" ] || [ "$rc" -ne 99 ]; }; then
    missing="$missing $name"
  fi
}
allow99=1; check pdftohtml pdftohtml -v
allow99=1; check pdfinfo pdfinfo -v
allow99=0; check pandoc pandoc --version
allow99=0; check typst typst --version
if [ -n "$missing" ]; then
  echo "error: required tool(s) missing or not working in the test image:$missing" >&2
  exit 127
fi
EOF

REGISTRY_MOUNT="$REGISTRY_VOLUME"
TARGET_MOUNT="$TARGET_VOLUME"
if [ -n "${TEST_CACHE_DIR:-}" ]; then
  case "$TEST_CACHE_DIR" in
    /*) ;;
    *)
      echo "error: TEST_CACHE_DIR must be an absolute path (got: $TEST_CACHE_DIR)" >&2
      exit 1
      ;;
  esac
  mkdir -p "$TEST_CACHE_DIR/cargo-registry" "$TEST_CACHE_DIR/cargo-target"
  REGISTRY_MOUNT="$TEST_CACHE_DIR/cargo-registry"
  TARGET_MOUNT="$TEST_CACHE_DIR/cargo-target"
fi

RUN_ARGS=(
  --rm
  -v "$REPO_ROOT:/app"
  -v "$REGISTRY_MOUNT:/usr/local/cargo/registry"
  -v "$TARGET_MOUNT:/cargo-target"
  -e CARGO_TARGET_DIR=/cargo-target
  -w /app
)

if [ "$SHELL_MODE" = "1" ]; then
  RUN_ARGS+=(-it)
  CMD='exec bash'
else
  CMD='cargo test --workspace "$@"'
fi

# Argument 0 of bash -c is "bash"; user arguments follow as "$@".
# `exec docker` makes the container exit code the script exit code.
exec docker run ${PLATFORM_ARGS[@]+"${PLATFORM_ARGS[@]}"} "${RUN_ARGS[@]}" "$IMAGE" \
  bash -c "$CHECK_TOOLS"$'\n'"$CMD" bash "$@"
