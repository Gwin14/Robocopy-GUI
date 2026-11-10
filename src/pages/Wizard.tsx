import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft, ArrowRight, Check, CopyPlus, FolderPlus, HardDrive, History, Laptop, Loader2, Network, RefreshCw, Save, Trash2, Usb, Folder,
} from "lucide-react";
import { api, DestKind, DestTest, Drive, errText, KnownFolder, Mode, Routine, ScheduleKind } from "../api";
import { Alert, useApp } from "../ui";
import { DAY_SHORT, describeSchedule, destLabel, fmtBytes, MODE_INFO } from "../text";

const STEPS = ["O que copiar", "Para onde", "Como copiar", "Quando", "Revisar e salvar"];

function emptyRoutine(): Routine {
  return {
    id: "",
    name: "",
    enabled: true,
    sources: [],
    dest_kind: "usb",
    dest_path: "",
    dest_volume_label: null,
    dest_volume_serial: null,
    net_user: "",
    has_password: false,
    mode: "copy",
    keep_versions: 7,
    schedule: { kind: "daily", time: "22:00", days: [1, 2, 3, 4, 5], every_hours: 4, also_on_startup: false, wake: false },
    options: { threads: 8, retries: 2, wait_secs: 5, exclude_files: [], exclude_dirs: [], ipg_ms: 0, backup_mode: true, prescan: true },
    created_at: "",
    updated_at: "",
  };
}

