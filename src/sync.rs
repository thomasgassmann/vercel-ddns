use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::{Mutex, RwLock, mpsc, oneshot};

use crate::cloudflare::{CaaData, Cloudflare, DnsRecord, RecordData, SrvData};
use crate::ip;
use crate::metrics::Metrics;
use crate::storage::{Record, Storage, SyncRun};

#[derive(Debug, Clone, Serialize)]
pub struct SyncOutcome {
    pub source: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub created: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub failed: u32,
    pub error: Option<String>,
}

impl SyncOutcome {
    fn as_sync_run(&self) -> SyncRun {
        SyncRun {
            source: self.source.clone(),
            started_at: self.started_at,
            finished_at: self.finished_at,
            ipv4: self.ipv4.clone(),
            ipv6: self.ipv6.clone(),
            created: self.created as i32,
            updated: self.updated as i32,
            unchanged: self.unchanged as i32,
            failed: self.failed as i32,
            error: self.error.clone(),
        }
    }

    pub fn changed(&self) -> bool {
        self.created + self.updated > 0
    }
    pub fn ok(&self) -> bool {
        self.failed == 0 && self.error.is_none()
    }
}

pub struct SyncRequest {
    pub source: &'static str,
    pub myip: Option<Ipv4Addr>,
    pub respond: Option<oneshot::Sender<SyncOutcome>>,
}

#[derive(Clone)]
pub struct Syncer {
    tx: mpsc::Sender<SyncRequest>,
    last: Arc<RwLock<Option<SyncOutcome>>>,
    // Single-replica only: API mutations and reconciliation share this lock.
    pub mutation_lock: Arc<Mutex<()>>,
}

impl Syncer {
    pub fn spawn(
        storage: Arc<Storage>,
        cloudflare: Arc<Cloudflare>,
        metrics: Arc<Metrics>,
        interval_secs: u64,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel::<SyncRequest>(16);
        let last = Arc::new(RwLock::new(None));
        let mutation_lock = Arc::new(Mutex::new(()));
        let worker_lock = mutation_lock.clone();
        let worker_last = last.clone();
        tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                let outcome = {
                    let _guard = worker_lock.lock().await;
                    run_sync(&storage, &cloudflare, &request).await
                };
                let run = outcome.as_sync_run();
                if let Err(error) = storage.save_sync_run(&run).await {
                    tracing::error!(%error, "failed to save sync run");
                } else {
                    metrics.observe_sync(&run);
                }
                tracing::info!(
                    source = request.source,
                    created = outcome.created,
                    updated = outcome.updated,
                    failed = outcome.failed,
                    "sync finished"
                );
                *worker_last.write().await = Some(outcome.clone());
                if let Some(respond) = request.respond {
                    let _ = respond.send(outcome);
                }
            }
        });
        let timer_tx = tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if timer_tx
                    .send(SyncRequest {
                        source: "timer",
                        myip: None,
                        respond: None,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
        });
        Self {
            tx,
            last,
            mutation_lock,
        }
    }

    pub async fn trigger(&self, source: &'static str) {
        let _ = self
            .tx
            .send(SyncRequest {
                source,
                myip: None,
                respond: None,
            })
            .await;
    }

    pub async fn sync_now(
        &self,
        source: &'static str,
        myip: Option<Ipv4Addr>,
    ) -> Option<SyncOutcome> {
        let (respond, done) = oneshot::channel();
        self.tx
            .send(SyncRequest {
                source,
                myip,
                respond: Some(respond),
            })
            .await
            .ok()?;
        done.await.ok()
    }

    pub async fn last(&self) -> Option<SyncOutcome> {
        self.last.read().await.clone()
    }
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim_end_matches('.')
        .eq_ignore_ascii_case(b.trim_end_matches('.'))
}

fn belongs_to(fqdn: &str, zone: &str) -> bool {
    same_name(fqdn, zone)
        || fqdn
            .to_ascii_lowercase()
            .ends_with(&format!(".{}", zone.to_ascii_lowercase()))
}

fn identity_matches(record: &Record, remote: &DnsRecord) -> bool {
    same_name(&record.fqdn, &remote.name) && record.record_type == remote.record_type
}

