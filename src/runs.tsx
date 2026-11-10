import { useEffect, useState } from "react";
import { FileText, FolderOpen, Square } from "lucide-react";
import { api, errText, Run } from "./api";
import { Confirm, Modal, Progress, StatusBadge, useApp } from "./ui";
import { fmtBytes, fmtDateTime, fmtDuration, fmtNum, PHASE_LABEL, runDuration, TRIGGER_LABEL } from "./text";

export function RunCard({ run }: { run: Run }) {
  const { guarded } = useApp();
  const [ask, setAsk] = useState(false);
  const hasTotal = run.total_bytes > 0 || run.total_files > 0;
  const pct = run.total_bytes > 0 ? (run.done_bytes / run.total_bytes) * 100 : run.total_files > 0 ? (run.done_files / run.total_files) * 100 : 0;
  const copying = run.phase === "copying";
  const remaining = run.speed_bps > 0 && run.total_bytes > 0 ? (run.total_bytes - run.done_bytes) / run.speed_bps : NaN;
  return (
    <div className="card run-card">
      <div className="row between">
        <div>
          <h3 style={{ margin: 0 }}>{run.routine_name}</h3>
          <div className="muted small">
            {PHASE_LABEL[run.phase] ?? run.phase} · iniciado {fmtDateTime(run.started_at)} · {TRIGGER_LABEL[run.trigger] ?? run.trigger}
          </div>
        </div>
        <button className="btn danger" disabled={run.cancel_requested} onClick={() => setAsk(true)}>
          <Square size={16} /> {run.cancel_requested ? "Parando…" : "Parar"}
        </button>
      </div>
      <div style={{ margin: "16px 0 6px" }}>
        <Progress value={pct} indeterminate={!copying || !hasTotal} />
      </div>
      <div className="row between small">
        <span>{copying && hasTotal ? `${Math.min(100, Math.floor(pct))}% concluído` : PHASE_LABEL[run.phase]}</span>
        <span className="muted">
          {copying && isFinite(remaining)
            ? `Falta cerca de ${fmtDuration(remaining)}`
            : copying && run.done_files === 0
              ? "Arquivos grandes podem levar alguns minutos para aparecer"
              : ""}
        </span>
      </div>
      <div className="run-meta muted">
        <div>
          Arquivos copiados
          <b>
            {fmtNum(run.done_files)}
            {run.total_files > 0 ? ` de ${fmtNum(run.total_files)}` : ""}
          </b>
        </div>
        <div>
          Dados copiados
          <b>
            {fmtBytes(run.done_bytes)}
            {run.total_bytes > 0 ? ` de ${fmtBytes(run.total_bytes)}` : ""}
          </b>
        </div>
        <div>
          Velocidade<b>{run.speed_bps > 0 ? `${fmtBytes(run.speed_bps)}/s` : "-"}</b>
        </div>
        <div>
          Problemas<b style={{ color: run.failed_files ? "var(--red)" : undefined }}>{fmtNum(run.failed_files)}</b>
        </div>
      </div>
      {run.current_file && (
        <div className="small muted ellipsis mono" style={{ marginTop: 10 }} title={run.current_file}>
          {run.current_file}
        </div>
      )}
      {ask && (
        <Confirm
          title="Parar este backup?"
          confirmText="Sim, parar"
          danger
          onClose={() => setAsk(false)}
          onConfirm={() => guarded(() => api.cancel(run.id), "Pedido de parada enviado.")}
        >
          Os arquivos que já foram copiados continuam no backup. Você pode executar de novo depois e ele continua de onde parou.
        </Confirm>
      )}
    </div>
  );
}

export function RunDetails({ runId, onClose }: { runId: number; onClose: () => void }) {
  const { toast } = useApp();
  const [run, setRun] = useState<Run | null>(null);
  const [log, setLog] = useState<string | null>(null);

  useEffect(() => {
    const load = () => api.run(runId).then(setRun).catch(() => {});
    load();
    const t = window.setInterval(load, 1500);
    return () => window.clearInterval(t);
  }, [runId]);

  if (!run) return null;
  const isRestore = run.trigger === "restore";
  return (
    <Modal wide onClose={onClose}>
      <div className="row between">
        <h2 style={{ margin: 0 }}>{run.routine_name}</h2>
        <StatusBadge status={run.status} />
      </div>
      <p className="muted" style={{ marginTop: 6 }}>
        {fmtDateTime(run.started_at)} · duração {runDuration(run)} · {TRIGGER_LABEL[run.trigger] ?? run.trigger}
      </p>
      {run.message && (
        <div className={`alert ${run.status === "success" ? "ok" : run.status === "warning" ? "warn" : run.status === "failed" ? "err" : "info"}`}>
          <div>{run.message}</div>
        </div>
      )}
      <div className="grid3" style={{ gridTemplateColumns: "repeat(4,1fr)" }}>
        <div className="card stat">
          <div className="label">Arquivos copiados</div>
          <div className="value">{fmtNum(run.done_files)}</div>
        </div>
        <div className="card stat">
          <div className="label">Já estavam em dia</div>
          <div className="value">{fmtNum(run.skipped_files)}</div>
        </div>
        <div className="card stat">
          <div className="label">Dados copiados</div>
          <div className="value">{fmtBytes(run.done_bytes)}</div>
        </div>
        <div className="card stat">
          <div className="label">Com problema</div>
          <div className="value" style={{ color: run.failed_files ? "var(--red)" : undefined }}>
            {fmtNum(run.failed_files)}
          </div>
        </div>
      </div>
      {run.mode === "mirror" && run.extra_files > 0 && (
        <p className="small muted">{fmtNum(run.extra_files)} arquivo(s) foram removidos do backup porque não existem mais no computador (modo Espelho).</p>
      )}
      {run.details.length > 0 && (
        <>
          <h3>Avisos e problemas</h3>
          <div className="errlist">
            {run.details.map((d, i) => (
              <div key={i}>{d}</div>
            ))}
          </div>
        </>
      )}
      {run.dest_folder && (
        <p className="small">
          <span className="muted">{isRestore ? "Restaurado em: " : "Gravado em: "}</span>
          <span className="mono">{run.dest_folder}</span>
        </p>
      )}
      {log !== null && <pre className="logbox">{log}</pre>}
      <div className="actions">
        {run.dest_folder && (
          <button className="btn" onClick={() => api.openPath(run.dest_folder!).catch((e) => toast(errText(e), "err"))}>
            <FolderOpen size={16} /> Abrir pasta
          </button>
        )}
        {run.log_path && (
          <button
            className="btn"
            onClick={() =>
              log !== null
                ? setLog(null)
                : api
                    .log(run.id)
                    .then(setLog)
                    .catch((e) => toast(errText(e), "err"))
            }
          >
            <FileText size={16} /> {log !== null ? "Esconder registro técnico" : "Ver registro técnico"}
          </button>
        )}
        <button className="btn primary" onClick={onClose}>
          Fechar
        </button>
      </div>
    </Modal>
  );
}
