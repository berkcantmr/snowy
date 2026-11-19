//! SQLite storage. Kept: public keys with their friend codes, friendships, pending requests,
//! and per-day totals (counts only: no IP addresses, no per-user activity).

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

pub struct Db {
    conn: Connection,
}

/// One day of usage totals.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct Day {
    pub day: String,
    /// Anonymous daily pings (each installation sends at most one per day).
    pub pings: i64,
    /// Different friend-feature users that connected that day.
    pub hub_users: i64,
    /// Most friend-feature users connected at the same time.
    pub peak_online: i64,
    /// Pings per app version.
    pub versions: Vec<(String, i64)>,
}

impl Db {
    pub fn open(path: &str) -> Result<Db> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS users (
                 key TEXT PRIMARY KEY,
                 code TEXT NOT NULL UNIQUE,
                 created INTEGER NOT NULL,
                 last_day TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS friends (
                 a TEXT NOT NULL, b TEXT NOT NULL,
                 PRIMARY KEY (a, b)
             );
             CREATE TABLE IF NOT EXISTS requests (
                 src TEXT NOT NULL, dst TEXT NOT NULL, created INTEGER NOT NULL,
                 PRIMARY KEY (src, dst)
             );
             CREATE TABLE IF NOT EXISTS daily (
                 day TEXT PRIMARY KEY,
                 pings INTEGER NOT NULL DEFAULT 0,
                 hub_users INTEGER NOT NULL DEFAULT 0,
                 peak_online INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS daily_ver (
                 day TEXT NOT NULL, ver TEXT NOT NULL, pings INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (day, ver)
             );",
        )?;
        Ok(Db { conn })
    }

    #[cfg(test)]
    pub fn memory() -> Result<Db> {
        Db::open(":memory:")
    }

    /// Registers the key (first login) and records that it was seen today. Returns false when
    /// another key already has this code (a 40-bit collision: practically never).
    pub fn login(&self, key: &str, code: &str, day: &str, now: i64) -> Result<bool> {
        let owner: Option<String> = self.conn.query_row("SELECT key FROM users WHERE code = ?1", [code], |r| r.get(0)).optional()?;
        if owner.as_deref().is_some_and(|k| k != key) {
            return Ok(false);
        }
        self.conn.execute(
            "INSERT INTO users (key, code, created, last_day) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET last_day = excluded.last_day",
            params![key, code, now, day],
        )?;
        Ok(true)
    }

    pub fn key_of(&self, code: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT key FROM users WHERE code = ?1", [code], |r| r.get(0)).optional()?)
    }

    pub fn are_friends(&self, a: &str, b: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT 1 FROM friends WHERE a = ?1 AND b = ?2", [a, b], |_| Ok(())).optional()?.is_some())
    }

    pub fn friends_of(&self, code: &str) -> Result<Vec<(String, String)>> {
        let mut st = self.conn.prepare("SELECT f.b, u.key FROM friends f JOIN users u ON u.code = f.b WHERE f.a = ?1 ORDER BY f.b")?;
        let rows = st.query_map([code], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn make_friends(&self, a: &str, b: &str) -> Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO friends (a, b) VALUES (?1, ?2)", [a, b])?;
        self.conn.execute("INSERT OR IGNORE INTO friends (a, b) VALUES (?1, ?2)", [b, a])?;
        self.conn.execute("DELETE FROM requests WHERE (src = ?1 AND dst = ?2) OR (src = ?2 AND dst = ?1)", [a, b])?;
        Ok(())
    }

    pub fn unfriend(&self, a: &str, b: &str) -> Result<()> {
        self.conn.execute("DELETE FROM friends WHERE (a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1)", [a, b])?;
        self.conn.execute("DELETE FROM requests WHERE (src = ?1 AND dst = ?2) OR (src = ?2 AND dst = ?1)", [a, b])?;
        Ok(())
    }

    pub fn has_request(&self, src: &str, dst: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT 1 FROM requests WHERE src = ?1 AND dst = ?2", [src, dst], |_| Ok(())).optional()?.is_some())
    }

    pub fn add_request(&self, src: &str, dst: &str, now: i64) -> Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO requests (src, dst, created) VALUES (?1, ?2, ?3)", params![src, dst, now])?;
        Ok(())
    }

    pub fn drop_request(&self, src: &str, dst: &str) -> Result<()> {
        self.conn.execute("DELETE FROM requests WHERE src = ?1 AND dst = ?2", [src, dst])?;
        Ok(())
    }

