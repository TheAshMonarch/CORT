use std::env;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use cort_backend::{build_router, db::init_db};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "cort_backend=info,tower_http=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:cort.db?mode=rwc".to_string());
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    info!("Initializing Cort database at {database_url}...");
    let pool = init_db(&database_url).await?;

    let app = build_router(pool);

    let listener = TcpListener::bind(&bind_addr).await?;
    info!("Cort Backend (Host Hotspot + Cloud Sync API) listening on http://{bind_addr}");

    axum::serve(listener, app).await?;
    Ok(())
}
