use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Entry {
    pub id: i64,
    pub fqdn: String,
    pub ipv4: bool,
    pub ipv6: bool,
    pub ttl: i64,
    pub ipv6_override: Option<String>,
    pub last_synced_ipv4: Option<String>,
    pub last_synced_ipv4_at: Option<DateTime<Utc>>,
    pub last_synced_ipv6: Option<String>,
    pub last_synced_ipv6_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct EntryValues<'a> {
    pub fqdn: &'a str,
    pub ipv4: bool,
    pub ipv6: bool,
    pub ttl: i64,
    pub ipv6_override: Option<&'a str>,
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

    pub async fn list(&self) -> Result<Vec<Entry>, sqlx::Error> {
        sqlx::query_as::<_, Entry>("SELECT * FROM entries ORDER BY fqdn")
            .fetch_all(&self.pool)
            .await
    }

    pub async fn get(&self, id: i64) -> Result<Option<Entry>, sqlx::Error> {
        sqlx::query_as::<_, Entry>("SELECT * FROM entries WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn create(&self, values: &EntryValues<'_>) -> Result<Entry, sqlx::Error> {
        sqlx::query_as::<_, Entry>(
            "INSERT INTO entries (fqdn, ipv4, ipv6, ttl, ipv6_override)
             VALUES ($1, $2, $3, $4, $5) RETURNING *",
        )
        .bind(values.fqdn)
        .bind(values.ipv4)
        .bind(values.ipv6)
        .bind(values.ttl)
        .bind(values.ipv6_override)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn update(&self, id: i64, values: &EntryValues<'_>) -> Result<Option<Entry>, sqlx::Error> {
        sqlx::query_as::<_, Entry>(
            "UPDATE entries
             SET fqdn = $2, ipv4 = $3, ipv6 = $4, ttl = $5, ipv6_override = $6,
                 last_synced_ipv4 = CASE WHEN $3 THEN last_synced_ipv4 END,
                 last_synced_ipv4_at = CASE WHEN $3 THEN last_synced_ipv4_at END,
                 last_synced_ipv6 = CASE WHEN $4 THEN last_synced_ipv6 END,
                 last_synced_ipv6_at = CASE WHEN $4 THEN last_synced_ipv6_at END,
                 updated_at = now()
             WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(values.fqdn)
        .bind(values.ipv4)
        .bind(values.ipv6)
        .bind(values.ttl)
        .bind(values.ipv6_override)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn delete(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM entries WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_synced(&self, id: i64, record_type: &str, ip: &str) -> Result<(), sqlx::Error> {
        let query = match record_type {
            "A" => {
                "UPDATE entries SET last_synced_ipv4 = $2, last_synced_ipv4_at = now() WHERE id = $1"
            }
            "AAAA" => {
                "UPDATE entries SET last_synced_ipv6 = $2, last_synced_ipv6_at = now() WHERE id = $1"
            }
            other => {
                return Err(sqlx::Error::Protocol(format!(
                    "mark_synced called with unknown record type {other}"
                )));
            }
        };
        sqlx::query(query).bind(id).bind(ip).execute(&self.pool).await?;
        Ok(())
    }
}
