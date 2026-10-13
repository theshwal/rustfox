//! SQLite persistence. The schema is the one extracted from the original
//! server (contract/db-schema.sql), applied verbatim so an existing openfox
//! database opens without any data migration.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The full DDL captured from co-l/openfox develop. Kept in sync with
/// contract/db-schema.sql; `tests/schema_matches_contract.rs` enforces that.
pub const SCHEMA: &str = include_str!("../contract/db-schema.sql");

#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

impl Db {
    /// Borrow the underlying connection handle (axum's State derefs to this).
    pub fn inner(&self) -> &Arc<Mutex<Connection>> {
        &self.0
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = if path.as_os_str() == ":memory:" {
            Connection::open_in_memory().context("open in-memory database")?
        } else {
            Connection::open(path)
                .with_context(|| format!("open database at {}", path.display()))?
        };

        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "foreign_keys", "ON").ok();

        let db = Db(Arc::new(Mutex::new(conn)));
        db.migrate()?;
        Ok(db)
    }

    /// Apply the extracted schema. The DDL captured from the original server
    /// uses bare CREATE TABLE/CREATE INDEX, so make each statement idempotent
    /// here; that keeps this safe against both a fresh file and an existing
    /// openfox database, and makes repeated runs a no-op.
    pub fn migrate(&self) -> Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .context("enable foreign keys")?;
        for stmt in split_statements(SCHEMA) {
            let idempotent = make_idempotent(&stmt);
            conn.execute_batch(&idempotent)
                .with_context(|| format!("apply schema statement: {idempotent}"))?;
        }
        Ok(())
    }

    pub fn query_map<T, F>(&self, sql: &str, f: F) -> Result<Vec<T>>
    where
        F: Fn(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        self.query_map_with(sql, &[], f)
    }

    /// Parameterised variant; every `?n` in `sql` must be supplied.
    pub fn query_map_with<T, F>(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
        f: F,
    ) -> Result<Vec<T>>
    where
        F: Fn(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, f)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn table_names(&self) -> Result<Vec<String>> {
        self.query_map(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            |r| r.get::<_, String>(0),
        )
    }

    pub fn count(&self, table: &str) -> Result<i64> {
        let conn = self.0.lock().unwrap();
        let sql = format!("SELECT COUNT(*) FROM \"{table}\"");
        let n: i64 = conn.query_row(&sql, [], |r| r.get(0))?;
        Ok(n)
    }

    /// Run a statement that changes rows, returning the number affected.
    pub fn execute(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<usize> {
        let conn = self.0.lock().unwrap();
        let n = conn.execute(sql, params)?;
        Ok(n)
    }

    /// Run a statement and return the inserted rowid (used by the event log).
    pub fn insert_returning_id(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<i64> {
        let conn = self.0.lock().unwrap();
        Ok(conn.execute(sql, params)? as i64)
    }

    /// Whether a SELECT returns at least one row. The selected column type is
    /// irrelevant, so the first column is read as a value of any kind.
    pub fn exists(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> bool {
        let conn = self.0.lock().unwrap();
        match conn.query_row(sql, params, |_| Ok(())) {
            Ok(()) => true,
            Err(rusqlite::Error::QueryReturnedNoRows) => false,
            Err(_) => false,
        }
    }

    /// Scalar integer query (COUNT/MAX/seq). Errors when no row is returned.
    pub fn query_scalar_i64(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<i64> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query(params)?;
        match rows.next()? {
            Some(row) => Ok(row.get(0)?),
            None => Err(anyhow::anyhow!("no rows")),
        }
    }
}

/// Rewrite a CREATE statement so re-running it is a no-op.
fn make_idempotent(stmt: &str) -> String {
    let upper = stmt.to_ascii_uppercase();
    let head = upper.trim_start();
    if head.starts_with("PRAGMA") {
        return stmt.to_string();
    }
    if head.starts_with("CREATE TABLE IF NOT EXISTS") {
        return stmt.to_string();
    }
    if head.starts_with("CREATE TABLE") {
        return stmt.replacen("CREATE TABLE", "CREATE TABLE IF NOT EXISTS", 1);
    }
    if head.starts_with("CREATE INDEX") {
        return stmt.replacen("CREATE INDEX", "CREATE INDEX IF NOT EXISTS", 1);
    }
    if head.starts_with("CREATE UNIQUE INDEX") {
        return stmt.replacen(
            "CREATE UNIQUE INDEX",
            "CREATE UNIQUE INDEX IF NOT EXISTS",
            1,
        );
    }
    stmt.to_string()
}

/// Split a SQL script into individual statements, dropping comment-only chunks.
fn split_statements(sql: &str) -> Vec<String> {
    sql.split(';')
        .map(|s| {
            s.lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_applies_to_fresh_database() {
        let db = Db::open(Path::new(":memory:")).unwrap();
        let tables = db.table_names().unwrap();
        for expected in [
            "projects",
            "sessions",
            "settings",
            "events",
            "tasks",
            "task_links",
            "task_gates",
            "task_gate_values",
            "task_audit",
            "workflow_executions",
            "notifications",
            "project_task_settings",
        ] {
            assert!(
                tables.iter().any(|t| t == expected),
                "missing table {expected}; got {tables:?}"
            );
        }
    }

    #[test]
    fn migrate_is_idempotent() {
        let db = Db::open(Path::new(":memory:")).unwrap();
        db.migrate().unwrap();
        db.migrate().unwrap();
    }
}
