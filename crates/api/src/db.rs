//! SQLite persistence layer for subscriptions, nodes and proxy state.

use crate::models::StoredSubscription;
use ironpass_core::models::{ProxyNode, SubscriptionMetadata};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct DbPool {
    conn: Arc<Mutex<Connection>>,
}

impl Clone for DbPool {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

impl DbPool {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, rusqlite::Error> {
        let path: PathBuf = path.into();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(&path)?;
        let pool = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        pool.init()?;
        Ok(pool)
    }

    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        let pool = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        pool.init()?;
        Ok(pool)
    }

    fn init(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS subscriptions (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                name TEXT,
                hwid TEXT,
                added_at TEXT NOT NULL,
                last_updated TEXT,
                is_active INTEGER NOT NULL DEFAULT 1,
                metadata TEXT NOT NULL DEFAULT '{}',
                traffic_used INTEGER,
                traffic_total INTEGER,
                expires_at TEXT
            );

            CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                subscription_id TEXT NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
                node_json TEXT NOT NULL,
                FOREIGN KEY (subscription_id) REFERENCES subscriptions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_nodes_subscription ON nodes(subscription_id);
            "#,
        )?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        Ok(())
    }

    pub fn insert_subscription(&self, sub: &StoredSubscription) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO subscriptions (id, url, name, hwid, added_at, last_updated, is_active, metadata, traffic_used, traffic_total, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                sub.id.to_string(),
                sub.url,
                sub.name,
                sub.hwid,
                sub.added_at.to_rfc3339(),
                sub.last_updated.map(|t| t.to_rfc3339()),
                sub.is_active as i32,
                serde_json::to_string(&sub.metadata).unwrap_or_else(|_| "{}".into()),
                sub.traffic_used.map(|v| v as i64),
                sub.traffic_total.map(|v| v as i64),
                sub.expires_at.map(|t| t.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn update_subscription(&self, sub: &StoredSubscription) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            UPDATE subscriptions
            SET url = ?2, name = ?3, hwid = ?4, added_at = ?5, last_updated = ?6,
                is_active = ?7, metadata = ?8, traffic_used = ?9, traffic_total = ?10, expires_at = ?11
            WHERE id = ?1
            "#,
            params![
                sub.id.to_string(),
                sub.url,
                sub.name,
                sub.hwid,
                sub.added_at.to_rfc3339(),
                sub.last_updated.map(|t| t.to_rfc3339()),
                sub.is_active as i32,
                serde_json::to_string(&sub.metadata).unwrap_or_else(|_| "{}".into()),
                sub.traffic_used.map(|v| v as i64),
                sub.traffic_total.map(|v| v as i64),
                sub.expires_at.map(|t| t.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn list_subscriptions(&self) -> Result<Vec<StoredSubscription>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, name, hwid, added_at, last_updated, is_active, metadata, traffic_used, traffic_total, expires_at FROM subscriptions ORDER BY added_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(StoredSubscription {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
                url: row.get(1)?,
                name: row.get(2)?,
                hwid: row.get(3)?,
                added_at: parse_datetime(row.get(4)?),
                last_updated: row.get::<_, Option<String>>(5)?.map(parse_datetime),
                is_active: row.get::<_, i32>(6)? != 0,
                metadata: row.get::<_, String>(7).map_or_else(
                    |_| SubscriptionMetadata::default(),
                    |s| serde_json::from_str(&s).unwrap_or_default(),
                ),
                traffic_used: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                traffic_total: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
                expires_at: row.get::<_, Option<String>>(10)?.map(parse_datetime),
            })
        })?;
        rows.collect()
    }

    pub fn get_subscription(&self, id: Uuid) -> Result<Option<StoredSubscription>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, name, hwid, added_at, last_updated, is_active, metadata, traffic_used, traffic_total, expires_at FROM subscriptions WHERE id = ?1",
        )?;
        let row = stmt
            .query_row([id.to_string()], |row| {
                Ok(StoredSubscription {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
                    url: row.get(1)?,
                    name: row.get(2)?,
                    hwid: row.get(3)?,
                    added_at: parse_datetime(row.get(4)?),
                    last_updated: row.get::<_, Option<String>>(5)?.map(parse_datetime),
                    is_active: row.get::<_, i32>(6)? != 0,
                metadata: row.get::<_, String>(7).map_or_else(
                    |_| SubscriptionMetadata::default(),
                    |s| serde_json::from_str(&s).unwrap_or_default(),
                ),
                traffic_used: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                traffic_total: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
                expires_at: row.get::<_, Option<String>>(10)?.map(parse_datetime),
            })
        })
        .optional()?;
    Ok(row)
}


    pub fn delete_subscription(&self, id: Uuid) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM subscriptions WHERE id = ?1",
            [id.to_string()],
        )?;
        Ok(rows > 0)
    }

    pub fn replace_nodes(
        &self,
        subscription_id: Uuid,
        nodes: &[ProxyNode],
    ) -> Result<Vec<Uuid>, rusqlite::Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM nodes WHERE subscription_id = ?1",
            [subscription_id.to_string()],
        )?;

        let mut ids = Vec::with_capacity(nodes.len());
        for node in nodes {
            let id = Uuid::new_v4();
            ids.push(id);
            tx.execute(
                "INSERT INTO nodes (id, subscription_id, node_json) VALUES (?1, ?2, ?3)",
                params![
                    id.to_string(),
                    subscription_id.to_string(),
                    serde_json::to_string(node).unwrap_or_default(),
                ],
            )?;
        }
        tx.commit()?;
        Ok(ids)
    }

    pub fn list_nodes(
        &self,
        subscription_id: Option<Uuid>,
    ) -> Result<Vec<(Uuid, Uuid, ProxyNode)>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let (sql, params_owned): (&str, Vec<String>) = match subscription_id {
            Some(id) => (
                "SELECT id, subscription_id, node_json FROM nodes WHERE subscription_id = ?1 ORDER BY id",
                vec![id.to_string()],
            ),
            None => (
                "SELECT id, subscription_id, node_json FROM nodes ORDER BY subscription_id, id",
                vec![],
            ),
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params_owned.iter()), |row| {
            let id = Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::new_v4());
            let sub_id =
                Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_else(|_| Uuid::new_v4());
            let node: ProxyNode = serde_json::from_str(&row.get::<_, String>(2)?)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok((id, sub_id, node))
        })?;
        rows.collect()
    }

    pub fn get_node(&self, id: Uuid) -> Result<Option<(Uuid, Uuid, ProxyNode)>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, subscription_id, node_json FROM nodes WHERE id = ?1",
        )?;
        let row = stmt
            .query_row([id.to_string()], |row| {
                let id = Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::new_v4());
                let sub_id =
                    Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_else(|_| Uuid::new_v4());
                let node: ProxyNode = serde_json::from_str(&row.get::<_, String>(2)?)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok((id, sub_id, node))
            })
            .optional()?;
        Ok(row)
    }

    pub fn clear_all_nodes(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM nodes", [])?;
        Ok(())
    }
}

fn parse_datetime(s: String) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}

/// Import legacy JSON subscription store into SQLite.
pub fn import_legacy_subscriptions(
    pool: &DbPool,
    legacy: &ironpass_config::SubscriptionsStore,
) -> Result<usize, rusqlite::Error> {
    let mut count = 0;
    for sub in &legacy.subscriptions {
        let stored = StoredSubscription {
            id: Uuid::new_v4(),
            url: sub.url.clone(),
            name: sub.name.clone(),
            hwid: sub.hwid.clone(),
            added_at: sub.added_at,
            last_updated: sub.last_updated,
            is_active: sub.is_active,
            metadata: SubscriptionMetadata::default(),
            traffic_used: None,
            traffic_total: None,
            expires_at: None,
        };
        match pool.insert_subscription(&stored) {
            Ok(()) => count += 1,
            Err(e) => tracing::warn!("Failed to import subscription: {}", e),
        }
    }
    Ok(count)
}
