import { useEffect, useState } from "react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { FolderOpen, KeyRound, Lock, Save, Wrench } from "lucide-react";
import { api, errText, Settings, Status } from "../api";
import { Alert, useApp } from "../ui";

export default function SettingsPage({ status, refresh }: { status: Status | null; refresh: () => void }) {
  const { guarded, toast } = useApp();
  const [s, setS] = useState<Settings | null>(null);
  const [auto, setAuto] = useState<boolean | null>(null);
  const [curPin, setCurPin] = useState("");
  const [newPin, setNewPin] = useState("");
  const [newPin2, setNewPin2] = useState("");

  useEffect(() => {
    api.settings().then(setS).catch(() => {});
    isEnabled().then(setAuto).catch(() => setAuto(null));
  }, []);

  const toggleAuto = async (v: boolean) => {
    try {
      if (v) await enable();
      else await disable();
      setAuto(await isEnabled());
      toast(v ? "O programa vai abrir junto com o Windows (na bandeja)." : "O programa não vai mais abrir sozinho.");
    } catch (e) {
      toast(errText(e), "err");
    }
  };

  const savePin = async () => {
    if (newPin !== newPin2) {
      toast("Os dois PINs digitados não são iguais.", "err");
      return;
    }
    const ok = await guarded(() => api.setPin(curPin, newPin), newPin ? "PIN salvo!" : "PIN removido.");
    if (ok) {
      setCurPin("");
      setNewPin("");
      setNewPin2("");
      refresh();
    }
  };

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Configurações</h1>
          <p>Ajustes gerais do programa</p>
        </div>
      </div>

      <div className="card">
        <h2>Geral</h2>
        <label className="check">
          <input type="checkbox" checked={!!auto} disabled={auto === null} onChange={(e) => toggleAuto(e.target.checked)} />
          <span>
            <b>Abrir o programa junto com o Windows</b>
            <div className="small muted">Fica perto do relógio (bandeja) mostrando avisos. Os backups agendados funcionam mesmo sem isso.</div>
          </span>
        </label>
        {s && (
          <>
            <label className="check">
              <input type="checkbox" checked={s.notifications} onChange={(e) => setS({ ...s, notifications: e.target.checked })} />
              <span>
                <b>Mostrar avisos quando um backup terminar ou falhar</b>
              </span>
            </label>
            <label className="field" style={{ maxWidth: 420, marginTop: 12 }}>
              <span>Avisar que o backup está atrasado depois de quantos dias sem sucesso?</span>
              <input type="number" min={1} max={60} value={s.overdue_days} onChange={(e) => setS({ ...s, overdue_days: parseInt(e.target.value) || 2 })} />
            </label>
            <button className="btn primary" onClick={() => guarded(() => api.saveSettings(s.notifications, s.overdue_days), "Configurações salvas.")}>
              <Save size={16} /> Salvar
            </button>
          </>
        )}
      </div>

      <div className="card">
        <h2>
          <KeyRound size={18} style={{ verticalAlign: -3, marginRight: 6 }} />
          PIN do administrador
        </h2>
        <p className="muted">
          Com um PIN, só quem sabe o número pode criar, editar ou excluir rotinas, restaurar para o local original e mudar configurações. Qualquer pessoa pode ver o
          andamento e mandar executar um backup.
        </p>
        {status?.has_pin && (
          <Alert kind="info">
            O PIN está ativado.{" "}
            {status.unlocked && (
              <button className="btn sm" onClick={() => api.lock().then(refresh)}>
                <Lock size={14} /> Bloquear agora
              </button>
            )}
          </Alert>
        )}
        <div className="grid3">
          {status?.has_pin && (
            <label className="field">
              <span>PIN atual</span>
              <input type="password" inputMode="numeric" value={curPin} onChange={(e) => setCurPin(e.target.value)} />
            </label>
          )}
          <label className="field">
            <span>{status?.has_pin ? "Novo PIN (vazio = remover)" : "Novo PIN"}</span>
            <input type="password" inputMode="numeric" value={newPin} onChange={(e) => setNewPin(e.target.value)} />
          </label>
          <label className="field">
            <span>Repita o novo PIN</span>
            <input type="password" inputMode="numeric" value={newPin2} onChange={(e) => setNewPin2(e.target.value)} />
          </label>
        </div>
        <button className="btn primary" disabled={!status?.has_pin && !newPin} onClick={savePin}>
          <Save size={16} /> {status?.has_pin ? (newPin ? "Trocar PIN" : "Remover PIN") : "Ativar PIN"}
        </button>
      </div>

      <div className="card">
        <h2>
          <Wrench size={18} style={{ verticalAlign: -3, marginRight: 6 }} />
          Manutenção
        </h2>
        <p className="muted">Se os backups automáticos pararem de funcionar (por exemplo, depois de reinstalar ou mover o programa), use o botão abaixo.</p>
        <div className="row">
          <button className="btn" onClick={() => guarded(() => api.repairTasks(), "Agendamentos recriados com sucesso!")}>
            <Wrench size={16} /> Reparar agendamentos
          </button>
          {status && (
            <button className="btn" onClick={() => api.openPath(status.data_dir).catch((e) => toast(errText(e), "err"))}>
              <FolderOpen size={16} /> Abrir pasta de dados e registros
            </button>
          )}
        </div>
        {status && (
          <p className="small muted" style={{ marginTop: 14 }}>
            Computador: {status.computer} · Dados em <span className="mono">{status.data_dir}</span> · RoboBackup 1.0
          </p>
        )}
      </div>
    </>
  );
}
