//! Integração com o Agendador de Tarefas do Windows (schtasks + XML).

use crate::models::{Routine, ScheduleKind};
use crate::{paths, robocopy, win};

pub fn task_name(routine_id: &str) -> String {
    format!("RoboBackup\\{routine_id}")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn valid_time(t: &str) -> String {
    let parts: Vec<u32> = t.split(':').filter_map(|p| p.trim().parse().ok()).collect();
    match parts.as_slice() {
        [h, m, ..] if *h < 24 && *m < 60 => format!("{h:02}:{m:02}"),
        _ => "22:00".into(),
    }
}

const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

pub fn triggers_xml(r: &Routine, today: &str) -> String {
    if !r.enabled {
        return String::new();
    }
    let s = &r.schedule;
    let start = format!("{today}T{}:00", valid_time(&s.time));
    let mut out = String::new();
    match s.kind {
        ScheduleKind::Manual => {}
        ScheduleKind::Daily => {
            out += &format!(
                "<CalendarTrigger><StartBoundary>{start}</StartBoundary><Enabled>true</Enabled>\
                 <ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay></CalendarTrigger>"
            );
        }
        ScheduleKind::Weekly => {
            let mut days: Vec<u8> = s.days.iter().copied().filter(|d| *d < 7).collect();
            days.sort();
            days.dedup();
            if days.is_empty() {
                days.push(1);
            }
            let d: String = days.iter().map(|d| format!("<{} />", DAYS[*d as usize])).collect();
            out += &format!(
                "<CalendarTrigger><StartBoundary>{start}</StartBoundary><Enabled>true</Enabled>\
                 <ScheduleByWeek><DaysOfWeek>{d}</DaysOfWeek><WeeksInterval>1</WeeksInterval></ScheduleByWeek></CalendarTrigger>"
            );
        }
        ScheduleKind::Interval => {
            let h = s.every_hours.clamp(1, 24 * 7);
            out += &format!(
                "<TimeTrigger><StartBoundary>{start}</StartBoundary><Enabled>true</Enabled>\
                 <Repetition><Interval>PT{h}H</Interval><StopAtDurationEnd>false</StopAtDurationEnd></Repetition></TimeTrigger>"
            );
        }
        ScheduleKind::Startup => {}
    }
    if s.kind == ScheduleKind::Startup || (s.also_on_startup && s.kind != ScheduleKind::Manual) {
        out += "<BootTrigger><Enabled>true</Enabled><Delay>PT5M</Delay></BootTrigger>";
    }
    out
}

pub fn task_xml(r: &Routine, exe: &str, today: &str) -> String {
    let triggers = triggers_xml(r, today);
    let triggers = if triggers.is_empty() { "<Triggers />".to_string() } else { format!("<Triggers>{triggers}</Triggers>") };
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Author>RoboBackup</Author>
    <Description>Backup automático: {name}</Description>
    <SecurityDescriptor>D:(A;;FA;;;BA)(A;;FA;;;SY)(A;;GRGX;;;AU)</SecurityDescriptor>
  </RegistrationInfo>
  {triggers}
  <Principals>
    <Principal id="Author">
      <UserId>S-1-5-18</UserId>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>{wake}</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>--run {id}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        name = esc(&r.name),
        wake = if r.schedule.wake { "true" } else { "false" },
        exe = esc(exe),
        id = esc(&r.id),
    )
}

fn run_schtasks(args: &[&str]) -> Result<String, String> {
    let out = win::hidden_command("schtasks.exe")
        .args(args)
        .output()
        .map_err(|e| format!("Não foi possível usar o Agendador de Tarefas: {e}"))?;
    let text = format!("{}{}", robocopy::decode_oem(&out.stdout), robocopy::decode_oem(&out.stderr));
    if out.status.success() {
        Ok(text)
    } else {
        Err(text.trim().to_string())
    }
}

/// Registra (ou atualiza) a tarefa. Precisa de administrador.
pub fn register(r: &Routine) -> Result<(), String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let xml = task_xml(r, &paths::current_exe(), &today);
    let dir = paths::tasks_tmp_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("{}.xml", r.id));
    let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    let f = file.to_string_lossy().to_string();
    let res = run_schtasks(&["/Create", "/TN", &task_name(&r.id), "/XML", &f, "/F"]);
    let _ = std::fs::remove_file(&file);
    res.map(|_| ()).map_err(|e| format!("Não foi possível agendar o backup: {e}"))
}

pub fn delete(routine_id: &str) -> Result<(), String> {
    if !exists(routine_id) {
        return Ok(());
    }
    run_schtasks(&["/Delete", "/TN", &task_name(routine_id), "/F"])
        .map(|_| ())
        .map_err(|e| format!("Não foi possível remover o agendamento: {e}"))
}

pub fn exists(routine_id: &str) -> bool {
    run_schtasks(&["/Query", "/TN", &task_name(routine_id)]).is_ok()
}

/// Pede ao Agendador para executar agora (roda como SYSTEM, sem UAC).
pub fn run_now(routine_id: &str) -> Result<(), String> {
    run_schtasks(&["/Run", "/TN", &task_name(routine_id)]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;

    fn routine(kind: ScheduleKind) -> Routine {
        Routine {
            id: "abc".into(),
            name: "Fotos & Docs".into(),
            enabled: true,
            sources: vec![],
            dest_kind: DestKind::Local,
            dest_path: "E:\\".into(),
            dest_volume_label: None,
            dest_volume_serial: None,
            net_user: None,
            has_password: false,
            mode: Mode::Copy,
            keep_versions: 7,
            schedule: Schedule { kind, time: "3:5".into(), days: vec![1, 5, 1], ..Default::default() },
            options: Options::default(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn xml_daily() {
        let x = task_xml(&routine(ScheduleKind::Daily), r"C:\App\robobackup.exe", "2026-11-10");
        assert!(x.contains("<StartBoundary>2026-11-10T03:05:00</StartBoundary>"));
        assert!(x.contains("<DaysInterval>1</DaysInterval>"));
        assert!(x.contains("Fotos &amp; Docs"));
        assert!(x.contains("<Arguments>--run abc</Arguments>"));
        assert!(x.contains("<UserId>S-1-5-18</UserId>"));
    }

    #[test]
    fn xml_weekly_and_boot() {
        let mut r = routine(ScheduleKind::Weekly);
        r.schedule.also_on_startup = true;
        let t = triggers_xml(&r, "2026-11-10");
        assert!(t.contains("<DaysOfWeek><Monday /><Friday /></DaysOfWeek>"));
        assert!(t.contains("<BootTrigger>"));
    }

    #[test]
    fn xml_manual_and_disabled() {
        assert_eq!(triggers_xml(&routine(ScheduleKind::Manual), "2026-11-10"), "");
        let mut r = routine(ScheduleKind::Daily);
        r.enabled = false;
        assert!(task_xml(&r, "x", "2026-11-10").contains("<Triggers />"));
    }

    #[test]
    fn xml_interval() {
        let mut r = routine(ScheduleKind::Interval);
        r.schedule.every_hours = 6;
        assert!(triggers_xml(&r, "2026-11-10").contains("<Interval>PT6H</Interval>"));
    }
}
