use crate::models::{Routine, Run};
use crate::paths;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::time::Duration;

pub type R<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    format!("Erro no banco de dados: {err}")
}

pub fn now() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

pub fn open() -> R<Connection> {
    let dir = paths::data_dir();
    std::fs::create_dir_all(&dir).map_err(|err| {
        format!("Não foi possível criar a pasta de dados {}: {err}", dir.display())
    })?;
    let _ = std::fs::create_dir_all(paths::logs_dir());
    let conn = Connection::open(paths::db_path()).map_err(e)?;
    conn.busy_timeout(Duration::from_secs(15)).map_err(e)?;
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(c: &Connection) -> R<()> {
    c.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS routines (
            id TEXT PRIMARY KEY,
            data TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            routine_id TEXT NOT NULL,
            routine_name TEXT NOT NULL,
            trigger TEXT NOT NULL,
            status TEXT NOT NULL,
            phase TEXT NOT NULL DEFAULT 'preparing',
            started_at TEXT NOT NULL,
            finished_at TEXT,
            pid INTEGER,
            total_files INTEGER NOT NULL DEFAULT 0,
            total_bytes INTEGER NOT NULL DEFAULT 0,
            done_files INTEGER NOT NULL DEFAULT 0,
            done_bytes INTEGER NOT NULL DEFAULT 0,
            current_file TEXT,
            failed_files INTEGER NOT NULL DEFAULT 0,
            skipped_files INTEGER NOT NULL DEFAULT 0,
            extra_files INTEGER NOT NULL DEFAULT 0,
            speed_bps REAL NOT NULL DEFAULT 0,
            exit_code INTEGER,
            message TEXT,
            details TEXT NOT NULL DEFAULT '[]',
            dest_folder TEXT,
            log_path TEXT,
            cancel_requested INTEGER NOT NULL DEFAULT 0,
            heartbeat TEXT,
            pruned INTEGER NOT NULL DEFAULT 0,
            notified INTEGER NOT NULL DEFAULT 0,
            mode TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_runs_routine ON runs(routine_id, started_at);
        CREATE INDEX IF NOT EXISTS idx_runs_status ON runs(status);
        "#,
    )
    .map_err(e)
}

// ---------------- Rotinas ----------------

pub fn list_routines(c: &Connection) -> R<Vec<Routine>> {
    let mut st = c.prepare("SELECT data FROM routines").map_err(e)?;
    let rows = st
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(e)?
        .filter_map(|x| x.ok())
        .filter_map(|s| serde_json::from_str::<Routine>(&s).ok())
        .collect::<Vec<_>>();
    let mut rows = rows;
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(rows)
}

pub fn get_routine(c: &Connection, id: &str) -> R<Option<Routine>> {
    let s: Option<String> = c
        .query_row("SELECT data FROM routines WHERE id=?1", [id], |r| r.get(0))
        .optional()
        .map_err(e)?;
    Ok(s.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn save_routine(c: &Connection, r: &Routine) -> R<()> {
    let data = serde_json::to_string(r).map_err(e)?;
    c.execute(
        "INSERT INTO routines(id, data) VALUES(?1, ?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        params![r.id, data],
    )
    .map_err(e)?;
    Ok(())
}

pub fn delete_routine(c: &Connection, id: &str) -> R<()> {
    c.execute("DELETE FROM routines WHERE id=?1", [id]).map_err(e)?;
    Ok(())
}

// ---------------- Configurações ----------------

pub fn get_setting(c: &Connection, key: &str) -> R<Option<String>> {
    c.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0))
        .optional()
        .map_err(e)
}

pub fn set_setting(c: &Connection, key: &str, value: &str) -> R<()> {
    c.execute(
        "INSERT INTO settings(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )
    .map_err(e)?;
    Ok(())
}

pub fn del_setting(c: &Connection, key: &str) -> R<()> {
    c.execute("DELETE FROM settings WHERE key=?1", [key]).map_err(e)?;
    Ok(())
}

// ---------------- Execuções ----------------

const RUN_COLS: &str = "id, routine_id, routine_name, trigger, status, phase, started_at, finished_at, pid,
    total_files, total_bytes, done_files, done_bytes, current_file, failed_files, skipped_files, extra_files,
    speed_bps, exit_code, message, details, dest_folder, log_path, cancel_requested, heartbeat, pruned, mode";

fn row_to_run(r: &Row) -> rusqlite::Result<Run> {
    let details: String = r.get(20)?;
    Ok(Run {
        id: r.get(0)?,
        routine_id: r.get(1)?,
        routine_name: r.get(2)?,
        trigger: r.get(3)?,
        status: r.get(4)?,
        phase: r.get(5)?,
        started_at: r.get(6)?,
        finished_at: r.get(7)?,
        pid: r.get(8)?,
        total_files: r.get(9)?,
        total_bytes: r.get(10)?,
        done_files: r.get(11)?,
        done_bytes: r.get(12)?,
        current_file: r.get(13)?,
        failed_files: r.get(14)?,
        skipped_files: r.get(15)?,
        extra_files: r.get(16)?,
        speed_bps: r.get(17)?,
        exit_code: r.get(18)?,
        message: r.get(19)?,
        details: serde_json::from_str(&details).unwrap_or_default(),
        dest_folder: r.get(21)?,
        log_path: r.get(22)?,
        cancel_requested: r.get::<_, i64>(23)? != 0,
        heartbeat: r.get(24)?,
        pruned: r.get::<_, i64>(25)? != 0,
        mode: r.get(26)?,
    })
}

pub fn create_run(c: &Connection, routine_id: &str, name: &str, trigger: &str, mode: &str) -> R<i64> {
    let t = now();
    c.execute(
        "INSERT INTO runs(routine_id, routine_name, trigger, status, phase, started_at, pid, heartbeat, mode)
         VALUES(?1, ?2, ?3, 'running', 'preparing', ?4, ?5, ?4, ?6)",
        params![routine_id, name, trigger, t, std::process::id() as i64, mode],
    )
    .map_err(e)?;
    Ok(c.last_insert_rowid())
}

pub fn get_run(c: &Connection, id: i64) -> R<Option<Run>> {
    c.query_row(&format!("SELECT {RUN_COLS} FROM runs WHERE id=?1"), [id], row_to_run)
        .optional()
        .map_err(e)
}

pub fn list_runs(c: &Connection, routine_id: Option<&str>, status: Option<&str>, limit: i64) -> R<Vec<Run>> {
    let mut sql = format!("SELECT {RUN_COLS} FROM runs WHERE 1=1");
    let mut args: Vec<String> = vec![];
    if let Some(r) = routine_id {
        args.push(r.to_string());
        sql += &format!(" AND routine_id=?{}", args.len());
    }
    if let Some(s) = status {
        args.push(s.to_string());
        sql += &format!(" AND status=?{}", args.len());
    }
    sql += &format!(" ORDER BY id DESC LIMIT {}", limit.max(1));
    let mut st = c.prepare(&sql).map_err(e)?;
    let rows = st
        .query_map(rusqlite::params_from_iter(args.iter()), row_to_run)
        .map_err(e)?
        .filter_map(|x| x.ok())
        .collect();
    Ok(rows)
}

pub fn active_runs(c: &Connection) -> R<Vec<Run>> {
    list_runs(c, None, Some("running"), 50)
}

pub struct Progress<'a> {
    pub phase: &'a str,
    pub total_files: i64,
    pub total_bytes: i64,
    pub done_files: i64,
    pub done_bytes: i64,
    pub current_file: Option<&'a str>,
    pub failed_files: i64,
    pub speed_bps: f64,
}

pub fn update_progress(c: &Connection, id: i64, p: &Progress) -> R<()> {
    c.execute(
        "UPDATE runs SET phase=?2, total_files=?3, total_bytes=?4, done_files=?5, done_bytes=?6,
         current_file=?7, failed_files=?8, speed_bps=?9, heartbeat=?10 WHERE id=?1",
        params![
            id,
            p.phase,
            p.total_files,
            p.total_bytes,
            p.done_files,
            p.done_bytes,
            p.current_file,
            p.failed_files,
            p.speed_bps,
            now()
        ],
    )
    .map_err(e)?;
    Ok(())
}

pub fn set_phase(c: &Connection, id: i64, phase: &str) -> R<()> {
    c.execute("UPDATE runs SET phase=?2, heartbeat=?3 WHERE id=?1", params![id, phase, now()])
        .map_err(e)?;
    Ok(())
}

pub fn set_run_paths(c: &Connection, id: i64, dest_folder: &str, log_path: &str) -> R<()> {
    c.execute(
        "UPDATE runs SET dest_folder=?2, log_path=?3 WHERE id=?1",
        params![id, dest_folder, log_path],
    )
    .map_err(e)?;
    Ok(())
}

pub struct Finish<'a> {
    pub status: &'a str,
    pub exit_code: Option<i64>,
    pub message: &'a str,
    pub details: &'a [String],
    pub done_files: i64,
    pub done_bytes: i64,
    pub failed_files: i64,
    pub skipped_files: i64,
    pub extra_files: i64,
}

