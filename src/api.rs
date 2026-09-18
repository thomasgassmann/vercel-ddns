use std::net::{Ipv4Addr, Ipv6Addr};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Serialize;

use crate::App;
use crate::auth::Session;
use crate::storage::{Record, RecordInput, SyncRun};
use crate::sync::{self, SyncOutcome};

pub async fn me(Extension(session): Extension<Session>) -> Json<Session> {
    Json(session)
}

fn database_error(error: sqlx::Error) -> Response {
    if let sqlx::Error::Database(db) = &error {
        if db.is_unique_violation() {
            return (StatusCode::CONFLICT, "record or provider ID already exists").into_response();
        }
        if db.is_check_violation() {
            return (StatusCode::UNPROCESSABLE_ENTITY, "invalid record").into_response();
        }
    }
    tracing::error!("database operation failed");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

fn invalid(message: &str) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, message.to_string()).into_response()
}

fn valid_name(name: &str, owner: bool) -> bool {
    let labels: Vec<_> = name.split('.').collect();
    labels.len() >= 2
        && name.len() <= 253
        && labels.iter().enumerate().all(|(i, label)| {
            if owner && i == 0 && *label == "*" {
                return true;
            }
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|c| {
                    c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || c == b'-'
                        || (owner && c == b'_')
                })
        })
}

pub(crate) fn validate(mut input: RecordInput) -> Result<RecordInput, &'static str> {
    input.fqdn = input.fqdn.trim().trim_end_matches('.').to_ascii_lowercase();
    input.record_type = input.record_type.to_ascii_uppercase();
    if !valid_name(&input.fqdn, true) {
        return Err("invalid FQDN");
    }
    if input
        .fqdn
        .split('.')
        .any(|label| label == "_acme-challenge")
    {
        return Err("ACME challenge records belong to cert-manager");
    }
    if input.ttl != 1 && !(60..=86400).contains(&input.ttl) {
        return Err("TTL must be 1 (automatic) or 60..86400");
    }
    let kind = input.record_type.as_str();
    if !matches!(kind, "A" | "AAAA" | "CAA" | "CNAME" | "MX" | "TXT" | "SRV") {
        return Err("unsupported record type");
    }
    if matches!(kind, "MX" | "SRV") {
        if !input.priority.is_some_and(|p| (0..=65535).contains(&p)) {
            return Err("priority must be 0..65535");
        }
    } else if input.priority.is_some() {
        return Err("priority only applies to MX and SRV");
    }
    if kind == "SRV" {
        if !input.port.is_some_and(|p| (0..=65535).contains(&p))
            || !input.weight.is_some_and(|p| (0..=65535).contains(&p))
        {
            return Err("SRV requires port and weight in 0..65535");
        }
    } else if input.port.is_some() || input.weight.is_some() {
        return Err("port and weight only apply to SRV");
    }
    if let Some(value) = &mut input.value {
        *value = match kind {
            "A" => value
                .trim()
                .parse::<Ipv4Addr>()
                .map_err(|_| "invalid IPv4")?
                .to_string(),
            "AAAA" => value
                .trim()
                .parse::<Ipv6Addr>()
                .map_err(|_| "invalid IPv6")?
                .to_string(),
            "MX" | "CNAME" | "SRV" => {
                let target = value.trim().trim_end_matches('.').to_ascii_lowercase();
                if (kind == "SRV" || (kind == "MX" && input.priority == Some(0)))
                    && value.trim() == "."
                {
                    ".".into()
                } else {
                    if !valid_name(&target, false) {
                        return Err("invalid target hostname");
                    }
                    target
                }
            }
            "CAA" => normalize_caa(value)?,
            "TXT" => {
                if value.len() > 2048 || value.contains(['\r', '\n', '\0']) {
                    return Err("invalid TXT value");
                }
                value.clone()
            }
            _ => unreachable!("record types are validated above"),
        };
    } else if !matches!(kind, "A" | "AAAA") {
        return Err("only A and AAAA can be dynamic");
    }
    Ok(input)
}

fn normalize_caa(value: &str) -> Result<String, &'static str> {
    let mut parts = value.splitn(3, char::is_whitespace);
    let flags = parts.next().unwrap_or_default();
    let tag = parts.next().unwrap_or_default();
    let value = parts.next().unwrap_or_default();
    if !flags.bytes().all(|c| c.is_ascii_digit()) || flags.parse::<u8>().is_err() {
        return Err("CAA flags must be 0..255");
    }
    if tag.is_empty() || tag.len() > 15 || !tag.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return Err("CAA tag must be 1 to 15 alphanumeric characters");
    }
    if value.is_empty() || value.contains(['\r', '\n', '\0']) {
        return Err("invalid CAA value");
    }
    Ok(format!("{flags} {tag} {value}"))
}

