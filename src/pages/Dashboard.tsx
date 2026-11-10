import { useState } from "react";
import { AlertTriangle, CalendarClock, CheckCircle2, Loader2, Play, Plus, ShieldAlert, XCircle } from "lucide-react";
import { api, RoutineView, Run, Settings } from "../api";
import { RunCard, RunDetails } from "../runs";
import { StatusBadge, useApp, usePoll } from "../ui";
import { describeSchedule, fmtDateTime, fmtNext, fmtRelative, nextRun, routineHealth } from "../text";

export default function Dashboard({ go }: { go: (p: string, arg?: string) => void }) {
  const { guarded } = useApp();
  const [routines, reload] = usePoll<RoutineView[]>(api.routines, 2500);
  const [active] = usePoll<Run[]>(api.activeRuns, 1000);
  const [recent] = usePoll<Run[]>(() => api.runs(null, null, 8), 3000);
  const [settings] = usePoll<Settings>(api.settings, 30000);
  const [detail, setDetail] = useState<number | null>(null);

  if (!routines) return <div className="empty"><Loader2 className="spin" /></div>;

  const od = settings?.overdue_days ?? 2;
  const health = routines.map((v) => ({ v, h: routineHealth(v.routine, v.last_run, v.last_success, od) }));
  const failed = health.filter((x) => x.h === "failed");
  const late = health.filter((x) => x.h === "late" || x.h === "never");
  const running = active ?? [];

  let hero: { cls: string; icon: JSX.Element; title: string; text: string };
  if (routines.length === 0) {
    hero = { cls: "blue", icon: <ShieldAlert size={38} />, title: "Nenhum backup configurado ainda", text: "Crie a primeira rotina para proteger os arquivos deste computador." };
  } else if (failed.length) {
    hero = {
      cls: "red",
      icon: <XCircle size={40} />,
      title: failed.length === 1 ? `O último backup "${failed[0].v.routine.name}" falhou` : `${failed.length} backups falharam`,
      text: failed[0].v.last_run?.message ?? "Veja os detalhes no histórico.",
    };
  } else if (late.length) {
    const x = late[0];
    hero = {
      cls: "yellow",
      icon: <AlertTriangle size={38} />,
      title: x.h === "never" ? `"${x.v.routine.name}" ainda não foi executado` : `"${x.v.routine.name}" está atrasado`,
      text: x.h === "never" ? "Clique em \"Executar agora\" para fazer o primeiro backup." : `Último backup bem-sucedido ${fmtRelative(x.v.last_success?.finished_at)}. Verifique se o disco ou a rede estão conectados.`,
    };
  } else if (running.length) {
    hero = { cls: "blue", icon: <Loader2 size={38} className="spin" />, title: "Backup em andamento…", text: "Pode continuar usando o computador normalmente." };
  } else {
    hero = { cls: "green", icon: <CheckCircle2 size={40} />, title: "Seus backups estão em dia", text: "Está tudo funcionando. Não é preciso fazer nada." };
  }

  const upcoming = routines
    .map((v) => ({ v, next: nextRun(v.routine.schedule, v.routine.enabled) }))
    .filter((x) => x.next)
    .sort((a, b) => a.next!.getTime() - b.next!.getTime())
    .slice(0, 4);

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Início</h1>
          <p>Resumo das cópias de segurança deste computador</p>
        </div>
        <div className="spacer" />
        <button className="btn primary big" onClick={() => go("wizard")}>
          <Plus size={20} /> Nova rotina de backup
        </button>
      </div>

      <div className={`hero ${hero.cls}`}>
        <div className="hero-icon">{hero.icon}</div>
        <div style={{ flex: 1 }}>
          <h2>{hero.title}</h2>
          <p>{hero.text}</p>
        </div>
        {(failed.length > 0 || late.length > 0) && (
          <button
            className="btn big"
            onClick={() => guarded(() => api.runNow((failed[0] ?? late[0]).v.routine.id), "Backup iniciado!").then(reload)}
          >
            <Play size={18} /> Tentar agora
          </button>
        )}
      </div>

      {running.length > 0 && (
        <>
          <h2 style={{ fontSize: 18 }}>Em andamento agora</h2>
          {running.map((r) => (
            <RunCard key={r.id} run={r} />
          ))}
        </>
      )}

      <div className="grid2">
        <div className="card">
          <h2>
            <CalendarClock size={18} style={{ verticalAlign: -3, marginRight: 6 }} />
            Próximos backups
          </h2>
          {upcoming.length === 0 ? (
            <p className="muted">Nenhum backup automático agendado.</p>
          ) : (
            <table className="tbl">
              <tbody>
                {upcoming.map(({ v, next }) => (
                  <tr key={v.routine.id}>
                    <td>
                      <b>{v.routine.name}</b>
                      <div className="small muted">{describeSchedule(v.routine.schedule)}</div>
                    </td>
                    <td style={{ textAlign: "right" }}>{fmtNext(next)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
        <div className="card">
          <h2>Últimos resultados</h2>
          {!recent || recent.length === 0 ? (
            <p className="muted">Nenhum backup foi feito ainda.</p>
          ) : (
            <table className="tbl">
              <tbody>
                {recent.map((r) => (
                  <tr key={r.id} className="click" onClick={() => setDetail(r.id)}>
                    <td>
                      <b>{r.routine_name}</b>
                      <div className="small muted">{fmtDateTime(r.started_at)}</div>
                    </td>
                    <td style={{ textAlign: "right" }}>
                      <StatusBadge status={r.status} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>

      {routines.length > 0 && (
        <div className="card">
          <h2>Executar um backup agora</h2>
          <div className="row">
            {routines.map((v) => (
              <button
                key={v.routine.id}
                className="btn"
                disabled={!!v.running}
                onClick={() => guarded(() => api.runNow(v.routine.id), `Backup "${v.routine.name}" iniciado!`).then(reload)}
              >
                {v.running ? <Loader2 size={16} className="spin" /> : <Play size={16} />} {v.routine.name}
              </button>
            ))}
          </div>
        </div>
      )}
      {detail !== null && <RunDetails runId={detail} onClose={() => setDetail(null)} />}
    </>
  );
}
