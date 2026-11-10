//! Montagem dos argumentos, leitura da saída e tradução dos códigos do Robocopy.

use crate::models::{Mode, Options};

pub const DEFAULT_EXCLUDE_FILES: &[&str] = &["Thumbs.db", "desktop.ini", "~$*", "*.tmp"];
pub const DEFAULT_EXCLUDE_DIRS: &[&str] = &["$RECYCLE.BIN", "System Volume Information"];

pub struct ArgSpec<'a> {
    pub src: &'a str,
    pub dst: &'a str,
    pub mode: &'a Mode,
    pub opts: &'a Options,
    pub elevated: bool,
    pub list_only: bool,
    pub log_path: Option<&'a str>,
    /// Na restauração: não substituir arquivos mais novos no destino.
    pub exclude_older: bool,
    /// Copiar apenas um arquivo específico da pasta `src`.
    pub single_file: Option<&'a str>,
}

pub fn build_args(s: &ArgSpec) -> Vec<String> {
    let mut a: Vec<String> = vec![s.src.to_string(), s.dst.to_string()];
    if let Some(f) = s.single_file {
        a.push(f.to_string());
    } else {
        match s.mode {
            Mode::Mirror => a.push("/MIR".into()),
            _ => a.push("/E".into()),
        }
    }
    a.extend(
        ["/COPY:DAT", "/DCOPY:DAT", "/XJ", "/FFT", "/BYTES", "/NP", "/NDL", "/FP", "/NJH", "/UNICODE"]
            .iter()
            .map(|x| x.to_string()),
    );
    a.push(format!("/R:{}", s.opts.retries.min(100)));
    a.push(format!("/W:{}", s.opts.wait_secs.min(600)));
    let threads = s.opts.threads.clamp(1, 128);
    if s.opts.ipg_ms > 0 {
        // /IPG não combina com /MT
        a.push(format!("/IPG:{}", s.opts.ipg_ms));
    } else if threads > 1 {
        a.push(format!("/MT:{threads}"));
    }
    if s.elevated && s.opts.backup_mode && !s.list_only {
        a.push("/B".into());
    }
    if s.exclude_older {
        a.push("/XO".into());
    }
    if s.list_only {
        a.push("/L".into());
        a.push("/NFL".into());
    } else if let Some(log) = s.log_path {
        a.push(format!("/UNILOG+:{log}"));
        a.push("/TEE".into());
    }
    let mut xf: Vec<String> = DEFAULT_EXCLUDE_FILES.iter().map(|x| x.to_string()).collect();
    xf.extend(s.opts.exclude_files.iter().filter(|x| !x.trim().is_empty()).map(|x| x.trim().to_string()));
    a.push("/XF".into());
    a.extend(xf);
    let mut xd: Vec<String> = DEFAULT_EXCLUDE_DIRS.iter().map(|x| x.to_string()).collect();
    xd.extend(s.opts.exclude_dirs.iter().filter(|x| !x.trim().is_empty()).map(|x| x.trim().to_string()));
    a.push("/XD".into());
    a.extend(xd);
    a
}

// ---------------- Leitura da saída ----------------

#[derive(Debug, PartialEq)]
pub enum Line {
    File { size: u64, path: String, extra: bool },
    Error { code: u32, text: String },
    Summary([u64; 6]),
    Other(String),
}

