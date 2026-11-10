//! Execução de um backup ou restauração (processo sem janela).

use crate::db::{self, Finish, Progress};
use crate::models::{DestKind, Mode, Options, Routine};
use crate::robocopy::{self, ArgSpec, Line, LineDecoder, Summary};
use crate::{paths, secrets, win};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

// ---------------- Destino ----------------

/// Descobre o caminho real do destino (HD externo pode ter mudado de letra).
pub fn resolve_dest(r: &Routine) -> Result<String, String> {
    let dest = r.dest_path.trim().to_string();
    if r.dest_kind == DestKind::Usb {
        if let (Some(serial), Some(root)) = (r.dest_volume_serial, win::drive_root(&dest)) {
            let ok = win::volume_info(&root).map(|v| v.serial == serial).unwrap_or(false);
            if !ok {
                match win::find_volume_by_serial(serial) {
                    Some(v) => return Ok(format!("{}{}", v.root, &dest[3.min(dest.len())..])),
                    None => {
                        let label = r.dest_volume_label.clone().filter(|l| !l.is_empty()).unwrap_or_else(|| "de backup".into());
                        return Err(format!(
                            "O HD externo \"{label}\" não está conectado. Conecte-o no computador e tente de novo."
                        ));
                    }
                }
            }
        }
    }
    Ok(dest)
}

/// Conexão de rede aberta durante o backup (desconecta ao sair).
pub struct NetGuard {
    path: Option<String>,
}

impl Drop for NetGuard {
    fn drop(&mut self) {
        if let Some(p) = &self.path {
            win::net_disconnect(p);
        }
    }
}

pub fn connect_dest(r: &Routine, dest: &str, password_override: Option<&str>) -> Result<NetGuard, String> {
    if !dest.starts_with("\\\\") {
        return Ok(NetGuard { path: None });
    }
    let password = match password_override {
        Some(p) => Some(p.to_string()),
        None if r.has_password => secrets::load(&r.id)?,
        None => None,
    };
    let user = r.net_user.as_deref().filter(|u| !u.trim().is_empty());
    if user.is_none() && password.is_none() {
        return Ok(NetGuard { path: None });
    }
    win::net_connect(dest, user, password.as_deref())?;
    Ok(NetGuard { path: Some(dest.to_string()) })
}

pub fn ensure_dir(path: &str) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| {
        if e.raw_os_error() == Some(5) {
            format!("Sem permissão para gravar em \"{path}\".")
        } else {
            format!("Não consegui acessar ou criar a pasta \"{path}\": {e}")
        }
    })
}

/// Raiz do backup desta rotina dentro do destino.
pub fn routine_base(dest: &str, r: &Routine) -> String {
    paths::join_win(dest, &paths::sanitize_name(&r.name))
}

/// Pasta de uma versão gravada antes (ajusta a letra do disco se ela mudou).
pub fn remap_version(base_now: &str, stored: &str) -> String {
    let leaf = stored.trim_end_matches('\\').rsplit('\\').next().unwrap_or(stored);
    paths::join_win(base_now, leaf)
}

fn is_version_folder_name(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 15 && b[4] == b'-' && b[7] == b'-' && b[10] == b'_' && b[..4].iter().all(|c| c.is_ascii_digit())
}

fn fmt_bytes(b: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", v, units[i]).replace('.', ",")
}

// ---------------- Execução do robocopy ----------------

#[derive(Default)]
struct CopyState {
    total_files: i64,
    total_bytes: i64,
    done_files: i64,
    done_bytes: i64,
    extra_files: i64,
    failed_seen: i64,
    current: String,
    errors: Vec<String>,
    seen_err: HashSet<String>,
    speed: f64,
    canceled: bool,
    // somatório dos resumos finais
    sum_copied: i64,
    sum_skipped: i64,
    sum_failed: i64,
    sum_extras: i64,
    sum_bytes_copied: i64,
    all_summaries: bool,
}

