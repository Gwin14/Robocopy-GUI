use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
    PathBuf::from(base).join("RoboBackup")
}

pub fn db_path() -> PathBuf {
    data_dir().join("robobackup.db")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn secrets_dir() -> PathBuf {
    data_dir().join("secrets")
}

pub fn tasks_tmp_dir() -> PathBuf {
    data_dir().join("tmp")
}

pub fn current_exe() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Remove caracteres inválidos para nome de pasta do Windows.
pub fn sanitize_name(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 32 => '_',
            c => c,
        })
        .collect();
    out = out.trim().trim_end_matches('.').trim().to_string();
    if out.is_empty() {
        "Backup".into()
    } else {
        out
    }
}

/// Nome da pasta de destino para uma origem (ex.: "C:\Users\Ana\Documents" -> "Documents", "D:\" -> "Disco D").
pub fn source_leaf(src: &str) -> String {
    let t = src.trim_end_matches(['\\', '/']);
    if t.len() == 2 && t.ends_with(':') {
        return format!("Disco {}", &t[..1].to_uppercase());
    }
    let leaf = t.rsplit(['\\', '/']).next().unwrap_or(t);
    sanitize_name(leaf)
}

/// Gera nomes únicos de subpastas para cada origem.
pub fn source_leaves(sources: &[String]) -> Vec<String> {
    let mut used: Vec<String> = vec![];
    let mut out = vec![];
    for s in sources {
        let base = source_leaf(s);
        let mut name = base.clone();
        let mut i = 2;
        while used.iter().any(|u| u.eq_ignore_ascii_case(&name)) {
            name = format!("{} ({})", base, i);
            i += 1;
        }
        used.push(name.clone());
        out.push(name);
    }
    out
}

pub fn join_win(a: &str, b: &str) -> String {
    let a = a.trim_end_matches(['\\', '/']);
    format!("{}\\{}", a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves() {
        assert_eq!(source_leaf(r"C:\Users\Ana\Documents\"), "Documents");
        assert_eq!(source_leaf(r"D:\"), "Disco D");
        assert_eq!(source_leaf(r"\\srv\share\Fotos"), "Fotos");
        let v = source_leaves(&[r"C:\a\Fotos".into(), r"D:\Fotos".into(), r"E:\fotos".into()]);
        assert_eq!(v, vec!["Fotos", "Fotos (2)", "fotos (3)"]);
    }

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_name("Backup: Recepção?"), "Backup_ Recepção_");
        assert_eq!(sanitize_name("  ..  "), "Backup");
    }
}
