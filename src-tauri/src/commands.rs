//! Comandos chamados pela interface (React).

use crate::elevation::{self, AdminOp};
use crate::models::{DestKind, Routine, Run};
use crate::runner::{self, RestoreSpec};
use crate::{db, paths, scheduler, secrets, win};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub type CmdResult<T> = Result<T, String>;

#[derive(Default)]
pub struct AppState {
    pub unlocked_until: Mutex<Option<Instant>>,
}

fn conn() -> CmdResult<rusqlite::Connection> {
    db::open()
}

fn pin_hash(conn: &rusqlite::Connection) -> Option<String> {
    db::get_setting(conn, "pin_hash").ok().flatten().filter(|s| !s.is_empty())
}

fn hash_pin(salt: &str, pin: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b":");
    h.update(pin.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn verify_pin(conn: &rusqlite::Connection, pin: &str) -> bool {
    match pin_hash(conn) {
        None => true,
        Some(stored) => {
            let mut it = stored.splitn(2, ':');
            let salt = it.next().unwrap_or("");
            let hash = it.next().unwrap_or("");
            hash_pin(salt, pin) == hash
        }
    }
}

fn is_unlocked(state: &AppState, conn: &rusqlite::Connection) -> bool {
    if pin_hash(conn).is_none() {
        return true;
    }
    state.unlocked_until.lock().unwrap().map(|t| Instant::now() < t).unwrap_or(false)
}

fn require_unlocked(state: &AppState, conn: &rusqlite::Connection) -> CmdResult<()> {
    if is_unlocked(state, conn) {
        Ok(())
    } else {
        Err("PIN_REQUIRED".into())
    }
}

// ---------------- Estado geral ----------------

#[derive(Serialize)]
pub struct Status {
    initialized: bool,
    elevated: bool,
    exe_mismatch: bool,
    has_pin: bool,
    unlocked: bool,
    data_dir: String,
    computer: String,
}

#[tauri::command]
pub fn get_status(state: tauri::State<AppState>) -> CmdResult<Status> {
    let c = conn()?;
    let initialized = db::get_setting(&c, "initialized")?.as_deref() == Some("1");
    let exe = db::get_setting(&c, "exe_path")?;
    let has_routines = !db::list_routines(&c)?.is_empty();
    let exe_mismatch = has_routines
        && exe.map(|e| !e.eq_ignore_ascii_case(&paths::current_exe())).unwrap_or(false);
    Ok(Status {
        initialized,
        elevated: win::is_elevated(),
        exe_mismatch,
        has_pin: pin_hash(&c).is_some(),
        unlocked: is_unlocked(&state, &c),
        data_dir: paths::data_dir().to_string_lossy().to_string(),
        computer: std::env::var("COMPUTERNAME").unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn setup_computer() -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(|| elevation::run_ops(vec![AdminOp::Setup]))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn repair_tasks(state: tauri::State<'_, AppState>) -> CmdResult<()> {
    require_unlocked(&state, &conn()?)?;
    tauri::async_runtime::spawn_blocking(|| elevation::run_ops(vec![AdminOp::RepairAll]))
        .await
        .map_err(|e| e.to_string())?
}

// ---------------- Rotinas ----------------

#[derive(Serialize)]
pub struct RoutineView {
    routine: Routine,
    last_run: Option<Run>,
    last_success: Option<Run>,
    running: Option<Run>,
}

#[tauri::command]
pub fn list_routines() -> CmdResult<Vec<RoutineView>> {
    let c = conn()?;
    let _ = db::fix_stale_runs(&c);
    let active = db::active_runs(&c)?;
    let mut out = vec![];
    for r in db::list_routines(&c)? {
        let running = active.iter().find(|a| a.routine_id == r.id && a.trigger != "restore").cloned();
        out.push(RoutineView {
            last_run: db::last_finished(&c, &r.id)?,
            last_success: db::last_success(&c, &r.id)?,
            running,
            routine: r,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn get_routine(id: String) -> CmdResult<Option<Routine>> {
    db::get_routine(&conn()?, &id)
}

#[tauri::command]
pub fn check_tasks() -> CmdResult<HashMap<String, bool>> {
    let c = conn()?;
    let mut m = HashMap::new();
    for r in db::list_routines(&c)? {
        m.insert(r.id.clone(), scheduler::exists(&r.id));
    }
    Ok(m)
}

#[tauri::command]
pub async fn save_routine(
    state: tauri::State<'_, AppState>,
    mut routine: Routine,
    password: Option<String>,
    clear_password: bool,
) -> CmdResult<Routine> {
    let c = conn()?;
    require_unlocked(&state, &c)?;
    routine.name = routine.name.trim().to_string();
    if routine.name.is_empty() {
        return Err("Dê um nome para a rotina.".into());
    }
    routine.sources.retain(|s| !s.trim().is_empty());
    if routine.sources.is_empty() {
        return Err("Escolha pelo menos uma pasta para copiar.".into());
    }
    if routine.dest_path.trim().is_empty() {
        return Err("Escolha para onde o backup vai.".into());
    }
    let dest_l = routine.dest_path.trim_end_matches('\\').to_lowercase();
    for s in &routine.sources {
        let s_l = s.trim_end_matches('\\').to_lowercase();
        if dest_l == s_l || dest_l.starts_with(&(s_l.clone() + "\\")) {
            return Err(format!("O destino não pode ficar dentro da pasta que está sendo copiada ({s})."));
        }
    }
    if routine.dest_kind == DestKind::Network && !routine.dest_path.starts_with("\\\\") {
        return Err("Endereço de rede deve começar com \\\\ (ex.: \\\\NAS\\Backup).".into());
    }
    if routine.id.is_empty() {
        routine.id = uuid::Uuid::new_v4().to_string();
        routine.created_at = db::now();
    }
    routine.updated_at = db::now();
    if routine.dest_kind == DestKind::Usb {
        if let Some(root) = win::drive_root(&routine.dest_path) {
            if let Some(v) = win::volume_info(&root) {
                routine.dest_volume_label = Some(v.label);
                routine.dest_volume_serial = Some(v.serial);
            }
        }
    } else {
        routine.dest_volume_label = None;
        routine.dest_volume_serial = None;
    }
    let mut ops = vec![];
    let pw = password.filter(|p| !p.is_empty());
    if let Some(p) = &pw {
        routine.has_password = true;
        ops.push(AdminOp::SaveSecret { routine_id: routine.id.clone(), password: p.clone() });
    } else if clear_password || routine.dest_kind != DestKind::Network {
        if routine.has_password {
            ops.push(AdminOp::DeleteSecret { routine_id: routine.id.clone() });
        }
        routine.has_password = false;
    }
    db::save_routine(&c, &routine)?;
    ops.push(AdminOp::RegisterTask { routine_id: routine.id.clone() });
    let r2 = routine.clone();
    tauri::async_runtime::spawn_blocking(move || elevation::run_ops(ops))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("A rotina foi salva, mas o agendamento não foi criado: {e}"))?;
    Ok(r2)
}

#[tauri::command]
pub async fn set_routine_enabled(state: tauri::State<'_, AppState>, id: String, enabled: bool) -> CmdResult<()> {
    let c = conn()?;
    require_unlocked(&state, &c)?;
    let mut r = db::get_routine(&c, &id)?.ok_or("Rotina não encontrada.")?;
    r.enabled = enabled;
    r.updated_at = db::now();
    db::save_routine(&c, &r)?;
    tauri::async_runtime::spawn_blocking(move || elevation::run_ops(vec![AdminOp::RegisterTask { routine_id: id }]))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_routine(state: tauri::State<'_, AppState>, id: String, delete_history: bool) -> CmdResult<()> {
    let c = conn()?;
    require_unlocked(&state, &c)?;
    let id2 = id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        elevation::run_ops(vec![
            AdminOp::DeleteTask { routine_id: id2.clone() },
            AdminOp::DeleteSecret { routine_id: id2 },
        ])
    })
    .await
    .map_err(|e| e.to_string())??;
    db::delete_routine(&c, &id)?;
    if delete_history {
        db::delete_runs_of(&c, &id)?;
    }
    Ok(())
}

// ---------------- Execução ----------------

fn spawn_direct(args: &[&str]) -> CmdResult<()> {
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    std::process::Command::new(paths::current_exe())
        .args(args)
        .creation_flags(win::CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Não foi possível iniciar o backup: {e}"))
}

#[tauri::command]
pub fn run_now(id: String) -> CmdResult<String> {
    let c = conn()?;
    if db::active_runs(&c)?.iter().any(|r| r.routine_id == id && r.trigger != "restore") {
        return Err("Este backup já está em andamento.".into());
    }
    db::set_setting(&c, &format!("manual_request:{id}"), &db::now())?;
    if scheduler::run_now(&id).is_ok() {
        return Ok("scheduler".into());
    }
    db::del_setting(&c, &format!("manual_request:{id}"))?;
    spawn_direct(&["--run", &id, "--trigger", "manual"])?;
    Ok("direct".into())
}

#[tauri::command]
pub fn cancel_run(run_id: i64) -> CmdResult<()> {
    db::request_cancel(&conn()?, run_id)
}

#[tauri::command]
pub fn list_runs(routine_id: Option<String>, status: Option<String>, limit: Option<i64>) -> CmdResult<Vec<Run>> {
    let c = conn()?;
    let _ = db::fix_stale_runs(&c);
    db::list_runs(&c, routine_id.as_deref(), status.as_deref(), limit.unwrap_or(200))
}

#[tauri::command]
pub fn active_runs() -> CmdResult<Vec<Run>> {
    let c = conn()?;
    let _ = db::fix_stale_runs(&c);
    db::active_runs(&c)
}

#[tauri::command]
pub fn get_run(id: i64) -> CmdResult<Option<Run>> {
    db::get_run(&conn()?, id)
}

#[tauri::command]
pub fn read_log(run_id: i64) -> CmdResult<String> {
    let c = conn()?;
    let run = db::get_run(&c, run_id)?.ok_or("Execução não encontrada.")?;
    let path = run.log_path.ok_or("Esta execução não tem registro detalhado.")?;
    let bytes = std::fs::read(&path).map_err(|_| "O arquivo de registro não foi encontrado.".to_string())?;
    // /UNILOG+ grava UTF-8 (às vezes com BOM UTF-16 na frente); UTF-16 "de verdade" também é aceito.
    let text = if bytes.starts_with(&[0xFF, 0xFE]) && bytes.get(3) == Some(&0) {
        let u: Vec<u16> = bytes[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
        String::from_utf16_lossy(&u)
    } else {
        let start = if bytes.starts_with(&[0xFF, 0xFE]) { 2 } else { 0 };
        String::from_utf8_lossy(&bytes[start..]).to_string()
    };
    const MAX: usize = 400_000;
    if text.len() > MAX {
        let mut start = text.len() - MAX;
        while !text.is_char_boundary(start) {
            start += 1;
        }
        Ok(format!("... (início omitido) ...\n{}", &text[start..]))
    } else {
        Ok(text)
    }
}

// ---------------- Destinos ----------------

#[derive(Serialize)]
pub struct DestTest {
    ok: bool,
    message: String,
    free: Option<u64>,
    total: Option<u64>,
    label: Option<String>,
    removable: bool,
}

#[tauri::command]
pub async fn test_destination(
    kind: DestKind,
    path: String,
    user: Option<String>,
    password: Option<String>,
    routine_id: Option<String>,
) -> CmdResult<DestTest> {
    tauri::async_runtime::spawn_blocking(move || test_destination_sync(kind, path, user, password, routine_id))
        .await
        .map_err(|e| e.to_string())
}

fn test_destination_sync(
    kind: DestKind,
    path: String,
    user: Option<String>,
    password: Option<String>,
    routine_id: Option<String>,
) -> DestTest {
    let path = path.trim().to_string();
    let fail = |m: String| DestTest { ok: false, message: m, free: None, total: None, label: None, removable: false };
    if path.is_empty() {
        return fail("Informe o destino.".into());
    }
    let mut connected = false;
    if path.starts_with("\\\\") {
        let mut pw = password.filter(|p| !p.is_empty());
        if pw.is_none() {
            if let Some(id) = &routine_id {
                pw = secrets::load(id).ok().flatten();
            }
        }
        let u = user.filter(|u| !u.trim().is_empty());
        if u.is_some() || pw.is_some() {
            match win::net_connect(&path, u.as_deref(), pw.as_deref()) {
                Ok(_) => connected = true,
                Err(e) => return fail(e),
            }
        }
    } else if kind == DestKind::Network {
        return fail("Endereço de rede deve começar com \\\\ (ex.: \\\\NAS\\Backup).".into());
    }
    let result = (|| {
        let p = std::path::Path::new(&path);
        if !p.exists() {
            if let Some(root) = win::drive_root(&path) {
                if !std::path::Path::new(&root).exists() {
                    return fail(format!("O disco {root} não está conectado."));
                }
            }
            if let Err(e) = std::fs::create_dir_all(p) {
                return fail(format!("Não consegui acessar ou criar a pasta: {e}"));
            }
        }
        let probe = p.join(format!(".robobackup-teste-{}", uuid::Uuid::new_v4()));
        if let Err(e) = std::fs::write(&probe, b"ok") {
            return fail(if e.raw_os_error() == Some(5) {
                "A pasta existe, mas não tenho permissão para gravar nela.".into()
            } else {
                format!("Não consegui gravar na pasta: {e}")
            });
        }
        let _ = std::fs::remove_file(&probe);
        let space = win::disk_space(&path);
        let vol = win::drive_root(&path).and_then(|r| win::volume_info(&r));
        DestTest {
            ok: true,
            message: "Tudo certo! O destino está acessível e pode receber os backups.".into(),
            free: space.map(|s| s.0),
            total: space.map(|s| s.1),
            label: vol.as_ref().map(|v| v.label.clone()),
            removable: vol.map(|v| v.drive_type == 2).unwrap_or(false),
        }
    })();
    if connected {
        win::net_disconnect(&path);
    }
    result
}

#[derive(Serialize)]
pub struct Drive {
    root: String,
    label: String,
    removable: bool,
    free: u64,
    total: u64,
}

#[tauri::command]
pub fn list_drives() -> Vec<Drive> {
    win::list_volumes()
        .into_iter()
        .filter(|v| v.drive_type == 2 || v.drive_type == 3)
        .map(|v| {
            let (free, total) = win::disk_space(&v.root).unwrap_or((0, 0));
            Drive { removable: v.drive_type == 2, root: v.root, label: v.label, free, total }
        })
        .collect()
}

#[derive(Serialize)]
pub struct KnownFolder {
    label: String,
    path: String,
}

#[tauri::command]
pub fn known_folders() -> Vec<KnownFolder> {
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    let mut out = vec![];
    let cands = [
        ("Documentos", "Documents"),
        ("Área de Trabalho", "Desktop"),
        ("Imagens", "Pictures"),
        ("Downloads", "Downloads"),
        ("Vídeos", "Videos"),
        ("Músicas", "Music"),
    ];
    for (label, dir) in cands {
        let mut p = format!("{home}\\{dir}");
        // OneDrive pode ter redirecionado a pasta
        if !std::path::Path::new(&p).exists() {
            if let Ok(od) = std::env::var("OneDrive") {
                let alt = format!("{od}\\{dir}");
                if std::path::Path::new(&alt).exists() {
                    p = alt;
                }
            }
        }
        if std::path::Path::new(&p).exists() {
            out.push(KnownFolder { label: label.into(), path: p });
        }
    }
    out
}

// ---------------- Restauração ----------------

#[tauri::command]
pub fn list_versions(routine_id: String) -> CmdResult<Vec<Run>> {
    let c = conn()?;
    let r = db::get_routine(&c, &routine_id)?.ok_or("Rotina não encontrada.")?;
    if r.mode == crate::models::Mode::Versions {
        Ok(db::version_runs(&c, &routine_id)?.into_iter().filter(|v| v.status != "failed").collect())
    } else {
        Ok(db::last_success(&c, &routine_id)?.into_iter().collect())
    }
}

#[tauri::command]
pub fn backup_location(routine_id: String, run_id: Option<i64>) -> CmdResult<String> {
    let c = conn()?;
    let r = db::get_routine(&c, &routine_id)?.ok_or("Rotina não encontrada.")?;
    let dest = runner::resolve_dest(&r)?;
    runner::backup_root_for(&c, &r, &dest, run_id)
}

#[tauri::command]
pub fn open_path(path: String) -> CmdResult<()> {
    win::open_in_explorer(&path)
}

#[tauri::command]
pub fn start_restore(state: tauri::State<AppState>, spec: RestoreSpec) -> CmdResult<()> {
    let c = conn()?;
    if spec.target == "original" {
        require_unlocked(&state, &c)?;
    }
    let dir = paths::tasks_tmp_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("restore-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(&file, serde_json::to_vec(&spec).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let f = file.to_string_lossy().to_string();
    if win::is_elevated() {
        return spawn_direct(&["--restore", &f]);
    }
    match win::run_elevated(&format!("--restore \"{f}\""), false) {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&file);
            if e == "CANCELADO" {
                Err("Permissão de administrador negada. A restauração não foi iniciada.".into())
            } else {
                Err(e)
            }
        }
    }
}

// ---------------- Configurações / PIN ----------------

#[derive(Serialize)]
pub struct Settings {
    notifications: bool,
    overdue_days: u32,
}

#[tauri::command]
pub fn get_settings() -> CmdResult<Settings> {
    let c = conn()?;
    Ok(Settings {
        notifications: db::get_setting(&c, "notifications")?.as_deref() != Some("0"),
        overdue_days: db::get_setting(&c, "overdue_days")?.and_then(|v| v.parse().ok()).unwrap_or(2),
    })
}

#[tauri::command]
pub fn save_settings(state: tauri::State<AppState>, notifications: bool, overdue_days: u32) -> CmdResult<()> {
    let c = conn()?;
    require_unlocked(&state, &c)?;
    db::set_setting(&c, "notifications", if notifications { "1" } else { "0" })?;
    db::set_setting(&c, "overdue_days", &overdue_days.clamp(1, 60).to_string())
}

#[tauri::command]
pub fn unlock(state: tauri::State<AppState>, pin: String) -> CmdResult<bool> {
    let c = conn()?;
    if verify_pin(&c, &pin) {
        *state.unlocked_until.lock().unwrap() = Some(Instant::now() + Duration::from_secs(15 * 60));
        Ok(true)
    } else {
        std::thread::sleep(Duration::from_millis(800));
        Ok(false)
    }
}

#[tauri::command]
pub fn lock(state: tauri::State<AppState>) {
    *state.unlocked_until.lock().unwrap() = None;
}

#[tauri::command]
pub fn set_pin(state: tauri::State<AppState>, current: String, new_pin: String) -> CmdResult<()> {
    let c = conn()?;
    if !verify_pin(&c, &current) {
        return Err("O PIN atual está incorreto.".into());
    }
    let new_pin = new_pin.trim().to_string();
    if new_pin.is_empty() {
        db::del_setting(&c, "pin_hash")?;
        return Ok(());
    }
    if new_pin.len() < 4 || !new_pin.chars().all(|c| c.is_ascii_digit()) {
        return Err("O PIN deve ter pelo menos 4 números.".into());
    }
    let salt = uuid::Uuid::new_v4().simple().to_string();
    db::set_setting(&c, "pin_hash", &format!("{salt}:{}", hash_pin(&salt, &new_pin)))?;
    *state.unlocked_until.lock().unwrap() = Some(Instant::now() + Duration::from_secs(15 * 60));
    Ok(())
}
