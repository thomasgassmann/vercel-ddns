mod api;
mod assets;
mod auth;
mod ip;
mod storage;
mod sync;
mod vercel;
mod webhook;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post, put};
use axum_extra::extract::cookie::Key;
use clap::Parser;

use crate::storage::Storage;
use crate::sync::Syncer;
use crate::vercel::Vercel;

#[derive(Parser, Debug)]
#[command(name = "ddnser", about = "Dynamic DNS daemon for Vercel-managed domains")]
pub struct Args {
    #[arg(long, env = "DDNSER_PORT", default_value_t = 8080)]
    pub port: u16,

    #[arg(long, env = "DDNSER_LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    #[arg(long, env = "DDNSER_DATABASE_URL")]
    pub database_url: String,

    #[arg(long, env = "VERCEL_TOKEN")]
    pub vercel_token: String,

    /// Basic auth credentials the router must present on /nic/update.
    #[arg(long, env = "DDNSER_WEBHOOK_USERNAME")]
    pub webhook_username: String,

    #[arg(long, env = "DDNSER_WEBHOOK_PASSWORD")]
    pub webhook_password: String,

    /// Periodic full-sync interval in seconds.
    #[arg(long, env = "DDNSER_SYNC_INTERVAL", default_value_t = 3600)]
    pub sync_interval: u64,

    #[arg(long, env = "DDNSER_OIDC_ISSUER")]
    pub oidc_issuer: String,

    #[arg(long, env = "DDNSER_OIDC_CLIENT_ID")]
    pub oidc_client_id: String,

    #[arg(long, env = "DDNSER_OIDC_CLIENT_SECRET")]
    pub oidc_client_secret: String,

    /// Externally reachable base URL of this service (for the OIDC redirect).
    #[arg(long, env = "DDNSER_PUBLIC_URL", default_value = "http://localhost:8080")]
    pub public_url: String,

    /// ID-token claim inspected for admin authorization.
    #[arg(long, env = "DDNSER_ADMIN_CLAIM", default_value = "groups")]
    pub admin_claim: String,

    /// Required value within the admin claim; unset = any authenticated
    /// identity is an admin (gate access at the IdP instead).
    #[arg(long, env = "DDNSER_ADMIN_GROUP")]
    pub admin_group: Option<String>,

    /// Secret the session cookies are encrypted with.
    #[arg(long, env = "DDNSER_SESSION_SECRET")]
    pub session_secret: String,

    /// When set, proxy frontend requests to this URL instead of serving embedded assets.
    /// Enables vite dev server with HMR during local development.
    #[arg(long, env = "DDNSER_DEV_FORWARD")]
    pub dev_forward: Option<String>,
}

pub struct AppState {
    pub storage: Arc<Storage>,
    pub vercel: Arc<Vercel>,
    pub syncer: Syncer,
    pub oidc: auth::Oidc,
    pub key: Key,
    pub admin_claim: String,
    pub admin_group: Option<String>,
    pub cookie_secure: bool,
    pub webhook_username: String,
    pub webhook_password: String,
    pub sync_interval: u64,
}

#[derive(Clone)]
pub struct App(pub Arc<AppState>);

impl std::ops::Deref for App {
    type Target = AppState;
    fn deref(&self) -> &AppState {
        &self.0
    }
}

impl axum::extract::FromRef<App> for Key {
    fn from_ref(app: &App) -> Key {
        app.key.clone()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| args.log_level.clone().into()),
        )
        .init();

    let storage = Arc::new(Storage::connect(&args.database_url).await?);
    let vercel = Arc::new(Vercel::new(args.vercel_token.clone()));
    let syncer = Syncer::spawn(storage.clone(), vercel.clone(), args.sync_interval);
    let oidc = auth::Oidc::discover(
        &args.oidc_issuer,
        &args.oidc_client_id,
        &args.oidc_client_secret,
        &args.public_url,
    )
    .await?;

    let state = App(Arc::new(AppState {
        storage,
        vercel,
        syncer,
        oidc,
        key: Key::derive_from(args.session_secret.as_bytes()),
        admin_claim: args.admin_claim,
        admin_group: args.admin_group,
        cookie_secure: args.public_url.starts_with("https://"),
        webhook_username: args.webhook_username,
        webhook_password: args.webhook_password,
        sync_interval: args.sync_interval,
    }));

    let api = Router::new()
        .route("/me", get(api::me))
        .route("/entries", get(api::list_entries).post(api::create_entry))
        .route("/entries/{id}", put(api::update_entry).delete(api::delete_entry))
        .route("/sync", post(api::sync_now))
        .route("/status", get(api::status))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_session,
        ));

    let app = Router::new()
        .route("/nic/update", get(webhook::nic_update))
        .route("/auth/login", get(auth::login))
        .route("/auth/callback", get(auth::callback))
        .route("/auth/logout", post(auth::logout))
        .nest("/api", api)
        .with_state(state);

    let app = match args.dev_forward.filter(|url| !url.is_empty()) {
        Some(url) => {
            tracing::info!(%url, "dev-forward enabled, proxying frontend requests");
            app.fallback_service(axum_reverse_proxy::ReverseProxy::new("/", &url))
        }
        None => app.fallback(get(assets::serve)),
    };

    let addr = format!("0.0.0.0:{}", args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "ddnser listening");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}
