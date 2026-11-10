import { invoke } from "@tauri-apps/api/core";

export type Mode = "copy" | "mirror" | "versions";
export type DestKind = "local" | "usb" | "network";
export type ScheduleKind = "manual" | "daily" | "weekly" | "interval" | "startup";

export interface Schedule {
  kind: ScheduleKind;
  time: string;
  days: number[];
  every_hours: number;
  also_on_startup: boolean;
  wake: boolean;
}

export interface Options {
  threads: number;
  retries: number;
  wait_secs: number;
  exclude_files: string[];
  exclude_dirs: string[];
  ipg_ms: number;
  backup_mode: boolean;
  prescan: boolean;
}

export interface Routine {
  id: string;
  name: string;
  enabled: boolean;
  sources: string[];
  dest_kind: DestKind;
  dest_path: string;
  dest_volume_label?: string | null;
  dest_volume_serial?: number | null;
  net_user?: string | null;
  has_password: boolean;
  mode: Mode;
  keep_versions: number;
  schedule: Schedule;
  options: Options;
  created_at: string;
  updated_at: string;
}

export interface Run {
  id: number;
  routine_id: string;
  routine_name: string;
  trigger: string;
  status: "running" | "success" | "warning" | "failed" | "canceled";
  phase: string;
  started_at: string;
  finished_at?: string | null;
  total_files: number;
  total_bytes: number;
  done_files: number;
  done_bytes: number;
  current_file?: string | null;
  failed_files: number;
  skipped_files: number;
  extra_files: number;
  speed_bps: number;
  exit_code?: number | null;
  message?: string | null;
  details: string[];
  dest_folder?: string | null;
  log_path?: string | null;
  cancel_requested: boolean;
  mode: string;
}

export interface RoutineView {
  routine: Routine;
  last_run?: Run | null;
  last_success?: Run | null;
  running?: Run | null;
}

export interface Status {
  initialized: boolean;
  elevated: boolean;
  exe_mismatch: boolean;
  has_pin: boolean;
  unlocked: boolean;
  data_dir: string;
  computer: string;
}

export interface DestTest {
  ok: boolean;
  message: string;
  free?: number | null;
  total?: number | null;
  label?: string | null;
  removable: boolean;
}

export interface Drive {
  root: string;
  label: string;
  removable: boolean;
  free: number;
  total: number;
}

export interface KnownFolder {
  label: string;
  path: string;
}

export interface Settings {
  notifications: boolean;
  overdue_days: number;
}

export interface RestoreSpec {
  routine_id: string;
  run_id: number | null;
  inner_path: string | null;
  target: "original" | "folder";
  folder: string | null;
  keep_newer: boolean;
}

export const api = {
  status: () => invoke<Status>("get_status"),
  setup: () => invoke<void>("setup_computer"),
  repairTasks: () => invoke<void>("repair_tasks"),
  routines: () => invoke<RoutineView[]>("list_routines"),
  routine: (id: string) => invoke<Routine | null>("get_routine", { id }),
  checkTasks: () => invoke<Record<string, boolean>>("check_tasks"),
  saveRoutine: (routine: Routine, password: string | null, clearPassword: boolean) =>
    invoke<Routine>("save_routine", { routine, password, clearPassword }),
  setEnabled: (id: string, enabled: boolean) => invoke<void>("set_routine_enabled", { id, enabled }),
  deleteRoutine: (id: string, deleteHistory: boolean) => invoke<void>("delete_routine", { id, deleteHistory }),
  runNow: (id: string) => invoke<string>("run_now", { id }),
  cancel: (runId: number) => invoke<void>("cancel_run", { runId }),
  runs: (routineId?: string | null, status?: string | null, limit?: number) =>
    invoke<Run[]>("list_runs", { routineId: routineId ?? null, status: status ?? null, limit: limit ?? 300 }),
  activeRuns: () => invoke<Run[]>("active_runs"),
  run: (id: number) => invoke<Run | null>("get_run", { id }),
  log: (runId: number) => invoke<string>("read_log", { runId }),
  testDest: (kind: DestKind, path: string, user: string | null, password: string | null, routineId: string | null) =>
    invoke<DestTest>("test_destination", { kind, path, user, password, routineId }),
  drives: () => invoke<Drive[]>("list_drives"),
  knownFolders: () => invoke<KnownFolder[]>("known_folders"),
  versions: (routineId: string) => invoke<Run[]>("list_versions", { routineId }),
  backupLocation: (routineId: string, runId: number | null) =>
    invoke<string>("backup_location", { routineId, runId }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  startRestore: (spec: RestoreSpec) => invoke<void>("start_restore", { spec }),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (notifications: boolean, overdueDays: number) =>
    invoke<void>("save_settings", { notifications, overdueDays }),
  unlock: (pin: string) => invoke<boolean>("unlock", { pin }),
  lock: () => invoke<void>("lock"),
  setPin: (current: string, newPin: string) => invoke<void>("set_pin", { current, newPin }),
};

export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