pub fn finish_run(c: &Connection, id: i64, f: &Finish) -> R<()> {
    let det = serde_json::to_string(f.details).unwrap_or_else(|_| "[]".into());
    c.execute(
        "UPDATE runs SET status=?2, phase='done', finished_at=?3, exit_code=?4, message=?5, details=?6,
         done_files=?7, done_bytes=?8, failed_files=?9, skipped_files=?10, extra_files=?11,
         current_file=NULL, speed_bps=0, heartbeat=?3 WHERE id=?1",
        params![
            id,
            f.status,
            now(),
            f.exit_code,
            f.message,
            det,
            f.done_files,
            f.done_bytes,
            f.failed_files,
            f.skipped_files,
            f.extra_files
        ],
    )
    .map_err(e)?;
    Ok(())
}

pub fn request_cancel(c: &Connection, id: i64) -> R<()> {
    c.execute("UPDATE runs SET cancel_requested=1 WHERE id=?1", [id]).map_err(e)?;
    Ok(())
}

pub fn is_cancel_requested(c: &Connection, id: i64) -> bool {
    c.query_row("SELECT cancel_requested FROM runs WHERE id=?1", [id], |r| r.get::<_, i64>(0))
        .map(|v| v != 0)
        .unwrap_or(false)
}

pub fn mark_pruned(c: &Connection, id: i64) -> R<()> {
    c.execute("UPDATE runs SET pruned=1 WHERE id=?1", [id]).map_err(e)?;
    Ok(())
}