fn desired<'a>(record: &'a Record, value: &'a str) -> crate::cloudflare::Record<'a> {
    crate::cloudflare::Record {
        name: &record.fqdn,
        record_type: &record.record_type,
        content: value,
        ttl: record.ttl as u32,
        priority: (record.record_type == "MX").then(|| record.priority.unwrap() as u16),
        data: match record.record_type.as_str() {
            "SRV" => Some(RecordData::Srv(SrvData {
                priority: record.priority.unwrap() as u16,
                weight: record.weight.unwrap() as u16,
                port: record.port.unwrap() as u16,
                target: value,
            })),
            "CAA" => Some(RecordData::Caa(caa_data(value))),
            _ => None,
        },
    }
}

fn content_matches(record: &Record, value: &str, remote: &DnsRecord) -> bool {
    match record.record_type.as_str() {
        "A" => value
            .parse::<Ipv4Addr>()
            .ok()
            .zip(remote.content.parse::<Ipv4Addr>().ok())
            .is_some_and(|(a, b)| a == b),
        "AAAA" => value
            .parse::<Ipv6Addr>()
            .ok()
            .zip(remote.content.parse::<Ipv6Addr>().ok())
            .is_some_and(|(a, b)| a == b),
        "CNAME" | "MX" => {
            same_name(value, &remote.content)
                && (record.record_type != "MX" || record.priority == remote.priority.map(i32::from))
        }
        "CAA" => remote.data.as_ref().is_some_and(|data| {
            let expected = caa_data(value);
            data.get("flags").and_then(|v| v.as_u64()) == Some(u64::from(expected.flags))
                && data.get("tag").and_then(|v| v.as_str()) == Some(expected.tag)
                && data.get("value").and_then(|v| v.as_str()) == Some(expected.value)
        }),
        "SRV" => remote.data.as_ref().is_some_and(|data| {
            data.get("target")
                .and_then(|v| v.as_str())
                .is_some_and(|target| same_name(value, target))
                && data.get("priority").and_then(|v| v.as_i64()) == record.priority.map(i64::from)
                && data.get("weight").and_then(|v| v.as_i64()) == record.weight.map(i64::from)
                && data.get("port").and_then(|v| v.as_i64()) == record.port.map(i64::from)
        }),
        _ => value == remote.content,
    }
}

fn caa_data(value: &str) -> CaaData<'_> {
    let mut parts = value.splitn(3, ' ');
    CaaData {
        flags: parts.next().unwrap().parse().unwrap(),
        tag: parts.next().unwrap(),
        value: parts.next().unwrap(),
    }
}

pub async fn delete_record(
    storage: &Storage,
    cloudflare: &Cloudflare,
    id: i64,
) -> Result<(), String> {
    let record = storage
        .get(id)
        .await
        .map_err(|_| "database read failed")?
        .ok_or("record not found")?;
    if let (Some(zone), Some(provider_id)) = (&record.zone_id, &record.provider_record_id) {
        let remote = cloudflare
            .get_record(zone, provider_id)
            .await
            .map_err(|e| e.to_string())?;
        if remote.id != *provider_id || !identity_matches(&record, &remote) {
            return Err("provider record identity changed; refusing deletion".into());
        }
        cloudflare
            .delete_record(zone, provider_id)
            .await
            .map_err(|e| e.to_string())?;
    }
    storage
        .delete(id)
        .await
        .map_err(|_| "database deletion failed")?
        .then_some(())
        .ok_or_else(|| "record not deleted".into())
}

