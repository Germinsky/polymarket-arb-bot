// src/journal/logger.rs — Persistent trade journal (CSV + SQLite)
use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct TradeRecord {
    pub timestamp: String,
    pub condition_id: String,
    pub question: String,
    pub side: String,
    pub token_id: String,
    pub entry_price: f64,
    pub size: f64,
    pub pnl: f64,
    pub divergence: f64,
    pub latency_us: u64,
    pub dry_run: bool,
}

pub struct TradeJournal {
    csv_path: String,
    db: Connection,
}

impl TradeJournal {
    /// Open / create the journal files. Creates CSV header and SQLite table if needed.
    pub fn open(data_dir: &str) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let csv_path = format!("{}/trades.csv", data_dir);
        let db_path = format!("{}/trades.db", data_dir);

        // CSV: write header if file is new
        let csv_exists = Path::new(&csv_path).exists();
        if !csv_exists {
            let mut wtr = csv::Writer::from_path(&csv_path)?;
            wtr.write_record([
                "timestamp",
                "condition_id",
                "question",
                "side",
                "token_id",
                "entry_price",
                "size",
                "pnl",
                "divergence",
                "latency_us",
                "dry_run",
            ])?;
            wtr.flush()?;
        }

        // SQLite
        let db = Connection::open(&db_path)?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS trades (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp   TEXT    NOT NULL,
                condition_id TEXT   NOT NULL,
                question    TEXT    NOT NULL,
                side        TEXT    NOT NULL,
                token_id    TEXT    NOT NULL,
                entry_price REAL    NOT NULL,
                size        REAL    NOT NULL,
                pnl         REAL    NOT NULL,
                divergence  REAL    NOT NULL,
                latency_us  INTEGER NOT NULL,
                dry_run     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_trades_ts ON trades(timestamp);",
        )?;

        Ok(Self { csv_path, db })
    }

    /// Record a trade to both CSV and SQLite.
    pub fn record(&self, rec: &TradeRecord) -> Result<()> {
        // CSV append
        let file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.csv_path)?;
        let mut wtr = csv::Writer::from_writer(file);
        wtr.serialize(rec)?;
        wtr.flush()?;

        // SQLite
        self.db.execute(
            "INSERT INTO trades (timestamp, condition_id, question, side, token_id,
                                 entry_price, size, pnl, divergence, latency_us, dry_run)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                rec.timestamp,
                rec.condition_id,
                rec.question,
                rec.side,
                rec.token_id,
                rec.entry_price,
                rec.size,
                rec.pnl,
                rec.divergence,
                rec.latency_us,
                rec.dry_run as i32,
            ],
        )?;
        Ok(())
    }

    /// Get today's total PnL from the database.
    pub fn daily_pnl(&self) -> Result<f64> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut stmt = self.db.prepare(
            "SELECT COALESCE(SUM(pnl), 0.0) FROM trades WHERE timestamp LIKE ?1",
        )?;
        let pnl: f64 = stmt.query_row(rusqlite::params![format!("{today}%")], |row| row.get(0))?;
        Ok(pnl)
    }
}
