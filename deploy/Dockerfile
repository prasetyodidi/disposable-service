# ==========================================
# Stage 1: Frontend Build (Astro + Tailwind v4 + React)
# ==========================================
FROM node:22-alpine AS frontend-builder
WORKDIR /app/frontend

COPY frontend/package*.json ./
RUN npm install

COPY frontend/ ./
RUN npm run build

# ==========================================
# Stage 2: Rust Server Build
# ==========================================
FROM rust:1.95-slim-bookworm AS backend-builder
WORKDIR /app

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock* ./
COPY crates/ ./crates/

RUN cargo build --release --bin disposable-server

# ==========================================
# Stage 3: Minimal Ephemeral Runtime
# ==========================================
FROM debian:bookworm-slim AS runtime

# Install Pandoc and pdflatex (texlive-latex-base)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    pandoc \
    texlive-latex-base \
    texlive-fonts-recommended \
    texlive-latex-recommended \
    lmodern \
    poppler-utils \
    && rm -rf /var/lib/apt/lists/*

# Fail the build if pdftohtml is missing (poppler's -v exits with 99)
RUN pdftohtml -v || [ $? -eq 99 ]

WORKDIR /app

# Copy binary and static frontend
COPY --from=backend-builder /app/target/release/disposable-server /usr/local/bin/disposable-server
COPY --from=frontend-builder /app/frontend/dist /app/frontend/dist

ENV PORT=80
ENV HOST=0.0.0.0
ENV STATIC_DIR=/app/frontend/dist
ENV PANDOC_BIN=pandoc
ENV PANDOC_PDF_ENGINE=pdflatex
ENV PDF_MD_ENGINE=poppler

EXPOSE 80

CMD ["/usr/local/bin/disposable-server"]