async fn reconcile(
    storage: &Storage,
    cloudflare: &Cloudflare,
    record: &Record,
    zone: &str,
    existing: &mut Vec<DnsRecord>,
    value: &str,
) -> Result<&'static str, String> {
    let result = if let Some(id) = &record.provider_record_id {
        if record.zone_id.as_deref() != Some(zone) {
            return Err("provider zone changed".into());
        }
        let current = existing
            .iter()
            .find(|r| r.id == *id)
            .ok_or("provider record missing; refusing automatic replacement")?;
        if !identity_matches(record, current) {
            return Err("provider record identity changed".into());
        }
        if content_matches(record, value, current)
            && current.ttl == record.ttl as u32
            && !current.proxied
        {
            "unchanged"
        } else {
            let remote = cloudflare
                .update_record(zone, id, &desired(record, value))
                .await
                .map_err(|e| e.to_string())?;
            if remote.id != *id || !identity_matches(record, &remote) {
                return Err("unexpected updated record identity".into());
            }
            let index = existing.iter().position(|r| r.id == *id).unwrap();
            existing[index] = remote;
            "updated"
        }
    } else if let Some(remote) = existing.iter().find(|remote| {
        identity_matches(record, remote)
            && content_matches(record, value, remote)
            && remote.ttl == record.ttl as u32
            && !remote.proxied
    }) {
        storage
            .set_provider_id(record.id, zone, &remote.id)
            .await
            .map_err(|_| "provider record ID could not be saved")?;
        "unchanged"
    } else {
        let remote = cloudflare
            .create_record(zone, &desired(record, value))
            .await
            .map_err(|e| e.to_string())?;
        if remote.id.is_empty() || !identity_matches(record, &remote) {
            return Err("unexpected created record identity".into());
        }
        storage
            .set_provider_id(record.id, zone, &remote.id)
            .await
            .map_err(|_| "created record ID could not be saved")?;
        existing.push(remote);
        "created"
    };
    Ok(result)
}

async fn run_sync(
    storage: &Storage,
    cloudflare: &Cloudflare,
    request: &SyncRequest,
) -> SyncOutcome {
    let now = Utc::now();
    let mut outcome = SyncOutcome {
        source: request.source.into(),
        started_at: now,
        finished_at: now,
        ipv4: None,
        ipv6: None,
        created: 0,
        updated: 0,
        unchanged: 0,
        failed: 0,
        error: None,
    };
    let result: Result<(), String> = async {
        let records = storage.list().await.map_err(|_| "database read failed")?;
        if records.is_empty() {
            return Ok(());
        }
        if records
            .iter()
            .any(|r| r.record_type == "A" && r.value.is_none())
        {
            outcome.ipv4 = ip::resolve_ipv4(request.myip)
                .await
                .map(|ip| ip.to_string());
        }
        if records
            .iter()
            .any(|r| r.record_type == "AAAA" && r.value.is_none())
        {
            outcome.ipv6 = ip::resolve_host_ipv6().await.map(|ip| ip.to_string());
        }
        let zones = cloudflare.list_zones().await.map_err(|e| e.to_string())?;
        let mut cache = std::collections::HashMap::new();
        for record in &records {
            let dynamic =
                matches!(record.record_type.as_str(), "A" | "AAAA") && record.value.is_none();
            if matches!(request.source, "timer" | "webhook") && !dynamic {
                outcome.unchanged += 1;
                continue;
            }
            let result = async {
                let value = record
                    .value
                    .as_deref()
                    .or(match record.record_type.as_str() {
                        "A" => outcome.ipv4.as_deref(),
                        "AAAA" => outcome.ipv6.as_deref(),
                        _ => None,
                    })
                    .ok_or("dynamic address unavailable")?;
                let zone = zones
                    .iter()
                    .filter(|z| belongs_to(&record.fqdn, &z.name))
                    .max_by_key(|z| z.name.len())
                    .ok_or("no matching Cloudflare zone")?;
                if !cache.contains_key(&zone.id) {
                    cache.insert(
                        zone.id.clone(),
                        cloudflare
                            .list_records(&zone.id)
                            .await
                            .map_err(|e| e.to_string())?,
                    );
                }
                reconcile(
                    storage,
                    cloudflare,
                    record,
                    &zone.id,
                    cache.get_mut(&zone.id).unwrap(),
                    value,
                )
                .await
            }
            .await;
            match result {
                Ok("created") => outcome.created += 1,
                Ok("updated") => outcome.updated += 1,
                Ok(_) => outcome.unchanged += 1,
                Err(error) => {
                    tracing::warn!(id = record.id, %error, "record sync failed");
                    outcome.failed += 1;
                    outcome.error = Some(error);
                }
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        outcome.error = Some(error);
    }
    outcome.finished_at = Utc::now();
    outcome
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
