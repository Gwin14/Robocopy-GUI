//! Chamadas da API do Windows: admin, volumes, rede, DPAPI e elevação (UAC).

use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::process::Command;

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Comando sem abrir janela de console.
pub fn hidden_command(program: &str) -> Command {
    let mut c = Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

pub fn is_elevated() -> bool {
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

// ---------------- Volumes / discos ----------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct VolumeInfo {
    pub root: String,
    pub label: String,
    pub serial: u32,
    pub drive_type: u32,
}

/// "E:\pasta" -> "E:\"
pub fn drive_root(path: &str) -> Option<String> {
    let b = path.as_bytes();
    if b.len() >= 2 && b[1] == b':' && (b[0] as char).is_ascii_alphabetic() {
        Some(format!("{}:\\", (b[0] as char).to_ascii_uppercase()))
    } else {
        None
    }
}

pub fn volume_info(root: &str) -> Option<VolumeInfo> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumeInformationW};
    let w = wide(root);
    let mut name = [0u16; 261];
    let mut fs = [0u16; 261];
    let mut serial: u32 = 0;
    let mut maxlen: u32 = 0;
    let mut flags: u32 = 0;
    let ok = unsafe {
        GetVolumeInformationW(
            w.as_ptr(),
            name.as_mut_ptr(),
            name.len() as u32,
            &mut serial,
            &mut maxlen,
            &mut flags,
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };
    if ok == 0 {
        return None;
    }
    let drive_type = unsafe { GetDriveTypeW(w.as_ptr()) };
    Some(VolumeInfo { root: root.to_string(), label: from_wide(&name), serial, drive_type })
}

/// Lista discos disponíveis (letras).
pub fn list_volumes() -> Vec<VolumeInfo> {
    let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
    let mut out = vec![];
    for i in 0..26u32 {
        if mask & (1 << i) != 0 {
            let root = format!("{}:\\", (b'A' + i as u8) as char);
            if let Some(v) = volume_info(&root) {
                out.push(v);
            }
        }
    }
    out
}

/// Procura o disco pelo número de série (caso a letra tenha mudado).
pub fn find_volume_by_serial(serial: u32) -> Option<VolumeInfo> {
    list_volumes().into_iter().find(|v| v.serial == serial)
}

/// (livre, total) em bytes. Funciona com caminho local ou UNC.
pub fn disk_space(path: &str) -> Option<(u64, u64)> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let mut p = path.to_string();
    if !p.ends_with('\\') {
        p.push('\\');
    }
    let w = wide(&p);
    let mut free: u64 = 0;
    let mut total: u64 = 0;
    let mut tfree: u64 = 0;
    let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut free, &mut total, &mut tfree) };
    if ok == 0 {
        None
    } else {
        Some((free, total))
    }
}

// ---------------- Rede ----------------

/// "\\servidor\compart\pasta" -> "\\servidor\compart"
pub fn unc_share_root(path: &str) -> Option<String> {
    if !path.starts_with("\\\\") {
        return None;
    }
    let parts: Vec<&str> = path[2..].split('\\').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    Some(format!("\\\\{}\\{}", parts[0], parts[1]))
}

pub fn net_error_text(code: u32) -> String {
    match code {
        5 => "Acesso negado: este usuário não tem permissão nessa pasta de rede.".into(),
        53 | 67 | 1222 => "Não encontrei o computador ou a pasta de rede. Verifique se o endereço está certo e se o aparelho (NAS/servidor) está ligado.".into(),
        86 | 1326 => "Usuário ou senha da rede incorretos.".into(),
        1219 => "Já existe uma conexão com esse servidor usando outro usuário. Reinicie o computador ou use o mesmo usuário.".into(),
        1311 | 1355 => "O servidor de rede não está disponível no momento.".into(),
        1909 => "A conta de usuário da rede está bloqueada.".into(),
        1330 => "A senha da conta de rede expirou.".into(),
        _ => format!("Falha ao conectar na rede (código {code})."),
    }
}

