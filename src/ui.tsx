import React, { createContext, useContext, useEffect, useRef, useState } from "react";
import { AlertTriangle, CheckCircle2, Info, Loader2, Lock, XCircle } from "lucide-react";
import { api, errText, Run } from "./api";
import { STATUS_INFO } from "./text";

// ---------------- Toast + PIN (contexto global) ----------------

interface Ctx {
  toast: (msg: string, kind?: "ok" | "err") => void;
  /** Executa uma ação protegida; se precisar de PIN, pede e tenta de novo. Retorna true se deu certo. */
  guarded: (fn: () => Promise<unknown>, okMsg?: string) => Promise<boolean>;
}

const AppCtx = createContext<Ctx>({ toast: () => {}, guarded: async () => false });
export const useApp = () => useContext(AppCtx);

export function AppProvider({ children, onStatusChange }: { children: React.ReactNode; onStatusChange: () => void }) {
  const [t, setT] = useState<{ msg: string; kind: "ok" | "err" } | null>(null);
  const timer = useRef<number>();
  const [pinReq, setPinReq] = useState<((ok: boolean) => void) | null>(null);

  const toast = (msg: string, kind: "ok" | "err" = "ok") => {
    setT({ msg, kind });
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setT(null), kind === "err" ? 9000 : 4500);
  };

  const askPin = () => new Promise<boolean>((resolve) => setPinReq(() => resolve));

  const guarded = async (fn: () => Promise<unknown>, okMsg?: string) => {
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        await fn();
        if (okMsg) toast(okMsg);
        onStatusChange();
        return true;
      } catch (e) {
        const m = errText(e);
        if (m === "PIN_REQUIRED" && attempt === 0) {
          const ok = await askPin();
          onStatusChange();
          if (!ok) return false;
          continue;
        }
        toast(m, "err");
        return false;
      }
    }
    return false;
  };

  return (
    <AppCtx.Provider value={{ toast, guarded }}>
      {children}
      {t && (
        <div className={`toast ${t.kind === "err" ? "err" : ""}`} onClick={() => setT(null)}>
          {t.kind === "err" ? <XCircle size={20} /> : <CheckCircle2 size={20} />}
          <div style={{ whiteSpace: "pre-wrap" }}>{t.msg}</div>
        </div>
      )}
      {pinReq && (
        <PinDialog
          onDone={(ok) => {
            pinReq(ok);
            setPinReq(null);
          }}
        />
      )}
    </AppCtx.Provider>
  );
}

function PinDialog({ onDone }: { onDone: (ok: boolean) => void }) {
  const [pin, setPin] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    setBusy(true);
    const ok = await api.unlock(pin).catch(() => false);
    setBusy(false);
    if (ok) onDone(true);
    else {
      setErr("PIN incorreto. Tente novamente.");
      setPin("");
    }
  };
  return (
    <Modal onClose={() => onDone(false)}>
      <div className="row" style={{ marginBottom: 8 }}>
        <Lock size={26} color="#2563eb" />
        <h2 style={{ margin: 0 }}>Área protegida</h2>
      </div>
      <p className="muted">Esta ação precisa do PIN do administrador do programa.</p>
      <input
        autoFocus
        type="password"
        inputMode="numeric"
        placeholder="Digite o PIN"
        value={pin}
        onChange={(e) => setPin(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        style={{ fontSize: 22, letterSpacing: 6, textAlign: "center" }}
      />
      {err && <p style={{ color: "var(--red)" }}>{err}</p>}
      <div className="actions">
        <button className="btn" onClick={() => onDone(false)}>
          Cancelar
        </button>
        <button className="btn primary" disabled={!pin || busy} onClick={submit}>
          Desbloquear
        </button>
      </div>
    </Modal>
  );
}

// ---------------- Componentes ----------------

export function Modal({ children, onClose, wide }: { children: React.ReactNode; onClose: () => void; wide?: boolean }) {
  useEffect(() => {
    const h = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);
  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={`modal ${wide ? "wide" : ""}`}>{children}</div>
    </div>
  );
}

export function Confirm({
  title,
  children,
  confirmText = "Confirmar",
  danger,
  onConfirm,
  onClose,
}: {
  title: string;
  children: React.ReactNode;
  confirmText?: string;
  danger?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <Modal onClose={onClose}>
      <h2>{title}</h2>
      <div className="muted" style={{ lineHeight: 1.5 }}>
        {children}
      </div>
      <div className="actions">
        <button className="btn" onClick={onClose}>
          Cancelar
        </button>
        <button
          className={`btn ${danger ? "danger solid" : "primary"}`}
          onClick={() => {
            onConfirm();
            onClose();
          }}
        >
          {confirmText}
        </button>
      </div>
    </Modal>
  );
}

export function StatusBadge({ status }: { status: Run["status"] }) {
  const i = STATUS_INFO[status] ?? STATUS_INFO.failed;
  const Icon =
    status === "success" ? CheckCircle2 : status === "warning" ? AlertTriangle : status === "running" ? Loader2 : status === "failed" ? XCircle : Info;
  return (
    <span className={`badge ${i.cls}`}>
      <Icon size={14} className={status === "running" ? "spin" : ""} />
      {i.label}
    </span>
  );
}

export function Alert({ kind, children }: { kind: "info" | "ok" | "warn" | "err"; children: React.ReactNode }) {
  const Icon = kind === "ok" ? CheckCircle2 : kind === "warn" ? AlertTriangle : kind === "err" ? XCircle : Info;
  return (
    <div className={`alert ${kind}`}>
      <Icon size={20} />
      <div>{children}</div>
    </div>
  );
}

export function Progress({ value, indeterminate }: { value: number; indeterminate?: boolean }) {
  return (
    <div className={`progress ${indeterminate ? "indeterminate" : ""}`}>
      <div style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
    </div>
  );
}

export function usePoll<T>(fn: () => Promise<T>, ms: number, deps: unknown[] = []): [T | undefined, () => void] {
  const [v, setV] = useState<T>();
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const load = () => {
    fnRef.current().then(setV).catch(() => {});
  };
  useEffect(() => {
    load();
    const t = window.setInterval(load, ms);
    return () => window.clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return [v, load];
}
