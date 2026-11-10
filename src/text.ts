import type { Mode, Routine, Run, Schedule } from "./api";

export const DAY_NAMES = ["Domingo", "Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado"];
export const DAY_SHORT = ["Dom", "Seg", "Ter", "Qua", "Qui", "Sex", "Sáb"];

export const MODE_INFO: Record<Mode, { title: string; short: string; desc: string }> = {
  copy: {
    title: "Cópia simples",
    short: "Cópia simples",
    desc: "Copia os arquivos novos e os que mudaram. Nunca apaga nada do backup, mesmo que você apague no computador. É a opção mais segura.",
  },
  mirror: {
    title: "Espelho (cópia idêntica)",
    short: "Espelho",
    desc: "O backup fica exatamente igual ao computador. ATENÇÃO: se um arquivo for apagado no computador, ele também será apagado do backup.",
  },
  versions: {
    title: "Versões com data",
    short: "Versões",
    desc: "Cada backup é guardado numa pasta separada com a data. Dá para voltar a como os arquivos estavam em dias anteriores. Usa mais espaço.",
  },
};

export function fmtBytes(b: number | null | undefined): string {
  if (b == null || isNaN(b)) return "-";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let v = b;
  let i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toLocaleString("pt-BR", { maximumFractionDigits: i === 0 ? 0 : 1 })} ${u[i]}`;
}

export function fmtNum(n: number): string {
  return (n ?? 0).toLocaleString("pt-BR");
}

export function parseDate(s?: string | null): Date | null {
  if (!s) return null;
  const d = new Date(s);
  return isNaN(d.getTime()) ? null : d;
}

export function fmtDateTime(s?: string | null): string {
  const d = parseDate(s);
  if (!d) return "-";
  return d.toLocaleString("pt-BR", { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit" });
}

export function fmtRelative(s?: string | null): string {
  const d = parseDate(s);
  if (!d) return "nunca";
  const diff = (Date.now() - d.getTime()) / 1000;
  if (diff < 60) return "agora há pouco";
  if (diff < 3600) return `há ${Math.floor(diff / 60)} min`;
  if (diff < 86400) return `há ${Math.floor(diff / 3600)} h`;
  const days = Math.floor(diff / 86400);
  if (days === 1) return "ontem";
  return `há ${days} dias`;
}

export function fmtDuration(secs: number): string {
  if (!isFinite(secs) || secs < 0) return "-";
  secs = Math.round(secs);
  if (secs < 60) return `${secs} s`;
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m} min ${secs % 60 ? (secs % 60) + " s" : ""}`.trim();
  const h = Math.floor(m / 60);
  return `${h} h ${m % 60} min`;
}

export function runDuration(r: Run): string {
  const a = parseDate(r.started_at);
  const b = parseDate(r.finished_at) ?? new Date();
  if (!a) return "-";
  return fmtDuration((b.getTime() - a.getTime()) / 1000);
}

export function describeSchedule(s: Schedule, enabled = true): string {
  if (!enabled) return "Pausado (não roda automaticamente)";
  const startup = s.also_on_startup ? " e ao ligar o computador" : "";
  switch (s.kind) {
    case "manual":
      return "Somente quando você mandar";
    case "daily":
      return `Todos os dias às ${s.time}${startup}`;
    case "weekly": {
      const days = [...s.days].sort().map((d) => DAY_SHORT[d]);
      return `${days.join(", ") || "Seg"} às ${s.time}${startup}`;
    }
    case "interval":
      return `A cada ${s.every_hours} hora${s.every_hours > 1 ? "s" : ""}${startup}`;
    case "startup":
      return "Sempre que o computador for ligado";
  }
}

/** Próxima execução prevista (aproximada, calculada localmente). */
export function nextRun(s: Schedule, enabled: boolean): Date | null {
  if (!enabled) return null;
  const now = new Date();
  const [hh, mm] = (s.time || "22:00").split(":").map((x) => parseInt(x, 10) || 0);
  if (s.kind === "daily" || s.kind === "weekly") {
    for (let i = 0; i < 8; i++) {
      const d = new Date(now);
      d.setDate(now.getDate() + i);
      d.setHours(hh, mm, 0, 0);
      if (d <= now) continue;
      if (s.kind === "weekly" && !(s.days.length ? s.days : [1]).includes(d.getDay())) continue;
      return d;
    }
  }
  if (s.kind === "interval") {
    const start = new Date(now);
    start.setHours(hh, mm, 0, 0);
    const step = Math.max(1, s.every_hours) * 3600 * 1000;
    let t = start.getTime();
    if (t > now.getTime()) {
      while (t - step > now.getTime()) t -= step;
    } else {
      while (t <= now.getTime()) t += step;
    }
    return new Date(t);
  }
  return null;
}

export function fmtNext(d: Date | null): string {
  if (!d) return "-";
  const today = new Date();
  const tomorrow = new Date();
  tomorrow.setDate(today.getDate() + 1);
  const time = d.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === today.toDateString()) return `Hoje às ${time}`;
  if (d.toDateString() === tomorrow.toDateString()) return `Amanhã às ${time}`;
  return `${DAY_NAMES[d.getDay()]}, ${d.toLocaleDateString("pt-BR")} às ${time}`;
}

export const STATUS_INFO: Record<Run["status"], { label: string; cls: string }> = {
  running: { label: "Em andamento", cls: "blue" },
  success: { label: "Concluído", cls: "green" },
  warning: { label: "Concluído com avisos", cls: "yellow" },
  failed: { label: "Falhou", cls: "red" },
  canceled: { label: "Interrompido", cls: "gray" },
};

export const TRIGGER_LABEL: Record<string, string> = {
  manual: "Iniciado manualmente",
  schedule: "Agendado",
  restore: "Restauração",
};

export const PHASE_LABEL: Record<string, string> = {
  preparing: "Preparando…",
  scanning: "Calculando o que precisa ser copiado…",
  copying: "Copiando arquivos…",
  cleaning: "Apagando versões antigas…",
  done: "Finalizando…",
};

export function destLabel(r: Routine): string {
  if (r.dest_kind === "network") return `Rede: ${r.dest_path}`;
  if (r.dest_kind === "usb") return `HD externo${r.dest_volume_label ? ` "${r.dest_volume_label}"` : ""}: ${r.dest_path}`;
  return r.dest_path;
}

/** Saúde da rotina: ok | late | failed | never | paused */
export function routineHealth(
  r: Routine,
  last?: Run | null,
  lastOk?: Run | null,
  overdueDays = 2
): "ok" | "late" | "failed" | "never" | "paused" {
  if (last && last.status === "failed") return "failed";
  if (!lastOk) return r.schedule.kind === "manual" || !r.enabled ? (r.enabled ? "never" : "paused") : "never";
  if (!r.enabled) return "paused";
  if (r.schedule.kind === "manual") return "ok";
  const d = parseDate(lastOk.finished_at ?? lastOk.started_at);
  if (!d) return "never";
  let limitH = overdueDays * 24;
  if (r.schedule.kind === "weekly") limitH = Math.max(limitH, 8 * 24);
  if (r.schedule.kind === "interval") limitH = Math.max(r.schedule.every_hours * 3, overdueDays * 24);
  if (r.schedule.kind === "startup") limitH = Math.max(limitH, 7 * 24);
  return (Date.now() - d.getTime()) / 3600000 > limitH ? "late" : "ok";
}
