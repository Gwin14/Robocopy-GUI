import { useState } from "react";
import { History as HistoryIcon, Loader2 } from "lucide-react";
import { api, RoutineView, Run } from "../api";
import { RunDetails } from "../runs";
import { StatusBadge, usePoll } from "../ui";
import { fmtBytes, fmtDateTime, fmtNum, runDuration, TRIGGER_LABEL } from "../text";

export default function History() {
  const [routineId, setRoutineId] = useState("");
  const [status, setStatus] = useState("");
  const [routines] = usePoll<RoutineView[]>(api.routines, 15000);
  const [runs] = usePoll<Run[]>(() => api.runs(routineId || null, status || null, 500), 3000, [routineId, status]);
  const [detail, setDetail] = useState<number | null>(null);

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Histórico</h1>
          <p>Todos os backups e restaurações já feitos. Clique em um para ver os detalhes.</p>
        </div>
      </div>
      <div className="card">
        <div className="row" style={{ marginBottom: 14 }}>
          <select style={{ maxWidth: 300 }} value={routineId} onChange={(e) => setRoutineId(e.target.value)}>
            <option value="">Todas as rotinas</option>
            {routines?.map((v) => (
              <option key={v.routine.id} value={v.routine.id}>
                {v.routine.name}
              </option>
            ))}
          </select>
          <select style={{ maxWidth: 240 }} value={status} onChange={(e) => setStatus(e.target.value)}>
            <option value="">Todos os resultados</option>
            <option value="success">Concluídos</option>
            <option value="warning">Com avisos</option>
            <option value="failed">Com falha</option>
            <option value="canceled">Interrompidos</option>
            <option value="running">Em andamento</option>
          </select>
        </div>
        {!runs ? (
          <div className="empty">
            <Loader2 className="spin" />
          </div>
        ) : runs.length === 0 ? (
          <div className="empty">
            <HistoryIcon size={44} />
            <p>Nenhum registro encontrado.</p>
          </div>
        ) : (
          <table className="tbl">
            <thead>
              <tr>
                <th>Data</th>
                <th>Rotina</th>
                <th>Resultado</th>
                <th>Arquivos</th>
                <th>Dados</th>
                <th>Duração</th>
                <th>Origem</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((r) => (
                <tr key={r.id} className="click" onClick={() => setDetail(r.id)}>
                  <td>{fmtDateTime(r.started_at)}</td>
                  <td>
                    <b>{r.routine_name}</b>
                    {r.message && (
                      <div className="small muted ellipsis" style={{ maxWidth: 380 }} title={r.message}>
                        {r.message}
                      </div>
                    )}
                  </td>
                  <td>
                    <StatusBadge status={r.status} />
                  </td>
                  <td>
                    {fmtNum(r.done_files)}
                    {r.failed_files > 0 && <span style={{ color: "var(--red)" }}> ({r.failed_files} erro)</span>}
                  </td>
                  <td>{fmtBytes(r.done_bytes)}</td>
                  <td>{runDuration(r)}</td>
                  <td className="small muted">{TRIGGER_LABEL[r.trigger] ?? r.trigger}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {detail !== null && <RunDetails runId={detail} onClose={() => setDetail(null)} />}
    </>
  );
}