fn exec_robocopy(conn: &Connection, run_id: i64, phase: &str, args: &[String], st: &mut CopyState) -> Result<(i32, Summary), String> {
    let mut child = win::hidden_command("robocopy.exe")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("Não foi possível iniciar o Robocopy: {e}"))?;
    let mut out = child.stdout.take().ok_or("Saída do Robocopy indisponível")?;
    let (tx, rx) = mpsc::channel::<Vec<String>>();
    std::thread::spawn(move || {
        let mut dec = LineDecoder::new();
        let mut buf = [0u8; 16384];
        loop {
            match out.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(dec.finish());
                    break;
                }
                Ok(n) => {
                    let lines = dec.push(&buf[..n]);
                    if !lines.is_empty() && tx.send(lines).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut summary = Summary::default();
    let mut last_tick = Instant::now();
    let mut bytes_at_tick = st.done_bytes;
    loop {
        match rx.recv_timeout(Duration::from_millis(400)) {
            Ok(lines) => {
                for l in lines {
                    match robocopy::parse_line(&l) {
                        Line::File { size, path, extra } => {
                            if extra {
                                st.extra_files += 1;
                            } else {
                                st.done_files += 1;
                                st.done_bytes += size as i64;
                                st.current = path;
                            }
                        }
                        Line::Error { code, text } => {
                            let p = text
                                .find(":\\")
                                .map(|i| i.saturating_sub(1))
                                .or_else(|| text.find("\\\\"))
                                .map(|i| text[i..].to_string())
                                .unwrap_or(text.clone());
                            let msg = format!("{}: {}", robocopy::win_error_text(code), p);
                            if st.seen_err.insert(msg.clone()) {
                                st.failed_seen += 1;
                                if st.errors.len() < 300 {
                                    st.errors.push(msg);
                                }
                            }
                        }
                        Line::Summary(row) => summary.push(row),
                        Line::Other(_) => {}
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if last_tick.elapsed() >= Duration::from_secs(1) {
            let secs = last_tick.elapsed().as_secs_f64();
            let inst = (st.done_bytes - bytes_at_tick).max(0) as f64 / secs;
            st.speed = if st.speed == 0.0 { inst } else { st.speed * 0.6 + inst * 0.4 };
            bytes_at_tick = st.done_bytes;
            last_tick = Instant::now();
            let _ = db::update_progress(
                conn,
                run_id,
                &Progress {
                    phase,
                    total_files: st.total_files,
                    total_bytes: st.total_bytes,
                    done_files: st.done_files,
                    done_bytes: st.done_bytes,
                    current_file: if st.current.is_empty() { None } else { Some(&st.current) },
                    failed_files: st.failed_seen,
                    speed_bps: st.speed,
                },
            );
            if db::is_cancel_requested(conn, run_id) {
                st.canceled = true;
                let _ = child.kill();
            }
        }
    }
    let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(16);
    Ok((code, summary))
}

struct Pair {
    src: String,
    dst: String,
    single_file: Option<String>,
}

struct CopyResult {
    code: i32,
    st: CopyState,
}

fn copy_pairs(
    conn: &Connection,
    run_id: i64,
    pairs: &[Pair],
    mode: &Mode,
    opts: &Options,
    log_path: &str,
    exclude_older: bool,
    dest_for_space: &str,
) -> Result<CopyResult, String> {
    let elevated = win::is_elevated();
    let mut st = CopyState { all_summaries: true, ..Default::default() };

    // 1) Pré-varredura: quanto será copiado
    if opts.prescan {
        db::set_phase(conn, run_id, "scanning")?;
        for p in pairs {
            let args = robocopy::build_args(&ArgSpec {
                src: &p.src,
                dst: &p.dst,
                mode,
                opts,
                elevated,
                list_only: true,
                log_path: None,
                exclude_older,
                single_file: p.single_file.as_deref(),
            });
            let (_, sum) = exec_robocopy(conn, run_id, "scanning", &args, &mut st)?;
            if st.canceled {
                return Ok(CopyResult { code: 0, st });
            }
            if sum.complete() {
                st.total_files += sum.files[1] as i64;
                st.total_bytes += sum.bytes[1] as i64;
            }
        }
        let _ = db::update_progress(
            conn,
            run_id,
            &Progress {
                phase: "copying",
                total_files: st.total_files,
                total_bytes: st.total_bytes,
                done_files: 0,
                done_bytes: 0,
                current_file: None,
                failed_files: 0,
                speed_bps: 0.0,
            },
        );
        if let Some((free, _)) = win::disk_space(dest_for_space) {
            if st.total_bytes as u64 > free {
                return Err(format!(
                    "Espaço insuficiente no destino: o backup precisa de {} e só há {} livres. Libere espaço ou use outro disco.",
                    fmt_bytes(st.total_bytes as u64),
                    fmt_bytes(free)
                ));
            }
        }
    }

    // 2) Cópia
    let mut code_all = 0i32;
    for p in pairs {
        ensure_dir(&p.dst)?;
        let args = robocopy::build_args(&ArgSpec {
            src: &p.src,
            dst: &p.dst,
            mode,
            opts,
            elevated,
            list_only: false,
            log_path: Some(log_path),
            exclude_older,
            single_file: p.single_file.as_deref(),
        });
        let (code, sum) = exec_robocopy(conn, run_id, "copying", &args, &mut st)?;
        if st.canceled {
            return Ok(CopyResult { code: code_all, st });
        }
        code_all |= code;
        if sum.complete() {
            st.sum_copied += sum.files[1] as i64;
            st.sum_skipped += sum.files[2] as i64;
            st.sum_failed += sum.files[4] as i64;
            st.sum_extras += sum.files[5] as i64;
            st.sum_bytes_copied += sum.bytes[1] as i64;
        } else {
            st.all_summaries = false;
        }
    }
    Ok(CopyResult { code: code_all, st })
}

fn finish_from(conn: &Connection, run_id: i64, res: &CopyResult, extra_details: &[String], extra_msg: Option<&str>) {
    let st = &res.st;
    let (copied, skipped, failed, extras, bytes) = if st.all_summaries {
        (st.sum_copied, st.sum_skipped, st.sum_failed.max(0), st.sum_extras, st.sum_bytes_copied)
    } else {
        (st.done_files, 0, st.failed_seen, st.extra_files, st.done_bytes)
    };
    let mut details: Vec<String> = extra_details.to_vec();
    details.extend(st.errors.iter().cloned());
    if st.canceled {
        let _ = db::finish_run(
            conn,
            run_id,
            &Finish {
                status: "canceled",
                exit_code: None,
                message: "Backup interrompido a pedido do usuário. Os arquivos já copiados continuam no destino.",
                details: &details,
                done_files: st.done_files,
                done_bytes: st.done_bytes,
                failed_files: st.failed_seen,
                skipped_files: 0,
                extra_files: st.extra_files,
            },
        );
        return;
    }
    let out = robocopy::interpret_exit(res.code, copied as u64, failed as u64);
    let mut status = out.status;
    let mut msg = out.message;
    if let Some(m) = extra_msg {
        msg = format!("{msg} {m}");
        if status == "success" {
            status = "warning";
        }
    }
    let _ = db::finish_run(
        conn,
        run_id,
        &Finish {
            status,
            exit_code: Some(res.code as i64),
            message: &msg,
            details: &details,
            done_files: copied,
            done_bytes: bytes,
            failed_files: failed,
            skipped_files: skipped,
            extra_files: extras,
        },
    );
}

fn fail(conn: &Connection, run_id: i64, msg: &str, details: &[String]) {
    let _ = db::finish_run(
        conn,
        run_id,
        &Finish {
            status: "failed",
            exit_code: None,
            message: msg,
            details,
            done_files: 0,
            done_bytes: 0,
            failed_files: 0,
            skipped_files: 0,
            extra_files: 0,
        },
    );
}

fn log_path_for(run_id: i64) -> String {
    let _ = std::fs::create_dir_all(paths::logs_dir());
    paths::logs_dir().join(format!("{run_id}.log")).to_string_lossy().to_string()
}

// ---------------- Backup ----------------

pub fn run_backup(routine_id: &str, trigger_arg: Option<&str>) -> i32 {
    let conn = match db::open() {
        Ok(c) => c,
        Err(_) => return 2,
    };
    let _ = db::fix_stale_runs(&conn);
    let r = match db::get_routine(&conn, routine_id) {
        Ok(Some(r)) => r,
        _ => return 3,
    };
    // Já está rodando? Não duplica.
    if let Ok(active) = db::active_runs(&conn) {
        if active.iter().any(|x| x.routine_id == r.id) {
            return 0;
        }
    }
    let key = format!("manual_request:{}", r.id);
    let trigger = match trigger_arg {
        Some(t) => t.to_string(),
        None => {
            let manual = db::get_setting(&conn, &key)
                .ok()
                .flatten()
                .and_then(|t| chrono::NaiveDateTime::parse_from_str(&t, "%Y-%m-%dT%H:%M:%S").ok())
                .map(|t| (chrono::Local::now().naive_local() - t).num_seconds() < 180)
                .unwrap_or(false);
            if manual {
                "manual".into()
            } else {
                "schedule".into()
            }
        }
    };
    let _ = db::del_setting(&conn, &key);
    let mode_s = match r.mode {
        Mode::Copy => "copy",
        Mode::Mirror => "mirror",
        Mode::Versions => "versions",
    };
    let run_id = match db::create_run(&conn, &r.id, &r.name, &trigger, mode_s) {
        Ok(id) => id,
        Err(_) => return 2,
    };

    // Heartbeat em paralelo (para fases longas sem saída).
    let hb_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let stop = hb_stop.clone();
        std::thread::spawn(move || {
            if let Ok(c) = db::open() {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = c.execute("UPDATE runs SET heartbeat=?2 WHERE id=?1 AND status='running'", rusqlite::params![run_id, db::now()]);
                    std::thread::sleep(Duration::from_secs(20));
                }
            }
        });
    }

    let result = do_backup(&conn, run_id, &r);
    hb_stop.store(true, std::sync::atomic::Ordering::Relaxed);
    match result {
        Ok(()) => 0,
        Err(msg) => {
            fail(&conn, run_id, &msg, &[]);
            1
        }
    }
}

fn do_backup(conn: &Connection, run_id: i64, r: &Routine) -> Result<(), String> {
    if r.sources.is_empty() {
        return Err("Esta rotina não tem nenhuma pasta para copiar.".into());
    }
    // 1) Origens
    let leaves = paths::source_leaves(&r.sources);
    let mut pairs_src: Vec<(String, String)> = vec![];
    let mut warn: Vec<String> = vec![];
    for (s, leaf) in r.sources.iter().zip(leaves.iter()) {
        if Path::new(s).exists() {
            pairs_src.push((s.clone(), leaf.clone()));
        } else {
            warn.push(format!("Pasta de origem não encontrada (foi ignorada): {s}"));
        }
    }
    if pairs_src.is_empty() {
        return Err("Nenhuma das pastas de origem foi encontrada. Elas podem ter sido movidas ou apagadas.".into());
    }

    // 2) Destino
    let dest = resolve_dest(r)?;
    let _net = connect_dest(r, &dest, None)?;
    if !Path::new(&dest).exists() {
        if dest.starts_with("\\\\") {
            return Err(format!("Não consegui acessar a pasta de rede \"{dest}\". Verifique se o servidor/NAS está ligado e se a rede está funcionando."));
        }
        if r.dest_kind == DestKind::Usb && win::drive_root(&dest).map(|d| !Path::new(&d).exists()).unwrap_or(false) {
            return Err("O HD externo não está conectado. Conecte-o e tente de novo.".into());
        }
    }
    let base = routine_base(&dest, r);
    let target_root = if r.mode == Mode::Versions {
        let now = chrono::Local::now();
        let mut t = paths::join_win(&base, &now.format("%Y-%m-%d_%H%M").to_string());
        if Path::new(&t).exists() {
            t = paths::join_win(&base, &now.format("%Y-%m-%d_%H%M%S").to_string());
        }
        t
    } else {
        base.clone()
    };
    ensure_dir(&target_root)?;
    let log = log_path_for(run_id);
    db::set_run_paths(conn, run_id, &target_root, &log)?;

    let pairs: Vec<Pair> = pairs_src
        .iter()
        .map(|(s, leaf)| Pair { src: s.clone(), dst: paths::join_win(&target_root, leaf), single_file: None })
        .collect();

    // 3) Cópia
    let res = copy_pairs(conn, run_id, &pairs, &r.mode, &r.options, &log, false, &dest)?;
    let ok = !res.st.canceled && res.code < 8;

    // 4) Limpeza de versões antigas
    if r.mode == Mode::Versions && !res.st.canceled {
        db::set_phase(conn, run_id, "cleaning")?;
        prune_versions(conn, r, &base, if ok { 1 } else { 0 }, &mut warn);
    }

    let extra = if warn.iter().any(|w| w.starts_with("Pasta de origem")) {
        Some("Atenção: alguma pasta de origem não foi encontrada.")
    } else {
        None
    };
    finish_from(conn, run_id, &res, &warn, extra);
    Ok(())
}

/// Mantém as N versões bem-sucedidas mais novas; apaga as demais (só pastas criadas pelo programa).
fn prune_versions(conn: &Connection, r: &Routine, base_now: &str, current_counts: u32, warn: &mut Vec<String>) {
    let keep = r.keep_versions.max(1);
    let mut kept = current_counts;
    let versions = match db::version_runs(conn, &r.id) {
        Ok(v) => v,
        Err(_) => return,
    };
    let base_lower = base_now.to_lowercase();
    for v in versions {
        let good = v.status == "success" || v.status == "warning";
        if good && kept < keep {
            kept += 1;
            continue;
        }
        let Some(stored) = v.dest_folder.as_deref() else { continue };
        let folder = remap_version(base_now, stored);
        let leaf = folder.rsplit('\\').next().unwrap_or("");
        if !folder.to_lowercase().starts_with(&base_lower) || !is_version_folder_name(leaf) {
            continue;
        }
        if !Path::new(&folder).exists() {
            let _ = db::mark_pruned(conn, v.id);
            continue;
        }
        match std::fs::remove_dir_all(&folder) {
            Ok(()) => {
                let _ = db::mark_pruned(conn, v.id);
            }
            Err(e) => warn.push(format!("Não consegui apagar a versão antiga {folder}: {e}")),
        }
    }
}

// ---------------- Restauração ----------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreSpec {
    pub routine_id: String,
    /// Execução (versão) escolhida. None = backup atual / versão mais recente.
    pub run_id: Option<i64>,
    /// Caminho dentro do backup (pasta ou arquivo) para restaurar só uma parte. None = tudo.
    pub inner_path: Option<String>,
    /// "original" ou "folder"
    pub target: String,
    pub folder: Option<String>,
    /// Não substituir arquivos que estejam mais novos no destino.
    pub keep_newer: bool,
}

/// Pasta do backup que será usada na restauração.
pub fn backup_root_for(conn: &Connection, r: &Routine, dest: &str, run_id: Option<i64>) -> Result<String, String> {
    let base = routine_base(dest, r);
    if r.mode != Mode::Versions {
        return Ok(base);
    }
    let run = match run_id {
        Some(id) => db::get_run(conn, id)?,
        None => db::version_runs(conn, &r.id)?.into_iter().find(|v| v.status != "failed"),
    };
    let run = run.ok_or("Nenhuma versão de backup disponível para restaurar.")?;
    let stored = run.dest_folder.ok_or("Esta execução não tem pasta de backup.")?;
    Ok(remap_version(&base, &stored))
}

pub fn run_restore(spec_file: &str) -> i32 {
    let spec: RestoreSpec = match std::fs::read_to_string(spec_file).ok().and_then(|s| serde_json::from_str(&s).ok()) {
        Some(s) => s,
        None => return 4,
    };
    let _ = std::fs::remove_file(spec_file);
    let conn = match db::open() {
        Ok(c) => c,
        Err(_) => return 2,
    };
    let r = match db::get_routine(&conn, &spec.routine_id) {
        Ok(Some(r)) => r,
        _ => return 3,
    };
    let run_id = match db::create_run(&conn, &r.id, &format!("Restauração: {}", r.name), "restore", "copy") {
        Ok(id) => id,
        Err(_) => return 2,
    };
    match do_restore(&conn, run_id, &r, &spec) {
        Ok(()) => 0,
        Err(msg) => {
            fail(&conn, run_id, &msg, &[]);
            1
        }
    }
}

fn do_restore(conn: &Connection, run_id: i64, r: &Routine, spec: &RestoreSpec) -> Result<(), String> {
    let dest = resolve_dest(r)?;
    let _net = connect_dest(r, &dest, None)?;
    let root = backup_root_for(conn, r, &dest, spec.run_id)?;
    if !Path::new(&root).exists() {
        return Err(format!("A pasta do backup não foi encontrada: {root}"));
    }
    let leaves = paths::source_leaves(&r.sources);
    let target_folder = spec.folder.clone().unwrap_or_default();
    if spec.target != "original" && target_folder.trim().is_empty() {
        return Err("Escolha a pasta onde os arquivos serão restaurados.".into());
    }
    let mut pairs: Vec<Pair> = vec![];
    for (src, leaf) in r.sources.iter().zip(leaves.iter()) {
        let from_base = paths::join_win(&root, leaf);
        let to_base = if spec.target == "original" { src.clone() } else { paths::join_win(&target_folder, leaf) };
        match &spec.inner_path {
            None => {
                if Path::new(&from_base).exists() {
                    pairs.push(Pair { src: from_base, dst: to_base, single_file: None });
                }
            }
            Some(inner) => {
                let inner_l = inner.trim_end_matches('\\').to_lowercase();
                let fb_l = from_base.to_lowercase();
                if inner_l == fb_l || inner_l.starts_with(&(fb_l.clone() + "\\")) {
                    let rel = inner.trim_end_matches('\\')[from_base.len()..].trim_start_matches('\\').to_string();
                    let p = Path::new(inner);
                    if p.is_file() {
                        let parent_rel = Path::new(&rel).parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                        let fname = p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                        let src_dir = p.parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                        let dst_dir = if parent_rel.is_empty() { to_base.clone() } else { paths::join_win(&to_base, &parent_rel) };
                        pairs.push(Pair { src: src_dir, dst: dst_dir, single_file: Some(fname) });
                    } else {
                        let dst = if rel.is_empty() { to_base.clone() } else { paths::join_win(&to_base, &rel) };
                        pairs.push(Pair { src: inner.trim_end_matches('\\').to_string(), dst, single_file: None });
                    }
                }
            }
        }
    }
    if pairs.is_empty() {
        return Err("Não encontrei esses arquivos dentro do backup.".into());
    }
    let log = log_path_for(run_id);
    let first_dst = pairs[0].dst.clone();
    db::set_run_paths(conn, run_id, &first_dst, &log)?;
    let space_check = win::drive_root(&first_dst).unwrap_or(first_dst.clone());
    let res = copy_pairs(conn, run_id, &pairs, &Mode::Copy, &r.options, &log, spec.keep_newer, &space_check)?;
    finish_from(conn, run_id, &res, &[], None);
    // Mensagem própria para restauração
    if !res.st.canceled && res.code < 8 {
        let _ = conn.execute(
            "UPDATE runs SET message=?2 WHERE id=?1",
            rusqlite::params![run_id, format!("Restauração concluída em \"{}\".", if spec.target == "original" { "local original".to_string() } else { target_folder })],
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_names() {
        assert!(is_version_folder_name("2026-11-10_0300"));
        assert!(is_version_folder_name("2026-11-10_030015"));
        assert!(!is_version_folder_name("Documentos"));
        assert_eq!(remap_version(r"F:\Backup\Pousada", r"E:\Backup\Pousada\2026-11-10_0300"), r"F:\Backup\Pousada\2026-11-10_0300");
    }
}
