use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use serde::Deserialize;

use crate::App;
use crate::ip;

/// dyndns2-style update endpoint the router calls (Zyxel "DNS user defined").
/// The hostname parameter is deliberately ignored: any authenticated call
/// means "the public IP may have changed", and the database decides which
/// records to update.
#[derive(Debug, Deserialize)]
pub struct NicUpdateQuery {
    hostname: Option<String>,
    myip: Option<String>,
    // Routers that can't do Basic auth sometimes pass credentials in the URL.
    user: Option<String>,
    username: Option<String>,
    pass: Option<String>,
    password: Option<String>,
}

/// Credentials from the Authorization header, or from query parameters as a
/// fallback. Returns (username, password, how-they-arrived).
fn credentials(
    headers: &HeaderMap,
    query: &NicUpdateQuery,
) -> Option<(String, String, &'static str)> {
    if let Some(value) = headers.get(axum::http::header::AUTHORIZATION) {
        let encoded = value.to_str().ok()?.strip_prefix("Basic ")?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        let text = String::from_utf8(decoded).ok()?;
        let (user, pass) = text.split_once(':')?;
        return Some((user.to_string(), pass.to_string(), "basic-auth"));
    }

    let user = query.user.as_ref().or(query.username.as_ref());
    let pass = query.pass.as_ref().or(query.password.as_ref());
    if let (Some(user), Some(pass)) = (user, pass) {
        return Some((user.clone(), pass.clone(), "query-params"));
    }
    None
}

pub async fn nic_update(
    State(app): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<NicUpdateQuery>,
) -> Response {
    tracing::info!(
        %peer,
        hostname = query.hostname.as_deref().unwrap_or("-"),
        myip = query.myip.as_deref().unwrap_or("-"),
        has_authorization_header = headers.contains_key(axum::http::header::AUTHORIZATION),
        "webhook called"
    );

    // dyndns2 status words in the body, but with honest HTTP status codes on
    // top: the Zyxel only inspects the status code (a 200 "911" body shows as
    // "Accepted" in its UI), and dyndns2 clients that parse bodies still can.
    let Some((user, pass, via)) = credentials(&headers, &query) else {
        tracing::warn!(%peer, "webhook rejected: no credentials (neither Basic auth nor user/pass query params)");
        return (StatusCode::UNAUTHORIZED, "badauth").into_response();
    };
    if user != app.webhook_username || pass != app.webhook_password {
        let reason = if user != app.webhook_username {
            "unknown username"
        } else {
            "wrong password"
        };
        tracing::warn!(%peer, username = %user, via, reason, "webhook rejected");
        return (StatusCode::UNAUTHORIZED, "badauth").into_response();
    }
    tracing::info!(%peer, username = %user, via, "webhook authenticated");

    let myip = query.myip.as_deref().and_then(ip::parse_myip);
    if query.myip.is_some() && myip.is_none() {
        tracing::warn!(myip = ?query.myip, "ignoring myip parameter without a public IPv4");
    }

    let Some(outcome) = app.syncer.sync_now("webhook", myip).await else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "911").into_response();
    };
    if !outcome.ok() {
        tracing::warn!(
            error = outcome.error.as_deref().unwrap_or("-"),
            failed = outcome.failed,
            "webhook sync had errors, replying 911"
        );
        return (StatusCode::INTERNAL_SERVER_ERROR, "911").into_response();
    }

    let ip = outcome
        .ipv4
        .clone()
        .unwrap_or_default();
    let status = if outcome.changed() { "good" } else { "nochg" };
    let body = format!("{status} {ip}").trim_end().to_string();
    tracing::info!(%peer, response = %body, "webhook done");
    body.into_response()
}