    pub fn outgoing(&self, code: &str) -> Result<Vec<String>> {
        self.list("SELECT dst FROM requests WHERE src = ?1 ORDER BY created", code)
    }

    pub fn incoming(&self, code: &str) -> Result<Vec<String>> {
        self.list("SELECT src FROM requests WHERE dst = ?1 ORDER BY created", code)
    }

    fn list(&self, sql: &str, arg: &str) -> Result<Vec<String>> {
        let mut st = self.conn.prepare(sql)?;
        let rows = st.query_map([arg], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Drops requests older than `before` (unix seconds).
    pub fn expire_requests(&self, before: i64) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM requests WHERE created < ?1", [before])?)
    }

    pub fn count_ping(&self, day: &str, ver: &str) -> Result<()> {
        self.conn.execute("INSERT INTO daily (day, pings) VALUES (?1, 1) ON CONFLICT(day) DO UPDATE SET pings = pings + 1", [day])?;
        self.conn.execute(
            "INSERT INTO daily_ver (day, ver, pings) VALUES (?1, ?2, 1) ON CONFLICT(day, ver) DO UPDATE SET pings = pings + 1",
            [day, ver],
        )?;
        Ok(())
    }

    /// Keeps the highest values seen for the day.
    pub fn record_hub(&self, day: &str, users: i64, online: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO daily (day, hub_users, peak_online) VALUES (?1, ?2, ?3)
             ON CONFLICT(day) DO UPDATE SET hub_users = max(hub_users, ?2), peak_online = max(peak_online, ?3)",
            params![day, users, online],
        )?;
        Ok(())
    }

    /// Different keys seen today (for `hub_users` after a restart).
    pub fn users_seen(&self, day: &str) -> Result<i64> {
        Ok(self.conn.query_row("SELECT count(*) FROM users WHERE last_day = ?1", [day], |r| r.get(0))?)
    }

    pub fn days(&self, limit: i64) -> Result<Vec<Day>> {
        let mut st = self.conn.prepare("SELECT day, pings, hub_users, peak_online FROM daily ORDER BY day DESC LIMIT ?1")?;
        let mut days: Vec<Day> = st
            .query_map([limit], |r| Ok(Day { day: r.get(0)?, pings: r.get(1)?, hub_users: r.get(2)?, peak_online: r.get(3)?, versions: Vec::new() }))?
            .collect::<std::result::Result<_, _>>()?;
        let mut vs = self.conn.prepare("SELECT ver, pings FROM daily_ver WHERE day = ?1 ORDER BY pings DESC")?;
        for d in &mut days {
            d.versions = vs.query_map([&d.day], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
        }
        Ok(days)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendship_lifecycle() {
        let db = Db::memory().unwrap();
        assert!(db.login("ka", "AAAA-AAAA", "2026-11-19", 1).unwrap());
        assert!(db.login("kb", "BBBB-BBBB", "2026-11-19", 1).unwrap());
        assert!(!db.login("kc", "AAAA-AAAA", "2026-11-19", 1).unwrap(), "code collision refused");
        db.add_request("AAAA-AAAA", "BBBB-BBBB", 1).unwrap();
        assert_eq!(db.incoming("BBBB-BBBB").unwrap(), ["AAAA-AAAA"]);
        db.make_friends("BBBB-BBBB", "AAAA-AAAA").unwrap();
        assert!(db.are_friends("AAAA-AAAA", "BBBB-BBBB").unwrap() && db.are_friends("BBBB-BBBB", "AAAA-AAAA").unwrap());
        assert!(db.incoming("BBBB-BBBB").unwrap().is_empty());
        assert_eq!(db.friends_of("AAAA-AAAA").unwrap(), [("BBBB-BBBB".to_string(), "kb".to_string())]);
        db.unfriend("BBBB-BBBB", "AAAA-AAAA").unwrap();
        assert!(!db.are_friends("AAAA-AAAA", "BBBB-BBBB").unwrap());
    }

    #[test]
    fn daily_totals() {
        let db = Db::memory().unwrap();
        db.count_ping("2026-11-19", "0.1.6").unwrap();
        db.count_ping("2026-11-19", "0.1.6").unwrap();
        db.count_ping("2026-11-19", "0.1.5").unwrap();
        db.record_hub("2026-11-19", 4, 3).unwrap();
        db.record_hub("2026-11-19", 2, 5).unwrap();
        let d = &db.days(10).unwrap()[0];
        assert_eq!((d.pings, d.hub_users, d.peak_online), (3, 4, 5));
        assert_eq!(d.versions, [("0.1.6".to_string(), 2), ("0.1.5".to_string(), 1)]);
    }
}
