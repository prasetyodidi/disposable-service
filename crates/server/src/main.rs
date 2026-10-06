mod auth;
mod routes;
mod state;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::middleware::map_request;
use axum::Router;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::routes::create_api_router;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,disposable_server=debug,tower_http=info".into()),
        )
        .init();

    let state = AppState::new();

    // CORS configuration for local development
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::COOKIE]);

    let api_router = create_api_router().layer(cors);

    // Static asset directory (Astro build output)
    let static_dir = std::env::var("STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let possible_paths = [
                PathBuf::from("frontend/dist"),
                PathBuf::from("../frontend/dist"),
                PathBuf::from("/app/frontend/dist"),
                PathBuf::from("/var/www/html"),
            ];
            for p in &possible_paths {
                if p.exists() {
                    return p.clone();
                }
            }
            PathBuf::from("frontend/dist")
        });

    info!("Using static assets directory: {:?}", static_dir);

    let app = if static_dir.exists() {
        let index_file = static_dir.join("index.html");

        // Astro dibuild dengan `build.format = "file"`, sehingga halaman dihasilkan
        // sebagai file datar (`setup.html`, `login.html`, `pandoc.html`), sementara
        // frontend bernavigasi ke clean URL (`/setup`, `/login`, `/pandoc`).
        //
        // ServeDir tidak tahu pemetaan clean URL → file `.html`: request `/setup`
        // tidak menemukan file bernama `setup`, lalu jatuh ke fallback `index.html`
        // (dashboard) dengan status 200. Akibatnya AppRouter me-render
        // `page="dashboard"` padahal pathname `/setup`, dan form setup tidak tampil.
        //
        // Middleware berikut meniru `try_files $uri.html $uri/index.html` ala nginx:
        // URI clean URL di-rewrite ke file `.html`/`index.html` yang benar SEBELUM
        // diserahkan ke ServeDir. Pathname di browser tetap `/setup` (cocok dengan
        // cek `window.location.pathname === '/setup'` di AppRouter) dan status tetap 200.
        let rewrite_dir = static_dir.clone();
        let spa_rewrite = move |mut req: Request<Body>| {
            let dir = rewrite_dir.clone();
            async move {
                if req.method() == Method::GET || req.method() == Method::HEAD {
                    let uri = req.uri().clone();
                    let path = uri.path().to_string();
                    let last_segment = path.rsplit('/').next().unwrap_or("");
                    let is_api = path == "/api" || path.starts_with("/api/");
                    let looks_like_asset = last_segment.contains('.');

                    if !is_api && !looks_like_asset {
                        let rel = path.trim_start_matches('/');
                        // Tolak path traversal dan jangan rewrite root.
                        if !rel.is_empty() && !rel.split('/').any(|seg| seg == "..") {
                            let html_file = dir.join(format!("{rel}.html"));
                            let index_html = dir.join(rel).join("index.html");
                            let target = if html_file.is_file() {
                                Some(format!("{rel}.html"))
                            } else if index_html.is_file() {
                                Some(format!("{rel}/index.html"))
                            } else {
                                None
                            };

                            if let Some(target) = target {
                                let rewritten = match uri.query() {
                                    Some(q) => format!("/{target}?{q}"),
                                    None => format!("/{target}"),
                                };
                                if let Ok(new_uri) = rewritten.parse::<axum::http::Uri>() {
                                    *req.uri_mut() = new_uri;
                                }
                            }
                        }
                    }
                }
                req
            }
        };

        let serve_dir = ServeDir::new(&static_dir).not_found_service(ServeFile::new(index_file));
        Router::new()
            .merge(api_router)
            .fallback_service(serve_dir)
            .layer(map_request(spa_rewrite))
            .with_state(state)
            .layer(TraceLayer::new_for_http())
    } else {
        info!("Static directory not found yet; mounting fallback API root.");
        Router::new()
            .merge(api_router)
            .fallback(|| async {
                (
                    StatusCode::OK,
                    "Disposable Service Workspace API is running. (Frontend dist not built yet)",
                )
            })
            .with_state(state)
            .layer(TraceLayer::new_for_http())
    };

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(80);

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let addr_str = format!("{}:{}", host, port);

    let listener = match tokio::net::TcpListener::bind(&addr_str).await {
        Ok(l) => {
            info!("Listening on http://{}", addr_str);
            l
        }
        Err(e) if port == 80 => {
            info!(
                "Binding to port 80 failed ({}). Falling back to port 8080...",
                e
            );
            let fallback_addr = format!("{}:8080", host);
            let l = tokio::net::TcpListener::bind(&fallback_addr).await?;
            info!("Listening on fallback http://{}", fallback_addr);
            l
        }
        Err(e) => return Err(e.into()),
    };

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    info!("Shutting down gracefully...");
}
