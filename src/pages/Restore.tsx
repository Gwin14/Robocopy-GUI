import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileSearch, FolderOpen, FolderSearch, Loader2, RotateCcw, Undo2 } from "lucide-react";
import { api, errText, RoutineView, Run } from "../api";
import { RunCard } from "../runs";
import { Alert, useApp, usePoll } from "../ui";
import { fmtDateTime, fmtRelative, MODE_INFO } from "../text";

export default function Restore() {
  const { guarded, toast } = useApp();
  const [routines] = usePoll<RoutineView[]>(api.routines, 10000);
  const [active] = usePoll<Run[]>(api.activeRuns, 1000);
  const [rid, setRid] = useState("");
  const [versions, setVersions] = useState<Run[] | null>(null);
  const [runId, setRunId] = useState<number | null>(null);
  const [location, setLocation] = useState("");
  const [inner, setInner] = useState<string | null>(null);
  const [target, setTarget] = useState<"folder" | "original">("folder");
  const [folder, setFolder] = useState("");
  const [keepNewer, setKeepNewer] = useState(true);
  const [busy, setBusy] = useState(false);

  const routine = routines?.find((v) => v.routine.id === rid)?.routine;

  useEffect(() => {
    setVersions(null);
    setRunId(null);
    setInner(null);
    setLocation("");
    if (!rid) return;
    api
      .versions(rid)
      .then((v) => {
        setVersions(v);
        setRunId(v[0]?.id ?? null);
      })
      .catch((e) => toast(errText(e), "err"));
  }, [rid]);

  useEffect(() => {
    setInner(null);
    if (!rid) return;
    api
      .backupLocation(rid, routine?.mode === "versions" ? runId : null)
      .then(setLocation)
      .catch(() => setLocation(""));
  }, [rid, runId, routine?.mode]);

  const pickInner = async (dir: boolean) => {
    const sel = await open({ directory: dir, multiple: false, defaultPath: location || undefined, title: dir ? "Escolha a pasta dentro do backup" : "Escolha o arquivo dentro do backup" });
    if (typeof sel !== "string") return;
    if (location && !sel.toLowerCase().startsWith(location.toLowerCase())) {
      toast("Escolha algo de dentro da pasta do backup.", "err");
      return;
    }
    setInner(sel);
  };

  const start = async () => {
    if (!routine) return;
    setBusy(true);
    await guarded(
      () =>
        api.startRestore({
          routine_id: routine.id,
          run_id: routine.mode === "versions" ? runId : null,
          inner_path: inner,
          target,
          folder: target === "folder" ? folder : null,
          keep_newer: keepNewer,
        }),
      "Restauração iniciada! Acompanhe o progresso abaixo."
    );
    setBusy(false);
  };

  const restores = (active ?? []).filter((r) => r.trigger === "restore");

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Restaurar arquivos</h1>
          <p>Recupere arquivos apagados ou estragados a partir de um backup</p>
        </div>
      </div>

      {restores.map((r) => (
        <RunCard key={r.id} run={r} />
      ))}

      <div className="card">
        <h2>1. De qual backup?</h2>
        {!routines ? (
          <Loader2 className="spin" />
        ) : routines.length === 0 ? (
          <p className="muted">Ainda não existe nenhuma rotina de backup.</p>
        ) : (
          <div className="choice-grid">
            {routines.map((v) => (
              <button key={v.routine.id} className={`choice ${rid === v.routine.id ? "sel" : ""}`} onClick={() => setRid(v.routine.id)}>
                <b>{v.routine.name}</b>
                <p>
                  {MODE_INFO[v.routine.mode].short} · último sucesso {fmtRelative(v.last_success?.finished_at)}
                </p>
              </button>
            ))}
          </div>
        )}
      </div>

      {routine && (
        <>
          <div className="card">
            <h2>2. {routine.mode === "versions" ? "De qual dia?" : "Backup disponível"}</h2>
            {versions === null ? (
              <Loader2 className="spin" />
            ) : versions.length === 0 ? (
              <Alert kind="warn">Esta rotina ainda não tem nenhum backup concluído para restaurar.</Alert>
            ) : routine.mode === "versions" ? (
              <select value={runId ?? ""} onChange={(e) => setRunId(parseInt(e.target.value))} style={{ maxWidth: 420 }}>
                {versions.map((v) => (
                  <option key={v.id} value={v.id}>
                    {fmtDateTime(v.started_at)}
                    {v.status === "warning" ? " (com avisos)" : ""}
                  </option>
                ))}
              </select>
            ) : (
              <p>
                Última cópia: <b>{fmtDateTime(versions[0].finished_at)}</b>. No modo {MODE_INFO[routine.mode].short} existe uma única cópia, sempre atualizada.
              </p>
            )}
            {location && (
              <p className="small">
                <span className="muted">Local do backup: </span>
                <span className="mono">{location}</span>{" "}
                <button className="btn sm" onClick={() => api.openPath(location).catch((e) => toast(errText(e), "err"))}>
                  <FolderOpen size={14} /> Abrir no Explorer
                </button>
              </p>
            )}
            <p className="small muted">Dica: para recuperar só um ou dois arquivos, você também pode abrir o backup no Explorer e copiar e colar normalmente.</p>
          </div>

          <div className="card">
            <h2>3. O que restaurar?</h2>
            <div className="pill-list">
              <button className={`pill ${inner === null ? "sel" : ""}`} onClick={() => setInner(null)}>
                Tudo
              </button>
              <button className={`pill ${inner !== null ? "sel" : ""}`} onClick={() => pickInner(true)}>
                <FolderSearch size={14} style={{ verticalAlign: -2 }} /> Uma pasta específica…
              </button>
              <button className="pill" onClick={() => pickInner(false)}>
                <FileSearch size={14} style={{ verticalAlign: -2 }} /> Um arquivo específico…
              </button>
            </div>
            {inner && (
              <p className="mono small" style={{ marginTop: 10 }}>
                {inner}
              </p>
            )}
          </div>

          <div className="card">
            <h2>4. Para onde?</h2>
            <div className="choice-grid" style={{ gridTemplateColumns: "1fr 1fr" }}>
              <button className={`choice ${target === "folder" ? "sel" : ""}`} onClick={() => setTarget("folder")}>
                <b>Para uma pasta separada</b>
                <p>Os arquivos voltam numa pasta à sua escolha, sem mexer nos atuais. Depois você confere e copia o que precisar.</p>
                <span className="tag">Recomendado</span>
              </button>
              <button className={`choice ${target === "original" ? "sel" : ""}`} onClick={() => setTarget("original")}>
                <b>Para o lugar original</b>
                <p>Os arquivos voltam para onde estavam. Arquivos com o mesmo nome podem ser substituídos.</p>
                <span className="tag warn">Cuidado</span>
              </button>
            </div>
            {target === "folder" && (
              <div className="row" style={{ marginTop: 14, flexWrap: "nowrap" }}>
                <input type="text" value={folder} placeholder="Escolha uma pasta…" onChange={(e) => setFolder(e.target.value)} />
                <button
                  className="btn"
                  onClick={async () => {
                    const s = await open({ directory: true, multiple: false, title: "Onde colocar os arquivos restaurados?" });
                    if (typeof s === "string") setFolder(s);
                  }}
                >
                  Procurar…
                </button>
              </div>
            )}
            {target === "original" && (
              <div style={{ marginTop: 14 }}>
                <Alert kind="warn">Os arquivos do backup serão copiados por cima das pastas originais:</Alert>
                <div className="mono small">
                  {routine.sources.map((s) => (
                    <div key={s}>{s}</div>
                  ))}
                </div>
              </div>
            )}
            <label className="check" style={{ marginTop: 12 }}>
              <input type="checkbox" checked={keepNewer} onChange={(e) => setKeepNewer(e.target.checked)} />
              <span>Não substituir arquivos que estejam mais novos que os do backup</span>
            </label>
            <p className="small muted">Nada será apagado no destino; a restauração só adiciona ou substitui arquivos.</p>
            <div className="row" style={{ marginTop: 10 }}>
              <button
                className="btn primary big"
                disabled={busy || !versions?.length || (target === "folder" && !folder.trim())}
                onClick={start}
              >
                {busy ? <Loader2 size={18} className="spin" /> : target === "original" ? <Undo2 size={18} /> : <RotateCcw size={18} />} Restaurar agora
              </button>
              <span className="small muted">O Windows vai pedir permissão de administrador.</span>
            </div>
          </div>
        </>
      )}
    </>
  );
}