fn looks_like_path(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with("\\\\") || (b.len() >= 3 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
}

pub fn parse_line(raw: &str) -> Line {
    let line = raw.trim_end_matches(['\r', '\n']);
    let fields: Vec<&str> = line.split('\t').map(|f| f.trim()).filter(|f| !f.is_empty()).collect();
    if fields.len() >= 2 {
        let last = fields[fields.len() - 1];
        if looks_like_path(last) {
            if let Ok(size) = fields[fields.len() - 2].parse::<u64>() {
                let extra = fields.len() >= 3 && fields[0].starts_with('*');
                return Line::File { size, path: last.to_string(), extra };
            }
        }
    }
    // Erro: "2026/11/10 10:00:00 ERRO 5 (0x00000005) Copiando arquivo C:\..."
    let toks: Vec<&str> = line.split_whitespace().collect();
    if toks.len() >= 5
        && toks[0].matches('/').count() == 2
        && toks[1].matches(':').count() == 2
        && toks[3].chars().all(|c| c.is_ascii_digit())
        && toks[4].starts_with("(0x")
    {
        let code = toks[3].parse().unwrap_or(0);
        let text = toks[5..].join(" ");
        return Line::Error { code, text };
    }
    // Linha do resumo: "   Arquivos :   10   5   5   0   0   0"
    if let Some(idx) = line.find(':') {
        let rest = &line[idx + 1..];
        let nums: Vec<&str> = rest.split_whitespace().collect();
        if nums.len() == 6 {
            let parsed: Vec<u64> = nums.iter().filter_map(|n| n.parse().ok()).collect();
            if parsed.len() == 6 {
                let mut arr = [0u64; 6];
                arr.copy_from_slice(&parsed);
                return Line::Summary(arr);
            }
        }
    }
    Line::Other(line.to_string())
}

/// Resumo final: linhas na ordem Diretórios, Arquivos, Bytes.
/// Colunas: Total, Copiados, Ignorados, Incompatíveis, Falhas, Extras.
#[derive(Debug, Default, Clone)]
pub struct Summary {
    pub dirs: [u64; 6],
    pub files: [u64; 6],
    pub bytes: [u64; 6],
    pub rows: usize,
}

impl Summary {
    pub fn push(&mut self, row: [u64; 6]) {
        match self.rows {
            0 => self.dirs = row,
            1 => self.files = row,
            2 => self.bytes = row,
            _ => {}
        }
        self.rows += 1;
    }
    pub fn complete(&self) -> bool {
        self.rows >= 3
    }
}

// ---------------- Decodificação (UTF-16 com /UNICODE, ou OEM) ----------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Enc {
    Utf16,
    Oem,
}

pub struct LineDecoder {
    enc: Option<Enc>,
    buf: Vec<u8>,
}

impl LineDecoder {
    pub fn new() -> Self {
        LineDecoder { enc: None, buf: vec![] }
    }

    pub fn push(&mut self, data: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(data);
        if self.enc.is_none() {
            if self.buf.len() < 64 && !data.is_empty() {
                return vec![];
            }
            let sample = &self.buf[..self.buf.len().min(512)];
            let zeros_odd = sample.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
            let half = (sample.len() / 2).max(1);
            self.enc = Some(if zeros_odd * 10 >= half * 3 { Enc::Utf16 } else { Enc::Oem });
            // Remove BOM. Obs.: com /UNICODE o robocopy escreve BOM UTF-16 seguido de texto UTF-8.
            if self.buf.starts_with(&[0xFF, 0xFE]) {
                self.buf.drain(..2);
            }
        }
        self.extract(false)
    }

    pub fn finish(&mut self) -> Vec<String> {
        if self.enc.is_none() {
            let r = self.push(&[]);
            if !r.is_empty() {
                return r;
            }
            if self.enc.is_none() {
                self.enc = Some(Enc::Oem);
            }
        }
        self.extract(true)
    }

    fn extract(&mut self, flush: bool) -> Vec<String> {
        let mut out = vec![];
        match self.enc {
            Some(Enc::Utf16) => loop {
                let mut found = None;
                let mut i = 0;
                while i + 1 < self.buf.len() {
                    if self.buf[i] == b'\n' && self.buf[i + 1] == 0 {
                        found = Some(i);
                        break;
                    }
                    i += 2;
                }
                match found {
                    Some(pos) => {
                        let chunk: Vec<u8> = self.buf.drain(..pos + 2).collect();
                        out.push(decode_utf16(&chunk[..pos]));
                    }
                    None => {
                        if flush && !self.buf.is_empty() {
                            let chunk: Vec<u8> = std::mem::take(&mut self.buf);
                            out.push(decode_utf16(&chunk));
                        }
                        break;
                    }
                }
            },
            Some(Enc::Oem) => loop {
                match self.buf.iter().position(|b| *b == b'\n') {
                    Some(pos) => {
                        let chunk: Vec<u8> = self.buf.drain(..pos + 1).collect();
                        out.push(decode_bytes(&chunk[..pos]));
                    }
                    None => {
                        if flush && !self.buf.is_empty() {
                            let chunk = std::mem::take(&mut self.buf);
                            out.push(decode_bytes(&chunk));
                        }
                        break;
                    }
                }
            },
            None => {}
        }
        out.into_iter().map(|s| s.trim_end_matches('\r').to_string()).collect()
    }
}

