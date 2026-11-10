import { useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, Clock, HardDrive, Loader2, Network, Pause, Pencil, Play, Plus, Trash2, XCircle, FolderOpen, PlayCircle } from "lucide-react";
import { api, errText, RoutineView } from "../api";
import { RunCard } from "../runs";
import { Confirm, Modal, useApp, usePoll } from "../ui";
import { describeSchedule, destLabel, fmtNext, fmtRelative, MODE_INFO, nextRun, routineHealth } from "../text";

export default function Routines({ go }: { go: (p: string, arg?: string) => void }) {
  const { guarded, toast } = useApp();
  const [list, reload] = usePoll<RoutineView[]>(api.routines, 2000);
  const [tasks, setTasks] = useState<Record<string, boolean>>({});
  const [del, setDel] = useState<RoutineView | null>(null);
  const [delHist, setDelHist] = useState(false);
  const [pause, setPause] = useState<RoutineView | null>(null);

  useEffect(() => {
    api.checkTasks().then(setTasks).catch(() => {});
  }, [list?.length]);

  if (!list) return <div className="empty"><Loader2 className="spin" /></div>;

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Rotinas de backup</h1>
          <p>Cada rotina diz o que copiar, para onde e quando</p>
        </div>
        <div className="spacer" />
        <button className="btn primary big" onClick={() => go("wizard")}>
          <Plus size={20} /> Nova rotina
        </button>
      </div>

      {list.length === 0 && (
        <div className="card empty">
          <HardDrive size={48} />
          <h3>Nenhuma rotina ainda</h3>
          <p>Uma rotina é uma "receita" de backup: quais pastas copiar, para onde e de quanto em quanto tempo.</p>
          <button className="btn primary big" onClick={() => go("wizard")}>
            <Plus size={18} /> Criar a primeira rotina
          </button>
        </div>
      )}

      {list.map((v) => {
        const r = v.routine;
        const h = routineHealth(r, v.last_run, v.last_success);
        const cls = v.running ? "blue" : h === "ok" ? "green" : h === "failed" ? "red" : h === "paused" ? "gray" : "yellow";
        const Icon = v.running ? Loader2 : h === "ok" ? CheckCircle2 : h === "failed" ? XCircle : h === "paused" ? Pause : AlertTriangle;
        const taskMissing = tasks[r.id] === false;
        return (
          <div key={r.id}>
            <div className="card">
              <div className="routine">
                <div className={`routine-icon ${cls}`}>
                  <Icon size={26} className={v.running ? "spin" : ""} />
                </div>
                <div className="routine-body">
                  <div className="row between">
                    <h3>{r.name}</h3>
                    <div className="row">
                      <button className="btn primary" disabled={!!v.running} onClick={() => guarded(() => api.runNow(r.id), `Backup "${r.name}" iniciado!`).then(reload)}>
                        <Play size={16} /> Executar agora
                      </button>
                      <button className="btn" onClick={() => go("wizard", r.id)}>
                        <Pencil size={16} /> Editar
                      </button>
                      <button className="btn" onClick={() => (r.enabled ? setPause(v) : guarded(() => api.setEnabled(r.id, true), "Agendamento reativado.").then(reload))}>
                        {r.enabled ? <Pause size={16} /> : <PlayCircle size={16} />} {r.enabled ? "Pausar" : "Reativar"}
                      </button>
                      <button className="btn ghost danger" title="Excluir rotina" onClick={() => setDel(v)}>
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </div>
                  <div className="kv">
                    <span>O que copia</span>
                    <span className="mono ellipsis" title={r.sources.join("\n")}>
                      {r.sources.join("  •  ")}
                    </span>
                    <span>Para onde</span>
                    <span className="row" style={{ gap: 6 }}>
                      {r.dest_kind === "network" ? <Network size={15} /> : <HardDrive size={15} />}
                      <span className="mono">{destLabel(r)}</span>
                      <button
                        className="btn sm ghost"
                        title="Abrir pasta do backup"
                        onClick={() =>
                          api
                            .backupLocation(r.id, null)
                            .then((p) => api.openPath(r.mode === "versions" ? p.substring(0, p.lastIndexOf("\\")) : p))
                            .catch((e) => toast(errText(e), "err"))
                        }
                      >
                        <FolderOpen size={14} />
                      </button>
                    </span>
                    <span>Tipo</span>
                    <span>
                      {MODE_INFO[r.mode].title}
                      {r.mode === "versions" ? ` (guarda as últimas ${r.keep_versions})` : ""}
                    </span>
                    <span>Quando</span>
                    <span>
                      <Clock size={14} style={{ verticalAlign: -2 }} /> {describeSchedule(r.schedule, r.enabled)}
                      {nextRun(r.schedule, r.enabled) && <span className="muted"> · próximo: {fmtNext(nextRun(r.schedule, r.enabled))}</span>}
                    </span>
                    <span>Último backup</span>
                    <span>
                      {v.last_run ? (
                        <>
                          {fmtRelative(v.last_run.finished_at ?? v.last_run.started_at)} ·{" "}
                          <span style={{ color: v.last_run.status === "failed" ? "var(--red)" : v.last_run.status === "warning" ? "var(--yellow)" : "var(--green)" }}>
                            {v.last_run.message}
                          </span>
                        </>
                      ) : (
                        <span className="muted">Ainda não executado</span>
                      )}
                    </span>
                  </div>
                  {taskMissing && (
                    <div className="alert warn" style={{ marginTop: 12, marginBottom: 0 }}>
                      <AlertTriangle size={18} />
                      <div>
                        O agendamento desta rotina não existe no Windows, então ela não vai rodar sozinha.{" "}
                        <button className="btn sm" onClick={() => guarded(() => api.repairTasks(), "Agendamentos reparados!").then(() => api.checkTasks().then(setTasks))}>
                          Reparar agora
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            </div>
            {v.running && <RunCard run={v.running} />}
          </div>
        );
      })}

      {pause && (
        <Confirm
          title={`Pausar "${pause.routine.name}"?`}
          confirmText="Pausar"
          onClose={() => setPause(null)}
          onConfirm={() => guarded(() => api.setEnabled(pause.routine.id, false), "Rotina pausada.").then(reload)}
        >
          A rotina não vai mais rodar automaticamente até você reativá-la. Ainda será possível executá-la manualmente.
        </Confirm>
      )}
      {del && (
        <Modal onClose={() => setDel(null)}>
          <h2>Excluir a rotina "{del.routine.name}"?</h2>
          <p className="muted">
            A rotina e o agendamento serão removidos. <b>Os arquivos que já estão no backup NÃO serão apagados</b>. Eles continuam no destino.
          </p>
          <label className="check">
            <input type="checkbox" checked={delHist} onChange={(e) => setDelHist(e.target.checked)} />
            <span>Também apagar o histórico desta rotina</span>
          </label>
          <div className="actions">
            <button className="btn" onClick={() => setDel(null)}>
              Cancelar
            </button>
            <button
              className="btn danger solid"
              onClick={() => {
                const id = del.routine.id;
                setDel(null);
                guarded(() => api.deleteRoutine(id, delHist), "Rotina excluída.").then(reload);
              }}
            >
              <Trash2 size={16} /> Excluir
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