pub async fn list_records(State(app): State<App>) -> Result<Json<Vec<Record>>, Response> {
    app.storage.list().await.map(Json).map_err(database_error)
}

pub async fn create_record(
    State(app): State<App>,
    Json(input): Json<RecordInput>,
) -> Result<(StatusCode, Json<Record>), Response> {
    let input = validate(input).map_err(invalid)?;
    let record = {
        let _guard = app.syncer.mutation_lock.lock().await;
        app.storage.create(&input).await.map_err(database_error)?
    };
    app.syncer.trigger("record-created").await;
    Ok((StatusCode::CREATED, Json(record)))
}

pub async fn update_record(
    State(app): State<App>,
    Path(id): Path<i64>,
    Json(input): Json<RecordInput>,
) -> Result<Json<Record>, Response> {
    let input = validate(input).map_err(invalid)?;
    let record = {
        let _guard = app.syncer.mutation_lock.lock().await;
        let old = app
            .storage
            .get(id)
            .await
            .map_err(database_error)?
            .ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;
        if old.fqdn != input.fqdn || old.record_type != input.record_type {
            return Err(invalid("name and type are immutable; create a new record"));
        }
        app.storage
            .update(id, &input)
            .await
            .map_err(database_error)?
            .ok_or_else(|| StatusCode::NOT_FOUND.into_response())?
    };
    app.syncer.trigger("record-updated").await;
    Ok(Json(record))
}

pub async fn delete_record(
    State(app): State<App>,
    Path(id): Path<i64>,
) -> Result<StatusCode, Response> {
    let _guard = app.syncer.mutation_lock.lock().await;
    sync::delete_record(&app.storage, &app.cloudflare, id)
        .await
        .map_err(|error| (StatusCode::CONFLICT, error).into_response())?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn sync_now(State(app): State<App>) -> Result<Json<SyncOutcome>, Response> {
    app.syncer
        .sync_now("manual", None)
        .await
        .map(Json)
        .ok_or_else(|| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[derive(Serialize)]
pub struct Status {
    last_sync: Option<SyncRun>,
    sync_interval_secs: u64,
}

pub async fn status(State(app): State<App>) -> Json<Status> {
    let last_sync = match app.storage.latest_sync_run().await {
        Ok(run) => run,
        Err(error) => {
            tracing::error!(%error, "failed to read latest sync run");
            None
        }
    };
    Json(Status {
        last_sync,
        sync_interval_secs: app.sync_interval,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> RecordInput {
        RecordInput {
            fqdn: "Home.Example.COM.".into(),
            record_type: "a".into(),
            value: None,
            ttl: 1,
            priority: None,
            weight: None,
            port: None,
        }
    }
    #[test]
    fn normalizes_names_and_accepts_dynamic_address() {
        let result = validate(input()).unwrap();
        assert_eq!(result.fqdn, "home.example.com");
        assert_eq!(result.record_type, "A");
    }
    #[test]
    fn reserves_acme_challenge_records() {
        assert!(
            validate(RecordInput {
                fqdn: "_acme-challenge.example.com".into(),
                ..input()
            })
            .is_err()
        );
    }
    #[test]
    fn rejects_dynamic_mail_and_missing_srv_fields() {
        assert!(
            validate(RecordInput {
                record_type: "MX".into(),
                priority: Some(10),
                ..input()
            })
            .is_err()
        );
        assert!(
            validate(RecordInput {
                record_type: "SRV".into(),
                priority: Some(10),
                value: Some("sip.example.com".into()),
                ..input()
            })
            .is_err()
        );
    }
    #[test]
    fn validates_and_normalizes_caa() {
        let result = validate(RecordInput {
            record_type: "CAA".into(),
            value: Some("0 issue letsencrypt.org".into()),
            ..input()
        })
        .unwrap();
        assert_eq!(result.value.as_deref(), Some("0 issue letsencrypt.org"));
        assert!(
            validate(RecordInput {
                record_type: "CAA".into(),
                value: Some("256 issue example.com".into()),
                ..input()
            })
            .is_err()
        );
    }

    #[test]
    fn preserves_txt_case_and_whitespace() {
        let value = "  CaseSensitive  ";
        let result = validate(RecordInput {
            record_type: "TXT".into(),
            value: Some(value.into()),
            ..input()
        })
        .unwrap();
        assert_eq!(result.value.as_deref(), Some(value));
    }
}
