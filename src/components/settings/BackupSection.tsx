import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Archive, ArchiveRestore, LoaderCircle, RotateCw, Stethoscope, X } from "lucide-react";
import { errorMessage } from "../../lib/backend";
import { formatBytes, maintenance, type BackupInfo, type BackupResult, type Diagnostics } from "../../lib/maintenance";
import { restartApp } from "../../lib/updates";

type Busy = "backup" | "inspect" | "restore" | "diagnose" | null;

export function BackupSection() {
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<BackupResult | null>(null);
  const [candidate, setCandidate] = useState<{ path: string; info: BackupInfo } | null>(null);
  const [staged, setStaged] = useState<BackupInfo | null>(null);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);

  async function run<T>(kind: Busy, task: () => Promise<T>): Promise<T | null> {
    setBusy(kind);
    setError(null);
    try {
      return await task();
    } catch (cause) {
      setError(errorMessage(cause));
      return null;
    } finally {
      setBusy(null);
    }
  }

  async function backup() {
    const stamp = new Date().toISOString().slice(0, 10);
    const path = await save({ defaultPath: `researchpilot-respaldo-${stamp}.zip`, filters: [{ name: "Respaldo de ResearchPilot", extensions: ["zip"] }] });
    if (!path) return;
    const result = await run("backup", () => maintenance.createBackup(path));
    if (result) setCreated(result);
  }

  async function chooseBackup() {
    const path = await open({ multiple: false, filters: [{ name: "Respaldo de ResearchPilot", extensions: ["zip"] }] });
    if (typeof path !== "string") return;
    const info = await run("inspect", () => maintenance.inspectBackup(path));
    if (info) setCandidate({ path, info });
  }

  async function restore() {
    if (!candidate) return;
    const info = await run("restore", () => maintenance.stageRestore(candidate.path));
    if (info) {
      setStaged(info);
      setCandidate(null);
    }
  }

  async function cancel() {
    if (await run("restore", () => maintenance.cancelRestore()) !== null) setStaged(null);
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2">
        <button type="button" className="button-secondary px-3 py-2" disabled={busy !== null} onClick={() => void backup()}>
          {busy === "backup" ? <LoaderCircle className="size-3.5 animate-spin" /> : <Archive className="size-3.5" />} Crear respaldo
        </button>
        <button type="button" className="button-secondary px-3 py-2" disabled={busy !== null || staged !== null} onClick={() => void chooseBackup()}>
          {busy === "inspect" ? <LoaderCircle className="size-3.5 animate-spin" /> : <ArchiveRestore className="size-3.5" />} Restaurar…
        </button>
        <button type="button" className="button-secondary px-3 py-2" disabled={busy !== null} onClick={() => void run("diagnose", () => maintenance.diagnostics()).then((value) => value && setDiagnostics(value))}>
          {busy === "diagnose" ? <LoaderCircle className="size-3.5 animate-spin" /> : <Stethoscope className="size-3.5" />} Diagnóstico
        </button>
      </div>

      {created && (
        <p className="rounded-lg bg-teal-50 px-3 py-2 text-xs text-teal-800">
          Respaldo creado: {created.files} archivos · {formatBytes(created.bytes)}
          <span className="block break-all font-mono text-[10px] text-teal-700">{created.path}</span>
          <span className="block break-all font-mono text-[10px] text-teal-700">SHA-256 {created.sha256}</span>
        </p>
      )}

      {candidate && (
        <div className="rounded-xl border border-amber-200 bg-amber-50/50 p-3 text-xs text-slate-700" role="alertdialog" aria-label="Confirmar restauración">
          <p className="font-semibold">Respaldo verificado del {new Date(candidate.info.createdAt).toLocaleString("es")}</p>
          <p className="mt-1 text-[11px] text-slate-600">
            ResearchPilot {candidate.info.appVersion} · {candidate.info.articles} artículos · {candidate.info.projects} revisiones · {candidate.info.files} archivos ({formatBytes(candidate.info.bytes)}). Todas las huellas coinciden.
          </p>
          <p className="mt-2 text-[11px] text-slate-600">Restaurar reemplaza la biblioteca y las revisiones actuales. Antes se guarda automáticamente un respaldo del estado actual, y el cambio se aplica al reiniciar.</p>
          <div className="mt-3 flex gap-2">
            <button type="button" className="button-primary px-3 py-2" disabled={busy !== null} onClick={() => void restore()}>
              {busy === "restore" && <LoaderCircle className="size-3.5 animate-spin" />} Preparar restauración
            </button>
            <button type="button" className="button-secondary px-3 py-2" onClick={() => setCandidate(null)}>Cancelar</button>
          </div>
        </div>
      )}

      {staged && (
        <div className="rounded-xl bg-teal-50/60 p-3 text-xs text-slate-700">
          <p className="font-semibold">Restauración lista</p>
          {staged.preRestoreBackup && <p className="mt-1 break-all text-[11px] text-slate-600">Respaldo del estado actual: <span className="font-mono">{staged.preRestoreBackup}</span></p>}
          <div className="mt-3 flex gap-2">
            <button type="button" className="button-primary px-3 py-2" onClick={() => void restartApp()}><RotateCw className="size-3.5" /> Reiniciar y restaurar</button>
            <button type="button" className="button-secondary px-3 py-2" disabled={busy !== null} onClick={() => void cancel()}><X className="size-3.5" /> Descartar</button>
          </div>
        </div>
      )}

      {diagnostics && <DiagnosticsView diagnostics={diagnostics} />}
      {error && <p className="text-[11px] text-rose-700">{error}</p>}
      <p className="text-[11px] text-slate-400">El respaldo incluye la base de datos y los PDF administrados, con la huella SHA-256 de cada archivo. Las claves de API permanecen en el Llavero y no se copian.</p>
    </div>
  );
}

function DiagnosticsView({ diagnostics }: { diagnostics: Diagnostics }) {
  const healthy = diagnostics.integrity === "ok" && diagnostics.foreignKeyViolations === 0 && diagnostics.missingPdfs === 0;
  return (
    <div className={`rounded-xl p-3 text-xs ${healthy ? "bg-emerald-50/70 text-emerald-900" : "bg-amber-50/70 text-amber-900"}`}>
      <p className="font-semibold">{healthy ? "Todo en orden" : "Hay aspectos que revisar"}</p>
      <dl className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 text-[11px]">
        <dt>Integridad de la base</dt><dd>{diagnostics.integrity === "ok" ? "Correcta" : diagnostics.integrity}</dd>
        <dt>Referencias inconsistentes</dt><dd>{diagnostics.foreignKeyViolations}</dd>
        <dt>Esquema</dt><dd>{diagnostics.schemaVersion} de {diagnostics.supportedSchemaVersion}</dd>
        <dt>Base de datos</dt><dd>{formatBytes(diagnostics.databaseBytes)}</dd>
        <dt>PDF administrados</dt><dd>{diagnostics.libraryFiles} · {formatBytes(diagnostics.libraryBytes)}</dd>
        <dt>Artículos / revisiones</dt><dd>{diagnostics.articles} / {diagnostics.projects}</dd>
        <dt>PDF que faltan</dt><dd>{diagnostics.missingPdfs}</dd>
      </dl>
      {diagnostics.missingPdfTitles.length > 0 && <p className="mt-2 text-[11px]">Faltan: {diagnostics.missingPdfTitles.join(" · ")}</p>}
      {diagnostics.pendingRestore && <p className="mt-2 text-[11px]">Hay una restauración preparada que se aplicará al reiniciar.</p>}
    </div>
  );
}
