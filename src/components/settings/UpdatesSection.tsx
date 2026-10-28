import { useEffect, useState } from "react";
import { Download, LoaderCircle, RefreshCw, RotateCw } from "lucide-react";
import { errorMessage } from "../../lib/backend";
import { checkForUpdate, currentVersion, installUpdate, restartApp, type Update } from "../../lib/updates";

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "available"; update: Update }
  | { kind: "installing"; update: Update; progress: number | null }
  | { kind: "installed"; update: Update }
  | { kind: "error"; message: string };

export function UpdatesSection({ checkOnStart, onCheckOnStartChange }: { checkOnStart: boolean; onCheckOnStartChange: (value: boolean) => void }) {
  const [version, setVersion] = useState<string | null>(null);
  const [state, setState] = useState<State>({ kind: "idle" });

  useEffect(() => {
    currentVersion().then(setVersion).catch(() => setVersion(null));
  }, []);

  async function check() {
    setState({ kind: "checking" });
    try {
      const update = await checkForUpdate();
      setState(update ? { kind: "available", update } : { kind: "latest" });
    } catch (cause) {
      setState({ kind: "error", message: errorMessage(cause) });
    }
  }

  async function install(update: Update) {
    setState({ kind: "installing", update, progress: null });
    try {
      await installUpdate(update, (progress) => setState({ kind: "installing", update, progress }));
      setState({ kind: "installed", update });
    } catch (cause) {
      setState({ kind: "error", message: errorMessage(cause) });
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-4">
        <p className="text-xs text-slate-600">Versión instalada: <span className="font-semibold">{version ?? "—"}</span></p>
        <button type="button" className="button-secondary px-3 py-2" disabled={state.kind === "checking" || state.kind === "installing"} onClick={() => void check()}>
          {state.kind === "checking" ? <LoaderCircle className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />} Buscar actualizaciones
        </button>
      </div>

      {state.kind === "latest" && <p className="text-[11px] text-emerald-700">Tienes la versión más reciente publicada.</p>}
      {state.kind === "error" && <p className="text-[11px] text-rose-700">No se pudo completar: {state.message}</p>}
      {(state.kind === "available" || state.kind === "installing" || state.kind === "installed") && (
        <div className="rounded-xl bg-teal-50/60 p-3">
          <p className="text-xs font-semibold text-slate-800">ResearchPilot {state.update.version} disponible{state.update.date ? ` · ${new Date(state.update.date).toLocaleDateString("es")}` : ""}</p>
          {state.update.body && <p className="mt-1 max-h-40 overflow-y-auto whitespace-pre-wrap text-[11px] leading-4 text-slate-600">{state.update.body}</p>}
          {state.kind === "available" && (
            <button type="button" className="button-primary mt-3 px-3 py-2" onClick={() => void install(state.update)}><Download className="size-3.5" /> Descargar e instalar</button>
          )}
          {state.kind === "installing" && (
            <div className="mt-3">
              <div className="h-1.5 overflow-hidden rounded-full bg-white" role="progressbar" aria-valuenow={state.progress === null ? undefined : Math.round(state.progress * 100)} aria-valuemin={0} aria-valuemax={100}>
                <div className={`h-full rounded-full bg-teal-500 ${state.progress === null ? "w-1/3 animate-pulse" : ""}`} style={state.progress === null ? undefined : { width: `${Math.round(state.progress * 100)}%` }} />
              </div>
              <p className="mt-1 text-[11px] text-slate-500">Descargando y verificando la firma…</p>
            </div>
          )}
          {state.kind === "installed" && (
            <button type="button" className="button-primary mt-3 px-3 py-2" onClick={() => void restartApp()}><RotateCw className="size-3.5" /> Reiniciar para terminar</button>
          )}
        </div>
      )}

      <label className="flex items-start gap-2 text-xs text-slate-600">
        <input type="checkbox" className="mt-0.5 accent-teal-600" checked={checkOnStart} onChange={(event) => onCheckOnStartChange(event.target.checked)} />
        <span>Buscar actualizaciones al abrir la app<span className="block text-[11px] text-slate-400">Consulta el archivo de versiones de GitHub; no envía documentos ni datos. No se consulta automáticamente en modo solo local.</span></span>
      </label>
      <p className="text-[11px] text-slate-400">Solo se instalan paquetes firmados con la clave del proyecto; tus datos y la biblioteca no se modifican al actualizar.</p>
    </div>
  );
}