/// Conecta (sem letra de unidade) usando usuário/senha. Retorna Ok(true) se criou a conexão.
pub fn net_connect(unc: &str, user: Option<&str>, password: Option<&str>) -> Result<bool, String> {
    use windows_sys::Win32::NetworkManagement::WNet::{
        WNetAddConnection2W, WNetCancelConnection2W, CONNECT_TEMPORARY, NETRESOURCEW, RESOURCETYPE_DISK,
    };
    let root = unc_share_root(unc).ok_or_else(|| "Endereço de rede inválido. Use o formato \\\\servidor\\pasta".to_string())?;
    let mut remote = wide(&root);
    let user_w = user.filter(|u| !u.is_empty()).map(wide);
    let pass_w = password.map(wide);
    let attempt = |remote: &mut Vec<u16>| -> u32 {
        let mut nr: NETRESOURCEW = unsafe { std::mem::zeroed() };
        nr.dwType = RESOURCETYPE_DISK;
        nr.lpRemoteName = remote.as_mut_ptr();
        unsafe {
            WNetAddConnection2W(
                &nr,
                pass_w.as_ref().map(|p| p.as_ptr()).unwrap_or(std::ptr::null()),
                user_w.as_ref().map(|u| u.as_ptr()).unwrap_or(std::ptr::null()),
                CONNECT_TEMPORARY,
            )
        }
    };
    let mut rc = attempt(&mut remote);
    if rc == 1219 {
        // Conexão antiga com outras credenciais: derruba e tenta de novo.
        let w = wide(&root);
        unsafe { WNetCancelConnection2W(w.as_ptr(), 0, 1) };
        rc = attempt(&mut remote);
    }
    match rc {
        0 => Ok(true),
        _ => Err(net_error_text(rc)),
    }
}

pub fn net_disconnect(unc: &str) {
    use windows_sys::Win32::NetworkManagement::WNet::WNetCancelConnection2W;
    if let Some(root) = unc_share_root(unc) {
        let w = wide(&root);
        unsafe { WNetCancelConnection2W(w.as_ptr(), 0, 1) };
    }
}

// ---------------- DPAPI (escopo da máquina) ----------------

use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_LOCAL_MACHINE, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

unsafe fn free_blob(b: &CRYPT_INTEGER_BLOB) {
    windows_sys::Win32::Foundation::LocalFree(b.pbData as *mut c_void);
}

pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut input = data.to_vec();
    let inb = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_mut_ptr() };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        CryptProtectData(
            &inb,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_LOCAL_MACHINE | CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    if ok == 0 {
        return Err("Não foi possível proteger a senha.".into());
    }
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe { free_blob(&out) };
    Ok(v)
}

pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut input = data.to_vec();
    let inb = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_mut_ptr() };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        CryptUnprotectData(
            &inb,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    if ok == 0 {
        return Err("Não foi possível ler a senha salva.".into());
    }
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe { free_blob(&out) };
    Ok(v)
}

// ---------------- Elevação (UAC) ----------------

/// Inicia o próprio programa como administrador. Retorna o código de saída se `wait`.
/// Err("CANCELADO") quando o usuário recusa o UAC.
pub fn run_elevated(args: &str, wait: bool) -> Result<u32, String> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let exe = wide(&crate::paths::current_exe());
    let verb = wide("runas");
    let params = wide(args);
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = exe.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE as i32;
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(1223) {
            return Err("CANCELADO".into());
        }
        return Err(format!("Não foi possível obter permissão de administrador: {err}"));
    }
    let h = info.hProcess;
    if h.is_null() {
        return Ok(0);
    }
    let mut code: u32 = 0;
    if wait {
        unsafe {
            if WaitForSingleObject(h, INFINITE) == WAIT_OBJECT_0 {
                GetExitCodeProcess(h, &mut code);
            }
        }
    }
    unsafe { CloseHandle(h) };
    Ok(code)
}

/// Abre uma pasta no Explorer.
pub fn open_in_explorer(path: &str) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Não foi possível abrir o Explorer: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unc_root() {
        assert_eq!(unc_share_root(r"\\nas\backup\pousada").as_deref(), Some(r"\\nas\backup"));
        assert_eq!(unc_share_root(r"\\nas"), None);
        assert_eq!(drive_root(r"e:\x").as_deref(), Some(r"E:\"));
    }
    #[test]
    fn dpapi_roundtrip() {
        let enc = protect(b"segredo").unwrap();
        assert_eq!(unprotect(&enc).unwrap(), b"segredo");
    }
}