/// UTF-8 quando válido; senão página de código OEM.
pub fn decode_bytes(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => decode_oem(b),
    }
}

fn decode_utf16(b: &[u8]) -> String {
    let u: Vec<u16> = b.chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
    String::from_utf16_lossy(&u)
}

#[cfg(windows)]
pub fn decode_oem(b: &[u8]) -> String {
    use windows_sys::Win32::Globalization::{MultiByteToWideChar, CP_OEMCP};
    if b.is_empty() {
        return String::new();
    }
    unsafe {
        let n = MultiByteToWideChar(CP_OEMCP, 0, b.as_ptr(), b.len() as i32, std::ptr::null_mut(), 0);
        if n <= 0 {
            return String::from_utf8_lossy(b).to_string();
        }
        let mut w = vec![0u16; n as usize];
        MultiByteToWideChar(CP_OEMCP, 0, b.as_ptr(), b.len() as i32, w.as_mut_ptr(), n);
        String::from_utf16_lossy(&w)
    }
}

#[cfg(not(windows))]
pub fn decode_oem(b: &[u8]) -> String {
    String::from_utf8_lossy(b).to_string()
}

// ---------------- Códigos de saída ----------------

pub struct Outcome {
    pub status: &'static str,
    pub message: String,
}

/// Traduz o código de saída do Robocopy (máscara de bits) para algo compreensível.
pub fn interpret_exit(code: i32, copied: u64, failed: u64) -> Outcome {
    if code < 0 {
        return Outcome { status: "failed", message: "Não foi possível iniciar o Robocopy.".into() };
    }
    if code & 16 != 0 {
        return Outcome {
            status: "failed",
            message: "Erro grave: o Robocopy não conseguiu acessar a origem ou o destino. Verifique se as pastas existem e se há permissão.".into(),
        };
    }
    if code & 8 != 0 {
        if copied == 0 && failed > 0 {
            return Outcome {
                status: "failed",
                message: format!("Nenhum arquivo pôde ser copiado ({failed} com erro). Verifique o acesso ao destino."),
            };
        }
        return Outcome {
            status: "warning",
            message: format!(
                "Backup concluído, mas {failed} arquivo(s) não puderam ser copiados. Geralmente são arquivos abertos em uso por outro programa."
            ),
        };
    }
    let mut msg = if code & 1 != 0 {
        format!("Backup concluído com sucesso. {copied} arquivo(s) copiado(s).")
    } else {
        "Backup concluído. Nada mudou desde a última vez, tudo já estava em dia.".to_string()
    };
    if code & 4 != 0 {
        msg += " Alguns itens existem com tipos diferentes na origem e no destino.";
    }
    Outcome { status: "success", message: msg }
}

