import { useCallback, useEffect, useState } from "react";
import { History as HistoryIcon, Home, ListChecks, Loader2, Lock, RotateCcw, Settings as SettingsIcon, ShieldCheck, Unlock } from "lucide-react";
import { api, errText, Run, Status } from "./api";
import { Alert, AppProvider, useApp, usePoll } from "./ui";
import Dashboard from "./pages/Dashboard";
import Routines from "./pages/Routines";
import Wizard from "./pages/Wizard";
import History from "./pages/History";
import Restore from "./pages/Restore";
import SettingsPage from "./pages/SettingsPage";

type Page = "home" | "routines" | "wizard" | "history" | "restore" | "settings";

export default function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [loadErr, setLoadErr] = useState("");
  const refresh = useCallback(() => {
    api
      .status()
      .then((s) => {
        setStatus(s);
        setLoadErr("");
      })
      .catch((e) => setLoadErr(errText(e)));
  }, []);
  useEffect(() => {
    refresh();
    const t = window.setInterval(refresh, 20000);
    return () => window.clearInterval(t);
  }, [refresh]);

  return (
    <AppProvider onStatusChange={refresh}>
      {loadErr ? (
        <div className="setup">
          <Alert kind="err">Não foi possível iniciar: {loadErr}</Alert>
        </div>
      ) : !status ? (
        <div className="empty" style={{ marginTop: 120 }}>
          <Loader2 className="spin" size={32} />
        </div>
      ) : !status.initialized ? (
        <FirstSetup onDone={refresh} />
      ) : (
        <Shell status={status} refresh={refresh} />
      )}
    </AppProvider>
  );
}

function FirstSetup({ onDone }: { onDone: () => void }) {
  const { guarded } = useApp();
  const [busy, setBusy] = useState(false);
  return (
    <div className="setup">
      <div className="brand-logo" style={{ width: 84, height: 84, margin: "0 auto 18px", borderRadius: 22 }}>
        <ShieldCheck size={46} />
      </div>
      <h1>Bem-vindo ao RoboBackup</h1>
      <p className="muted" style={{ fontSize: 16 }}>
        Este programa faz cópias de segurança automáticas dos arquivos importantes deste computador.
      </p>
      <div className="card" style={{ marginTop: 26 }}>
        <h2>Antes de começar</h2>
        <p>
          É preciso preparar o computador uma única vez. O Windows vai perguntar se você permite que o programa faça alterações. Clique em <b>Sim</b>.
        </p>
        <ul className="muted" style={{ lineHeight: 1.7 }}>
          <li>Cria a pasta onde ficam o histórico e os registros dos backups</li>
          <li>Protege as senhas de rede salvas</li>
          <li>Permite que os backups rodem sozinhos, mesmo com o programa fechado</li>
        </ul>
        <button
          className="btn primary big"
          style={{ width: "100%", marginTop: 10 }}
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            await guarded(async () => {
              await api.setup();
              try {
                const { enable } = await import("@tauri-apps/plugin-autostart");
                await enable();
              } catch {
                /* opcional */
              }
            }, "Computador preparado! Agora crie sua primeira rotina de backup.");
            setBusy(false);
            onDone();
          }}
        >
          {busy ? <Loader2 className="spin" size={18} /> : <ShieldCheck size={18} />} Preparar o computador
        </button>
      </div>
    </div>
  );
}

function Shell({ status, refresh }: { status: Status; refresh: () => void }) {
  const { guarded } = useApp();
  const [page, setPage] = useState<Page>("home");
  const [editId, setEditId] = useState<string | undefined>();
  const [active] = usePoll<Run[]>(api.activeRuns, 2000);

  const go = (p: string, arg?: string) => {
    setEditId(p === "wizard" ? arg : undefined);
    setPage(p as Page);
  };

  const nav: [Page, string, JSX.Element][] = [
    ["home", "Início", <Home size={20} />],
    ["routines", "Rotinas", <ListChecks size={20} />],
    ["history", "Histórico", <HistoryIcon size={20} />],
    ["restore", "Restaurar", <RotateCcw size={20} />],
    ["settings", "Configurações", <SettingsIcon size={20} />],
  ];

  return (
    <div className="app">
      <aside className="side">
        <div className="brand">
          <div className="brand-logo">
            <ShieldCheck size={22} />
          </div>
          <div>
            <b>RoboBackup</b>
            <small>Cópias de segurança</small>
          </div>
        </div>
        <nav className="nav">
          {nav.map(([p, label, icon]) => (
            <button key={p} className={page === p || (p === "routines" && page === "wizard") ? "active" : ""} onClick={() => go(p)}>
              {icon}
              {label}
              {p === "home" && active && active.length > 0 && <span className="count">{active.length}</span>}
            </button>
          ))}
        </nav>
        <div className="side-foot">
          {status.has_pin && (
            <div>
              {status.unlocked ? (
                <button onClick={() => api.lock().then(refresh)}>
                  <Unlock size={12} /> Desbloqueado · bloquear
                </button>
              ) : (
                <span>
                  <Lock size={12} /> Modo protegido
                </span>
              )}
            </div>
          )}
          <div style={{ marginTop: 8 }}>Fechar a janela não para os backups. O programa continua perto do relógio.</div>
        </div>
      </aside>
      <main className="main">
        {status.exe_mismatch && (
          <Alert kind="warn">
            O programa foi movido ou atualizado e os agendamentos precisam ser atualizados.{" "}
            <button className="btn sm" onClick={() => guarded(() => api.repairTasks(), "Agendamentos atualizados!")}>
              Atualizar agora
            </button>
          </Alert>
        )}
        {page === "home" && <Dashboard go={go} />}
        {page === "routines" && <Routines go={go} />}
        {page === "wizard" && <Wizard key={editId ?? "new"} editId={editId} done={() => go("routines")} />}
        {page === "history" && <History />}
        {page === "restore" && <Restore />}
        {page === "settings" && <SettingsPage status={status} refresh={refresh} />}
      </main>
    </div>
  );
}
