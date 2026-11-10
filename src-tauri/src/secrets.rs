//! Senhas de rede cifradas com DPAPI (máquina), numa pasta só de administradores.

use crate::{paths, win};

fn file_for(routine_id: &str) -> std::path::PathBuf {
    let safe: String = routine_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    paths::secrets_dir().join(format!("{safe}.bin"))
}

pub fn save(routine_id: &str, password: &str) -> Result<(), String> {
    std::fs::create_dir_all(paths::secrets_dir()).map_err(|e| format!("Erro ao criar pasta de senhas: {e}"))?;
    let enc = win::protect(password.as_bytes())?;
    std::fs::write(file_for(routine_id), enc).map_err(|e| format!("Erro ao salvar senha: {e}"))
}

pub fn load(routine_id: &str) -> Result<Option<String>, String> {
    let f = file_for(routine_id);
    if !f.exists() {
        return Ok(None);
    }
    let data = std::fs::read(&f).map_err(|_| {
        "Sem permissão para ler a senha salva. Use o botão 'Executar agora' ou execute como administrador.".to_string()
    })?;
    let dec = win::unprotect(&data)?;
    Ok(Some(String::from_utf8_lossy(&dec).to_string()))
}

pub fn delete(routine_id: &str) {
    let _ = std::fs::remove_file(file_for(routine_id));
}
