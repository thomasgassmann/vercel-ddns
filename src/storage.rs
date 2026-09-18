use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Record {
    pub id: i64,
    pub fqdn: String,
    pub record_type: String,
    pub value: Option<String>,
    pub ttl: i64,
    pub priority: Option<i32>,
    pub weight: Option<i32>,
    pub port: Option<i32>,
    pub zone_id: Option<String>,
    pub provider_record_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordInput {
    pub fqdn: String,
    pub record_type: String,
    pub value: Option<String>,
    pub ttl: i64,
    pub priority: Option<i32>,
    pub weight: Option<i32>,
    pub port: Option<i32>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SyncRun {
    pub source: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub created: i32,
    pub updated: i32,
    pub unchanged: i32,
    pub failed: i32,
    pub error: Option<String>,
}

pub struct Storage {
    pool: PgPool,
}

impl Storage {
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn list(&self) -> Result<Vec<Record>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM records ORDER BY fqdn, record_type, id")
            .fetch_all(&self.pool)
            .await
    }

    pub async fn get(&self, id: i64) -> Result<Option<Record>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM records WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn create(&self, v: &RecordInput) -> Result<Record, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO records (fqdn, record_type, value, ttl, priority, weight, port)
             VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING *",
        )
        .bind(&v.fqdn)
        .bind(&v.record_type)
        .bind(&v.value)
        .bind(v.ttl)
        .bind(v.priority)
        .bind(v.weight)
        .bind(v.port)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn update(&self, id: i64, v: &RecordInput) -> Result<Option<Record>, sqlx::Error> {
        // Name and type are immutable: changing either requires a new record.
        sqlx::query_as(
            "UPDATE records SET value=$2, ttl=$3, priority=$4, weight=$5, port=$6, updated_at=now()
             WHERE id=$1 AND fqdn=$7 AND record_type=$8 RETURNING *",
        )
        .bind(id)
        .bind(&v.value)
        .bind(v.ttl)
        .bind(v.priority)
        .bind(v.weight)
        .bind(v.port)
        .bind(&v.fqdn)
        .bind(&v.record_type)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn set_provider_id(
        &self,
        id: i64,
        zone: &str,
        provider_id: &str,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
            "UPDATE records SET zone_id=$2, provider_record_id=$3, updated_at=now()
             WHERE id=$1 AND provider_record_id IS NULL",
        )
        .bind(id)
        .bind(zone)
        .bind(provider_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM records WHERE id=$1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn save_sync_run(&self, run: &SyncRun) -> Result<(), sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO sync_runs (source, started_at, finished_at, ipv4, ipv6, created, updated, unchanged, failed, error)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(&run.source).bind(run.started_at).bind(run.finished_at)
        .bind(&run.ipv4).bind(&run.ipv6).bind(run.created).bind(run.updated)
        .bind(run.unchanged).bind(run.failed).bind(&run.error)
        .execute(&mut *transaction).await?;
        sqlx::query("DELETE FROM sync_runs WHERE finished_at < now() - interval '30 days'")
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await
    }

    pub async fn latest_sync_run(&self) -> Result<Option<SyncRun>, sqlx::Error> {
        sqlx::query_as("SELECT source, started_at, finished_at, ipv4, ipv6, created, updated, unchanged, failed, error FROM sync_runs ORDER BY finished_at DESC, id DESC LIMIT 1")
            .fetch_optional(&self.pool).await
    }
}
