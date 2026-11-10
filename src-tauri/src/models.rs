use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Copia novos e alterados; nunca apaga no destino.
    Copy,
    /// Destino fica idêntico à origem (/MIR).
    Mirror,
    /// Cada execução numa pasta com data; mantém as últimas N.
    Versions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DestKind {
    Local,
    Usb,
    Network,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleKind {
    Manual,
    Daily,
    Weekly,
    Interval,
    Startup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub kind: ScheduleKind,
    /// "HH:MM"
    #[serde(default = "default_time")]
    pub time: String,
    /// 0 = domingo ... 6 = sábado
    #[serde(default)]
    pub days: Vec<u8>,
    #[serde(default = "default_hours")]
    pub every_hours: u32,
    #[serde(default)]
    pub also_on_startup: bool,
    #[serde(default)]
    pub wake: bool,
}

fn default_time() -> String {
    "22:00".into()
}
fn default_hours() -> u32 {
    4
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            kind: ScheduleKind::Manual,
            time: default_time(),
            days: vec![],
            every_hours: default_hours(),
            also_on_startup: false,
            wake: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    #[serde(default = "d_threads")]
    pub threads: u32,
    #[serde(default = "d_retries")]
    pub retries: u32,
    #[serde(default = "d_wait")]
    pub wait_secs: u32,
    #[serde(default)]
    pub exclude_files: Vec<String>,
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    /// Pausa entre pacotes (ms) para não travar a rede. 0 = sem limite.
    #[serde(default)]
    pub ipg_ms: u32,
    /// Usa /B (modo backup) quando estiver rodando como administrador.
    #[serde(default = "d_true")]
    pub backup_mode: bool,
    /// Calcula o total antes de copiar (permite barra de progresso).
    #[serde(default = "d_true")]
    pub prescan: bool,
}

fn d_threads() -> u32 {
    8
}
fn d_retries() -> u32 {
    2
}
fn d_wait() -> u32 {
    5
}
fn d_true() -> bool {
    true
}

impl Default for Options {
    fn default() -> Self {
        Options {
            threads: d_threads(),
            retries: d_retries(),
            wait_secs: d_wait(),
            exclude_files: vec![],
            exclude_dirs: vec![],
            ipg_ms: 0,
            backup_mode: true,
            prescan: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub name: String,
    #[serde(default = "d_true")]
    pub enabled: bool,
    pub sources: Vec<String>,
    pub dest_kind: DestKind,
    pub dest_path: String,
    #[serde(default)]
    pub dest_volume_label: Option<String>,
    #[serde(default)]
    pub dest_volume_serial: Option<u32>,
    #[serde(default)]
    pub net_user: Option<String>,
    #[serde(default)]
    pub has_password: bool,
    pub mode: Mode,
    #[serde(default = "d_keep")]
    pub keep_versions: u32,
    #[serde(default)]
    pub schedule: Schedule,
    #[serde(default)]
    pub options: Options,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn d_keep() -> u32 {
    7
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Run {
    pub id: i64,
    pub routine_id: String,
    pub routine_name: String,
    pub trigger: String,
    /// running | success | warning | failed | canceled
    pub status: String,
    /// preparing | scanning | copying | cleaning | done
    pub phase: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub pid: Option<i64>,
    pub total_files: i64,
    pub total_bytes: i64,
    pub done_files: i64,
    pub done_bytes: i64,
    pub current_file: Option<String>,
    pub failed_files: i64,
    pub skipped_files: i64,
    pub extra_files: i64,
    pub speed_bps: f64,
    pub exit_code: Option<i64>,
    pub message: Option<String>,
    /// JSON: lista de textos (erros/avisos)
    pub details: Vec<String>,
    /// Pasta raiz gravada nesta execução (para restaurar).
    pub dest_folder: Option<String>,
    pub log_path: Option<String>,
    pub cancel_requested: bool,
    pub heartbeat: Option<String>,
    pub pruned: bool,
    pub mode: String,
}
