//! Operações que precisam de administrador, agrupadas para pedir o UAC uma vez só.

use crate::{db, paths, scheduler, secrets, win};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum AdminOp {
    Setup,
    RegisterTask { routine_id: String },
    DeleteTask { routine_id: String },
    SaveSecret { routine_id: String, password: String },
    DeleteSecret { routine_id: String },
    RepairAll,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpResult {
    ok: bool,
    error: Option<String>,
}

fn icacls(args: &[&str]) -> Result<(), String> {
    let out = win::hidden_command("icacls.exe")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(crate::robocopy::decode_oem(&out.stdout))
    }
}

fn setup() -> Result<(), String> {
    let base = paths::data_dir();
    for d in [base.clone(), paths::logs_dir(), paths::secrets_dir(), paths::tasks_tmp_dir()] {
        std::fs::create_dir_all(&d).map_err(|e| format!("Erro ao criar {}: {e}", d.display()))?;
    }
    let conn = db::open()?;
    let b = base.to_string_lossy().to_string();
    // Usuários (S-1-5-32-545) podem ler e gravar o banco e os logs.
    icacls(&[&b, "/grant", "*S-1-5-32-545:(OI)(CI)M", "/T", "/C", "/Q"])
        .map_err(|e| format!("Erro ao ajustar permissões: {e}"))?;
    // Senhas: só SYSTEM e Administradores.
    let s = paths::secrets_dir().to_string_lossy().to_string();
    icacls(&[&s, "/inheritance:r", "/grant:r", "*S-1-5-18:(OI)(CI)F", "*S-1-5-32-544:(OI)(CI)F", "/T", "/C", "/Q"])
        .map_err(|e| format!("Erro ao proteger a pasta de senhas: {e}"))?;
    // O /T acima (na pasta base) também adicionou Usuários aqui: remove explicitamente.
    for sid in ["*S-1-5-32-545", "*S-1-5-11", "*S-1-3-0"] {
        icacls(&[&s, "/remove:g", sid, "/T", "/C", "/Q"])
            .map_err(|e| format!("Erro ao proteger a pasta de senhas: {e}"))?;
    }
    db::set_setting(&conn, "initialized", "1")?;
    Ok(())
}

fn execute(op: &AdminOp) -> Result<(), String> {
    match op {
        AdminOp::Setup => setup(),
        AdminOp::RegisterTask { routine_id } => {
            let conn = db::open()?;
            let r = db::get_routine(&conn, routine_id)?.ok_or("Rotina não encontrada.")?;
            scheduler::register(&r)?;
            db::set_setting(&conn, "exe_path", &paths::current_exe())
        }
        AdminOp::DeleteTask { routine_id } => scheduler::delete(routine_id),
        AdminOp::SaveSecret { routine_id, password } => secrets::save(routine_id, password),
        AdminOp::DeleteSecret { routine_id } => {
            secrets::delete(routine_id);
            Ok(())
        }
        AdminOp::RepairAll => {
            setup()?;
            let conn = db::open()?;
            let mut errs = vec![];
            for r in db::list_routines(&conn)? {
                if let Err(e) = scheduler::register(&r) {
                    errs.push(format!("{}: {e}", r.name));
                }
            }
            db::set_setting(&conn, "exe_path", &paths::current_exe())?;
            if errs.is_empty() {
                Ok(())
            } else {
                Err(errs.join("\n"))
            }
        }
    }
}

fn execute_all(ops: &[AdminOp]) -> Result<(), String> {
    for op in ops {
        execute(op)?;
    }
    Ok(())
}

/// Chamado pela GUI. Se já for admin executa direto; senão pede UAC.
pub fn run_ops(ops: Vec<AdminOp>) -> Result<(), String> {
    if ops.is_empty() {
        return Ok(());
    }
    if win::is_elevated() {
        return execute_all(&ops);
    }
    // Pasta compartilhada da máquina: se o UAC pedir a senha de OUTRA conta (usuário padrão),
    // o processo elevado roda com outro %TEMP%, mas enxerga o ProgramData.
    let dir = paths::tasks_tmp_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Erro ao criar pasta temporária: {e}"))?;
    let tmp = dir.join(format!("op-{}.json", uuid::Uuid::new_v4()));
    let res_file = tmp.with_extension("result.json");
    std::fs::write(&tmp, serde_json::to_vec(&ops).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let args = format!("--admin-op \"{}\"", tmp.display());
    let r = win::run_elevated(&args, true);
    let _ = std::fs::remove_file(&tmp);
    match r {
        Err(e) if e == "CANCELADO" => {
            Err("Permissão de administrador negada. A alteração não foi aplicada.".into())
        }
        Err(e) => Err(e),
        Ok(_) => {
            let txt = std::fs::read_to_string(&res_file).unwrap_or_default();
            let _ = std::fs::remove_file(&res_file);
            match serde_json::from_str::<OpResult>(&txt) {
                Ok(OpResult { ok: true, .. }) => Ok(()),
                Ok(OpResult { error, .. }) => Err(error.unwrap_or_else(|| "Falha na operação de administrador.".into())),
                Err(_) => Err("A operação de administrador não retornou resultado.".into()),
            }
        }
    }
}

/// Ponto de entrada do processo elevado: `--admin-op <arquivo>`.
pub fn admin_main(file: &str) -> i32 {
    let path = std::path::PathBuf::from(file);
    let res_file = path.with_extension("result.json");
    let ops: Vec<AdminOp> = match std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()) {
        Some(o) => o,
        None => {
            let _ = std::fs::write(&res_file, r#"{"ok":false,"error":"Arquivo de operação inválido."}"#);
            return 1;
        }
    };
    let _ = std::fs::remove_file(&path);
    let result = execute_all(&ops);
    let out = OpResult { ok: result.is_ok(), error: result.err() };
    let _ = std::fs::write(&res_file, serde_json::to_vec(&out).unwrap_or_default());
    if out.ok {
        0
    } else {
        1
    }
}
