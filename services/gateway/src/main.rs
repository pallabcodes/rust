use anyhow::Result;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use corelib::math;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::signal;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let app = Router::new()
        .route("/health", get(health))
        .route("/sum", post(sum));

    let addr: SocketAddr = "0.0.0.0:3000".parse()?;
    info!(%addr, "starting gateway");

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Deserialize)]
struct SumRequest {
    a: i64,
    b: i64,
}

#[derive(Serialize)]
struct SumResponse {
    total: i64,
}

async fn sum(Json(payload): Json<SumRequest>) -> Result<Json<SumResponse>, StatusCode> {
    match math::checked_add(payload.a, payload.b) {
        Ok(total) => Ok(Json(SumResponse { total })),
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

async fn shutdown_signal() {
    let _ = signal::ctrl_c().await;
    info!("shutdown signal received");
}

fn init_tracing() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