/// Mensagem amigável para códigos de erro do Windows que o Robocopy mostra.
pub fn win_error_text(code: u32) -> &'static str {
    match code {
        2 | 3 => "Arquivo ou pasta não encontrado",
        5 => "Acesso negado (sem permissão)",
        32 | 33 => "Arquivo em uso por outro programa",
        39 | 112 => "Disco de destino cheio",
        53 | 67 => "Computador ou pasta de rede não encontrado",
        64 | 59 => "A conexão de rede caiu durante a cópia",
        86 | 1326 => "Usuário ou senha da rede incorretos",
        121 => "A rede demorou demais para responder",
        1392 => "Arquivo corrompido ou ilegível",
        _ => "Erro ao copiar",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_copy_and_mirror() {
        let o = Options::default();
        let a = build_args(&ArgSpec {
            src: r"C:\Dados",
            dst: r"E:\Backup\Dados",
            mode: &Mode::Copy,
            opts: &o,
            elevated: true,
            list_only: false,
            log_path: Some(r"C:\log.txt"),
            exclude_older: false,
            single_file: None,
        });
        assert_eq!(a[0], r"C:\Dados");
        assert!(a.contains(&"/E".to_string()));
        assert!(!a.contains(&"/MIR".to_string()));
        assert!(a.contains(&"/B".to_string()));
        assert!(a.contains(&"/MT:8".to_string()));
        assert!(a.contains(&r"/UNILOG+:C:\log.txt".to_string()));
        let m = build_args(&ArgSpec {
            src: "a",
            dst: "b",
            mode: &Mode::Mirror,
            opts: &o,
            elevated: false,
            list_only: true,
            log_path: None,
            exclude_older: false,
            single_file: None,
        });
        assert!(m.contains(&"/MIR".to_string()));
        assert!(m.contains(&"/L".to_string()));
        assert!(!m.contains(&"/B".to_string()));
        assert!(!m.iter().any(|x| x.starts_with("/UNILOG")));
    }

    #[test]
    fn ipg_disables_mt() {
        let mut o = Options::default();
        o.ipg_ms = 50;
        let a = build_args(&ArgSpec {
            src: "a",
            dst: "b",
            mode: &Mode::Copy,
            opts: &o,
            elevated: false,
            list_only: false,
            log_path: None,
            exclude_older: true,
            single_file: None,
        });
        assert!(a.contains(&"/IPG:50".to_string()));
        assert!(!a.iter().any(|x| x.starts_with("/MT")));
        assert!(a.contains(&"/XO".to_string()));
    }

    #[test]
    fn parse_lines() {
        assert_eq!(
            parse_line("\t    Novo arquivo  \t\t    1234\tC:\\src\\a b.txt"),
            Line::File { size: 1234, path: "C:\\src\\a b.txt".into(), extra: false }
        );
        assert_eq!(
            parse_line("\t   *EXTRA Arquivo \t\t     10\tE:\\dst\\x.txt"),
            Line::File { size: 10, path: "E:\\dst\\x.txt".into(), extra: true }
        );
        assert_eq!(
            parse_line("2026/11/10 10:00:00 ERRO 5 (0x00000005) Copiando arquivo C:\\a.txt"),
            Line::Error { code: 5, text: "Copiando arquivo C:\\a.txt".into() }
        );
        assert_eq!(
            parse_line("   Arquivos :        10         5         5         0         0         0"),
            Line::Summary([10, 5, 5, 0, 0, 0])
        );
        assert!(matches!(parse_line("   Tempos :   0:00:01   0:00:01   0:00:00"), Line::Other(_)));
    }

    #[test]
    fn decoder_utf16() {
        let text = "linha um\r\nação 2\r\n";
        let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let mut d = LineDecoder::new();
        let mut lines = d.push(&bytes);
        lines.extend(d.finish());
        assert_eq!(lines, vec!["linha um", "ação 2"]);
    }

    #[test]
    fn decoder_bom_then_utf8() {
        // Formato real do robocopy /UNICODE redirecionado: BOM UTF-16 + texto UTF-8
        let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
        bytes.extend_from_slice("\n\t  Novo Arquivo\t\t       5\tC:\\o\\relatório.txt\r\n".as_bytes());
        bytes.extend_from_slice("Diretórios:         2         2         0         0         0         0\r\n".as_bytes());
        bytes.extend(std::iter::repeat(b' ').take(80));
        let mut d = LineDecoder::new();
        let mut lines = d.push(&bytes);
        lines.extend(d.finish());
        assert_eq!(
            parse_line(&lines[1]),
            Line::File { size: 5, path: "C:\\o\\relatório.txt".into(), extra: false }
        );
        assert_eq!(parse_line(&lines[2]), Line::Summary([2, 2, 0, 0, 0, 0]));
    }

    #[test]
    fn decoder_ascii() {
        let mut d = LineDecoder::new();
        let mut lines = d.push(b"abc\r\ndef\r\n");
        lines.extend(d.finish());
        assert_eq!(lines, vec!["abc", "def"]);
    }

    #[test]
    fn exit_codes() {
        assert_eq!(interpret_exit(0, 0, 0).status, "success");
        assert_eq!(interpret_exit(1, 5, 0).status, "success");
        assert_eq!(interpret_exit(3, 5, 0).status, "success");
        assert_eq!(interpret_exit(9, 5, 2).status, "warning");
        assert_eq!(interpret_exit(8, 0, 2).status, "failed");
        assert_eq!(interpret_exit(16, 0, 0).status, "failed");
    }
}