export default function Wizard({ editId, done }: { editId?: string; done: () => void }) {
  const { guarded } = useApp();
  const [step, setStep] = useState(0);
  const [r, setR] = useState<Routine>(emptyRoutine());
  const [password, setPassword] = useState("");
  const [clearPw, setClearPw] = useState(false);
  const [known, setKnown] = useState<KnownFolder[]>([]);
  const [drives, setDrives] = useState<Drive[]>([]);
  const [test, setTest] = useState<DestTest | null>(null);
  const [testing, setTesting] = useState(false);
  const [mirrorOk, setMirrorOk] = useState(false);
  const [advanced, setAdvanced] = useState(false);
  const [saving, setSaving] = useState(false);
  const [loaded, setLoaded] = useState(!editId);

  useEffect(() => {
    api.knownFolders().then(setKnown).catch(() => {});
    refreshDrives();
    if (editId) {
      api.routine(editId).then((x) => {
        if (x) {
          setR(x);
          setMirrorOk(x.mode === "mirror");
        }
        setLoaded(true);
      });
    }
  }, [editId]);

  const refreshDrives = () => api.drives().then(setDrives).catch(() => {});
  const set = (patch: Partial<Routine>) => {
    setR((x) => ({ ...x, ...patch }));
    if ("dest_path" in patch || "dest_kind" in patch || "net_user" in patch) setTest(null);
  };
  const setSched = (patch: Partial<Routine["schedule"]>) => setR((x) => ({ ...x, schedule: { ...x.schedule, ...patch } }));
  const setOpt = (patch: Partial<Routine["options"]>) => setR((x) => ({ ...x, options: { ...x.options, ...patch } }));

  const toggleSource = (p: string) =>
    set({ sources: r.sources.some((s) => s.toLowerCase() === p.toLowerCase()) ? r.sources.filter((s) => s.toLowerCase() !== p.toLowerCase()) : [...r.sources, p] });

  const addFolders = async () => {
    const sel = await open({ directory: true, multiple: true, title: "Escolha as pastas para o backup" });
    if (!sel) return;
    const arr = Array.isArray(sel) ? sel : [sel];
    const next = [...r.sources];
    for (const p of arr) if (!next.some((s) => s.toLowerCase() === p.toLowerCase())) next.push(p);
    set({ sources: next });
  };

  const pickDestFolder = async () => {
    const sel = await open({ directory: true, multiple: false, title: "Escolha a pasta de destino do backup", defaultPath: r.dest_path || undefined });
    if (typeof sel === "string") set({ dest_path: sel });
  };

  const runTest = async () => {
    setTesting(true);
    try {
      setTest(await api.testDest(r.dest_kind, r.dest_path, r.net_user || null, password || null, r.id || null));
    } catch (e) {
      setTest({ ok: false, message: errText(e), removable: false });
    }
    setTesting(false);
  };

  const sameDisk =
    r.dest_kind !== "network" &&
    r.dest_path.length >= 2 &&
    r.sources.some((s) => s.substring(0, 2).toLowerCase() === r.dest_path.substring(0, 2).toLowerCase());

  const canNext = [
    r.sources.length > 0,
    r.dest_path.trim().length > 2 && (r.dest_kind !== "network" || r.dest_path.startsWith("\\\\")),
    r.mode !== "mirror" || mirrorOk,
    r.schedule.kind !== "weekly" || r.schedule.days.length > 0,
    r.name.trim().length > 0,
  ];

  const save = async () => {
    setSaving(true);
    const ok = await guarded(
      () => api.saveRoutine({ ...r, net_user: r.dest_kind === "network" ? r.net_user : null }, password || null, clearPw),
      editId ? "Rotina atualizada!" : "Rotina criada! O backup vai rodar automaticamente no horário escolhido."
    );
    setSaving(false);
    if (ok) done();
  };

  useEffect(() => {
    if (step === 4 && !r.name) {
      const base = r.schedule.kind === "daily" ? "Backup diário" : r.schedule.kind === "weekly" ? "Backup semanal" : "Backup";
      set({ name: r.dest_kind === "network" ? `${base} na rede` : r.dest_kind === "usb" ? `${base} no HD externo` : base });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step]);

  if (!loaded) return <div className="empty"><Loader2 className="spin" /></div>;

  return (
    <>
      <div className="page-head">
        <div>
          <h1>{editId ? `Editar "${r.name}"` : "Nova rotina de backup"}</h1>
          <p>Responda as perguntas abaixo. Você pode voltar e mudar a qualquer momento.</p>
        </div>
        <div className="spacer" />
        <button className="btn" onClick={done}>
          Cancelar
        </button>
      </div>

      <div className="steps">
        {STEPS.map((s, i) => (
          <div
            key={s}
            className={`step ${i === step ? "active" : ""} ${i < step ? "done" : ""}`}
            style={{ cursor: i < step || editId ? "pointer" : "default" }}
            onClick={() => (i < step || editId) && setStep(i)}
          >
            <span className="n">{i < step ? <Check size={14} /> : i + 1}</span>
            {s}
          </div>
        ))}
      </div>

      <div className="card">
        {step === 0 && (
          <>
            <h2>Quais pastas você quer proteger?</h2>
            <p className="muted">Clique nas pastas mais comuns ou adicione outras (por exemplo, a pasta do sistema da pousada).</p>
            <div className="pill-list" style={{ marginBottom: 18 }}>
              {known.map((k) => (
                <button key={k.path} className={`pill ${r.sources.some((s) => s.toLowerCase() === k.path.toLowerCase()) ? "sel" : ""}`} onClick={() => toggleSource(k.path)}>
                  {r.sources.some((s) => s.toLowerCase() === k.path.toLowerCase()) && <Check size={14} style={{ verticalAlign: -2 }} />} {k.label}
                </button>
              ))}
            </div>
            {r.sources.map((s) => (
              <div className="src-item" key={s}>
                <Folder size={18} color="#2563eb" />
                <span className="mono">{s}</span>
                <button className="btn sm ghost danger" onClick={() => toggleSource(s)}>
                  <Trash2 size={14} /> Remover
                </button>
              </div>
            ))}
            <button className="btn big" onClick={addFolders}>
              <FolderPlus size={18} /> Adicionar outra pasta…
            </button>
          </>
        )}

        {step === 1 && (
          <>
            <h2>Para onde vai o backup?</h2>
            <div className="choice-grid" style={{ marginBottom: 20 }}>
              {(
                [
                  ["usb", <Usb size={28} />, "HD externo / pendrive", "Um disco ligado no USB. Bom para guardar fora do computador."],
                  ["network", <Network size={28} />, "Pasta na rede", "Um NAS, servidor ou outro computador da pousada."],
                  ["local", <Laptop size={28} />, "Outra pasta / disco", "Outro disco interno deste computador."],
                ] as [DestKind, JSX.Element, string, string][]
              ).map(([k, icon, t, d]) => (
                <button key={k} className={`choice ${r.dest_kind === k ? "sel" : ""}`} onClick={() => set({ dest_kind: k, dest_path: "" })}>
                  <div className="ci">{icon}</div>
                  <b>{t}</b>
                  <p>{d}</p>
                </button>
              ))}
            </div>

            {r.dest_kind === "usb" && (
              <>
                <div className="row between">
                  <h3>Escolha o disco</h3>
                  <button className="btn sm" onClick={refreshDrives}>
                    <RefreshCw size={14} /> Atualizar lista
                  </button>
                </div>
                <p className="muted small">Se o HD externo não aparecer, conecte-o e clique em "Atualizar lista".</p>
                <div className="choice-grid">
                  {drives
                    .filter((d) => d.root.toUpperCase() !== "C:\\")
                    .map((d) => (
                      <button
                        key={d.root}
                        className={`choice ${r.dest_path.toUpperCase().startsWith(d.root.toUpperCase()) ? "sel" : ""}`}
                        onClick={() => set({ dest_path: `${d.root}Backups` })}
                      >
                        <div className="ci">{d.removable ? <Usb size={24} /> : <HardDrive size={24} />}</div>
                        <b>
                          {d.root} {d.label || (d.removable ? "Disco removível" : "Disco")}
                        </b>
                        <p>
                          {fmtBytes(d.free)} livres de {fmtBytes(d.total)}
                        </p>
                      </button>
                    ))}
                </div>
                {drives.filter((d) => d.root.toUpperCase() !== "C:\\").length === 0 && <Alert kind="warn">Nenhum disco externo encontrado. Conecte o HD e clique em "Atualizar lista".</Alert>}
                {r.dest_path && (
                  <label className="field" style={{ marginTop: 16 }}>
                    <span>Pasta dentro do disco</span>
                    <div className="row" style={{ flexWrap: "nowrap" }}>
                      <input type="text" value={r.dest_path} onChange={(e) => set({ dest_path: e.target.value })} />
                      <button className="btn" onClick={pickDestFolder}>
                        Procurar…
                      </button>
                    </div>
                    <small>Se a letra do disco mudar (ex.: de E: para F:), o programa encontra o disco sozinho.</small>
                  </label>
                )}
              </>
            )}

            {r.dest_kind === "network" && (
              <>
                <label className="field">
                  <span>Endereço da pasta na rede</span>
                  <input type="text" placeholder="\\NAS\Backup" value={r.dest_path} onChange={(e) => set({ dest_path: e.target.value.replace(/\//g, "\\") })} />
                  <small>Formato: \\nome-ou-IP-do-servidor\pasta. Exemplo: \\192.168.0.10\Backups</small>
                </label>
                <div className="grid2">
                  <label className="field">
                    <span>Usuário (se pedir)</span>
                    <input type="text" placeholder="ex.: backup ou NAS\backup" value={r.net_user ?? ""} onChange={(e) => set({ net_user: e.target.value })} />
                  </label>
                  <label className="field">
                    <span>Senha (se pedir)</span>
                    <input
                      type="password"
                      placeholder={r.has_password && !clearPw ? "•••••• (senha salva, deixe vazio para manter)" : ""}
                      value={password}
                      onChange={(e) => {
                        setPassword(e.target.value);
                        setTest(null);
                      }}
                    />
                  </label>
                </div>
                {r.has_password && (
                  <label className="check">
                    <input type="checkbox" checked={clearPw} onChange={(e) => setClearPw(e.target.checked)} />
                    <span>Remover a senha salva</span>
                  </label>
                )}
                <p className="small muted">A senha fica guardada de forma criptografada no Windows, e só o sistema de backup consegue lê-la.</p>
              </>
            )}

            {r.dest_kind === "local" && (
              <label className="field">
                <span>Pasta de destino</span>
                <div className="row" style={{ flexWrap: "nowrap" }}>
                  <input type="text" value={r.dest_path} onChange={(e) => set({ dest_path: e.target.value })} />
                  <button className="btn" onClick={pickDestFolder}>
                    Procurar…
                  </button>
                </div>
              </label>
            )}

            {sameDisk && (
              <Alert kind="warn">
                O destino está no <b>mesmo disco</b> que os arquivos originais. Isso protege contra arquivos apagados por engano, mas <b>não</b> protege se o disco quebrar. O ideal é usar um HD externo ou a rede.
              </Alert>
            )}

            {r.dest_path && (
              <div className="row" style={{ marginTop: 8 }}>
                <button className="btn" onClick={runTest} disabled={testing}>
                  {testing ? <Loader2 size={16} className="spin" /> : <Check size={16} />} Testar destino
                </button>
              </div>
            )}
            {test && (
              <div style={{ marginTop: 14 }}>
                <Alert kind={test.ok ? "ok" : "err"}>
                  {test.message}
                  {test.ok && test.free != null && (
                    <div className="small">
                      Espaço livre: <b>{fmtBytes(test.free)}</b> de {fmtBytes(test.total)}
                    </div>
                  )}
                </Alert>
              </div>
            )}
          </>
        )}

        {step === 2 && (
          <>
            <h2>Como o backup deve funcionar?</h2>
            <div className="choice-grid">
              {(
                [
                  ["copy", <CopyPlus size={28} />, "Recomendado", false],
                  ["versions", <History size={28} />, "Mais proteção", false],
                  ["mirror", <RefreshCw size={28} />, "Cuidado: apaga arquivos", true],
                ] as [Mode, JSX.Element, string, boolean][]
              ).map(([m, icon, tag, warn]) => (
                <button key={m} className={`choice ${r.mode === m ? "sel" : ""}`} onClick={() => set({ mode: m })}>
                  <div className="ci">{icon}</div>
                  <b>{MODE_INFO[m].title}</b>
                  <p>{MODE_INFO[m].desc}</p>
                  <span className={`tag ${warn ? "warn" : ""}`}>{tag}</span>
                </button>
              ))}
            </div>
            {r.mode === "versions" && (
              <label className="field" style={{ marginTop: 18, maxWidth: 380 }}>
                <span>Quantas versões guardar?</span>
                <input type="number" min={1} max={365} value={r.keep_versions} onChange={(e) => set({ keep_versions: Math.max(1, parseInt(e.target.value) || 1) })} />
                <small>
                  As mais antigas são apagadas automaticamente. Cada versão é uma cópia completa, então {r.keep_versions} versões ocupam cerca de {r.keep_versions}× o tamanho
                  das pastas.
                </small>
              </label>
            )}
            {r.mode === "mirror" && (
              <div style={{ marginTop: 18 }}>
                <Alert kind="err">
                  No modo Espelho, <b>arquivos apagados no computador também serão apagados do backup</b> na próxima execução. Se alguém apagar algo por engano, ou se um
                  vírus apagar os arquivos, o backup também perde esses arquivos.
                </Alert>
                <label className="check">
                  <input type="checkbox" checked={mirrorOk} onChange={(e) => setMirrorOk(e.target.checked)} />
                  <span>Entendi e quero usar o modo Espelho mesmo assim</span>
                </label>
              </div>
            )}
          </>
        )}

        {step === 3 && (
          <>
            <h2>Quando o backup deve rodar?</h2>
            <div className="pill-list" style={{ marginBottom: 20 }}>
              {(
                [
                  ["daily", "Todos os dias"],
                  ["weekly", "Em dias da semana"],
                  ["interval", "A cada algumas horas"],
                  ["startup", "Ao ligar o computador"],
                  ["manual", "Só quando eu mandar"],
                ] as [ScheduleKind, string][]
              ).map(([k, l]) => (
                <button key={k} className={`pill ${r.schedule.kind === k ? "sel" : ""}`} onClick={() => setSched({ kind: k })}>
                  {l}
                </button>
              ))}
            </div>
            {r.schedule.kind === "weekly" && (
              <label className="field">
                <span>Quais dias?</span>
                <div className="pill-list">
                  {DAY_SHORT.map((d, i) => (
                    <button
                      key={d}
                      className={`pill ${r.schedule.days.includes(i) ? "sel" : ""}`}
                      onClick={() => setSched({ days: r.schedule.days.includes(i) ? r.schedule.days.filter((x) => x !== i) : [...r.schedule.days, i] })}
                    >
                      {d}
                    </button>
                  ))}
                </div>
              </label>
            )}
            {(r.schedule.kind === "daily" || r.schedule.kind === "weekly" || r.schedule.kind === "interval") && (
              <div className="grid2" style={{ maxWidth: 560 }}>
                <label className="field">
                  <span>{r.schedule.kind === "interval" ? "Começando às" : "Horário"}</span>
                  <input type="time" value={r.schedule.time} onChange={(e) => setSched({ time: e.target.value || "22:00" })} />
                </label>
                {r.schedule.kind === "interval" && (
                  <label className="field">
                    <span>Repetir a cada</span>
                    <select value={r.schedule.every_hours} onChange={(e) => setSched({ every_hours: parseInt(e.target.value) })}>
                      {[1, 2, 3, 4, 6, 8, 12].map((h) => (
                        <option key={h} value={h}>
                          {h} hora{h > 1 ? "s" : ""}
                        </option>
                      ))}
                    </select>
                  </label>
                )}
              </div>
            )}
            {r.schedule.kind !== "manual" && r.schedule.kind !== "startup" && (
              <>
                <label className="check">
                  <input type="checkbox" checked={r.schedule.also_on_startup} onChange={(e) => setSched({ also_on_startup: e.target.checked })} />
                  <span>Também fazer backup ao ligar o computador</span>
                </label>
                <label className="check">
                  <input type="checkbox" checked={r.schedule.wake} onChange={(e) => setSched({ wake: e.target.checked })} />
                  <span>Acordar o computador se ele estiver em suspensão (não liga se estiver desligado)</span>
                </label>
              </>
            )}
            <Alert kind="info">
              Se o computador estiver desligado no horário, o backup roda assim que ele for ligado. Os backups rodam mesmo com este programa fechado e sem ninguém
              logado.
            </Alert>
          </>
        )}

        {step === 4 && (
          <>
            <h2>Revise e dê um nome</h2>
            <label className="field" style={{ maxWidth: 520 }}>
              <span>Nome da rotina</span>
              <input type="text" value={r.name} onChange={(e) => set({ name: e.target.value })} placeholder="ex.: Backup diário da recepção" />
            </label>
            <div className="kv" style={{ gridTemplateColumns: "160px 1fr", fontSize: 15 }}>
              <span>O que copiar</span>
              <span className="mono">
                {r.sources.map((s) => (
                  <div key={s}>{s}</div>
                ))}
              </span>
              <span>Para onde</span>
              <span className="mono">{destLabel(r)}</span>
              <span>Como</span>
              <span>
                {MODE_INFO[r.mode].title}
                {r.mode === "versions" ? `, guardando as últimas ${r.keep_versions}` : ""}
              </span>
              <span>Quando</span>
              <span>{describeSchedule(r.schedule)}</span>
            </div>

            <div style={{ marginTop: 20 }}>
              <button className="btn sm ghost" onClick={() => setAdvanced(!advanced)}>
                {advanced ? "▾" : "▸"} Opções avançadas (para técnicos)
              </button>
            </div>
            {advanced && (
              <div className="card" style={{ marginTop: 10, background: "#fbfcfe" }}>
                <div className="grid3">
                  <label className="field">
                    <span>Cópias simultâneas (/MT)</span>
                    <input type="number" min={1} max={128} value={r.options.threads} onChange={(e) => setOpt({ threads: parseInt(e.target.value) || 8 })} />
                  </label>
                  <label className="field">
                    <span>Tentativas por arquivo (/R)</span>
                    <input type="number" min={0} max={100} value={r.options.retries} onChange={(e) => setOpt({ retries: parseInt(e.target.value) || 0 })} />
                  </label>
                  <label className="field">
                    <span>Espera entre tentativas, s (/W)</span>
                    <input type="number" min={0} max={600} value={r.options.wait_secs} onChange={(e) => setOpt({ wait_secs: parseInt(e.target.value) || 0 })} />
                  </label>
                </div>
                <label className="field">
                  <span>Ignorar arquivos (separe por espaço)</span>
                  <input type="text" placeholder="*.mp4 *.iso" value={r.options.exclude_files.join(" ")} onChange={(e) => setOpt({ exclude_files: e.target.value.split(/\s+/).filter(Boolean) })} />
                  <small>Já são ignorados: Thumbs.db, desktop.ini, ~$* e *.tmp</small>
                </label>
                <label className="field">
                  <span>Ignorar pastas (uma por linha)</span>
                  <textarea rows={2} value={r.options.exclude_dirs.join("\n")} onChange={(e) => setOpt({ exclude_dirs: e.target.value.split("\n").map((x) => x.trim()).filter(Boolean) })} />
                </label>
                <label className="field" style={{ maxWidth: 380 }}>
                  <span>Limitar uso da rede, ms entre pacotes (/IPG)</span>
                  <input type="number" min={0} max={2000} value={r.options.ipg_ms} onChange={(e) => setOpt({ ipg_ms: parseInt(e.target.value) || 0 })} />
                  <small>0 = sem limite. Usar um valor desativa as cópias simultâneas.</small>
                </label>
                <label className="check">
                  <input type="checkbox" checked={r.options.prescan} onChange={(e) => setOpt({ prescan: e.target.checked })} />
                  <span>Calcular o tamanho antes de copiar (mostra porcentagem e tempo restante)</span>
                </label>
                <label className="check">
                  <input type="checkbox" checked={r.options.backup_mode} onChange={(e) => setOpt({ backup_mode: e.target.checked })} />
                  <span>Modo backup (/B): copia até arquivos com permissão restrita</span>
                </label>
              </div>
            )}
            <Alert kind="info">
              Ao salvar, o Windows vai pedir permissão de administrador para criar o agendamento. Clique em <b>Sim</b>.
            </Alert>
          </>
        )}

        <div className="wizard-foot">
          <button className="btn big" disabled={step === 0} onClick={() => setStep(step - 1)}>
            <ArrowLeft size={18} /> Voltar
          </button>
          {step < 4 ? (
            <button
              className="btn primary big"
              disabled={!canNext[step]}
              onClick={() => {
                if (step === 1 && !test && r.dest_path) runTest();
                setStep(step + 1);
              }}
            >
              Próximo <ArrowRight size={18} />
            </button>
          ) : (
            <button className="btn primary big" disabled={!canNext.every(Boolean) || saving} onClick={save}>
              {saving ? <Loader2 size={18} className="spin" /> : <Save size={18} />} Salvar rotina
            </button>
          )}
        </div>
      </div>
      {step === 1 && test && !test.ok && <p className="small muted">Você pode continuar mesmo assim, mas o backup só vai funcionar quando o destino estiver acessível.</p>}
    </>
  );
}