/// Execuções "travadas" (processo morreu sem finalizar) viram falha.
pub fn fix_stale_runs(c: &Connection) -> R<()> {
    let limit = (chrono::Local::now() - chrono::Duration::seconds(90))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    c.execute(
        "UPDATE runs SET status='failed', phase='done', finished_at=heartbeat,
         message='O backup foi interrompido (o computador desligou ou o programa foi fechado).'
         WHERE status='running' AND (heartbeat IS NULL OR heartbeat < ?1)",
        [limit],
    )
    .map_err(e)?;
    Ok(())
}

/// Execuções terminadas que ainda não geraram notificação.
pub fn take_unnotified(c: &Connection) -> R<Vec<Run>> {
    let runs: Vec<Run> = {
        let mut st = c
            .prepare(&format!(
                "SELECT {RUN_COLS} FROM runs WHERE status!='running' AND notified=0 ORDER BY id"
            ))
            .map_err(e)?;
        let v = st
            .query_map([], row_to_run)
            .map_err(e)?
            .filter_map(|x| x.ok())
            .collect();
        v
    };
    c.execute("UPDATE runs SET notified=1 WHERE status!='running' AND notified=0", [])
        .map_err(e)?;
    Ok(runs)
}

/// Versões (pastas com data) ainda existentes de uma rotina, mais novas primeiro.
pub fn version_runs(c: &Connection, routine_id: &str) -> R<Vec<Run>> {
    let mut st = c
        .prepare(&format!(
            "SELECT {RUN_COLS} FROM runs WHERE routine_id=?1 AND mode='versions' AND trigger!='restore'
             AND dest_folder IS NOT NULL AND pruned=0 AND status!='running' ORDER BY id DESC"
        ))
        .map_err(e)?;
    let v = st
        .query_map([routine_id], row_to_run)
        .map_err(e)?
        .filter_map(|x| x.ok())
        .collect();
    Ok(v)
}

pub fn last_finished(c: &Connection, routine_id: &str) -> R<Option<Run>> {
    c.query_row(
        &format!(
            "SELECT {RUN_COLS} FROM runs WHERE routine_id=?1 AND status!='running' AND trigger!='restore'
             ORDER BY id DESC LIMIT 1"
        ),
        [routine_id],
        row_to_run,
    )
    .optional()
    .map_err(e)
}

pub fn last_success(c: &Connection, routine_id: &str) -> R<Option<Run>> {
    c.query_row(
        &format!(
            "SELECT {RUN_COLS} FROM runs WHERE routine_id=?1 AND status IN ('success','warning') AND trigger!='restore'
             ORDER BY id DESC LIMIT 1"
        ),
        [routine_id],
        row_to_run,
    )
    .optional()
    .map_err(e)
}

pub fn delete_runs_of(c: &Connection, routine_id: &str) -> R<()> {
    c.execute("DELETE FROM runs WHERE routine_id=?1", [routine_id]).map_err(e)?;
    Ok(())
}
