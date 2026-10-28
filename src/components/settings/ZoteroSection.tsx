import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, LoaderCircle, LibraryBig } from "lucide-react";
import { errorMessage } from "../../lib/backend";
import { maintenance, type ZoteroImportSummary } from "../../lib/maintenance";

export function ZoteroSection({ onImported }: { onImported?: () => void }) {
  const [dataDir, setDataDir] = useState("");
  const [includePdfs, setIncludePdfs] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [summary, setSummary] = useState<ZoteroImportSummary | null>(null);

  useEffect(() => {
    maintenance.zoteroDefaultDir().then((dir) => { if (dir) setDataDir((current) => current || dir); }).catch(() => undefined);
  }, []);

  async function choose() {
    const selected = await open({ directory: true, multiple: false, title: "Carpeta de datos de Zotero" });
    if (typeof selected === "string") setDataDir(selected);
  }

  async function runImport() {
    setBusy(true);
    setError(null);
    try {
      setSummary(await maintenance.importZotero(dataDir, includePdfs));
      onImported?.();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex gap-2">
        <input className="input flex-1 font-mono text-[11px]" placeholder="Carpeta de datos de Zotero (contiene zotero.sqlite)" value={dataDir} onChange={(event) => setDataDir(event.target.value)} aria-label="Carpeta de datos de Zotero" />
        <button type="button" className="button-secondary px-3 py-2" onClick={() => void choose()}><FolderOpen className="size-3.5" /> Elegir</button>
      </div>
      <label className="flex items-start gap-2 text-xs text-slate-600">
        <input type="checkbox" className="mt-0.5 accent-teal-600" checked={includePdfs} onChange={(event) => setIncludePdfs(event.target.checked)} />
        <span>Importar también los PDF adjuntos<span className="block text-[11px] text-slate-400">Se copian a la biblioteca y, si está activado, se encola su extracción científica.</span></span>
      </label>
      <button type="button" className="button-secondary px-3 py-2" disabled={busy || dataDir.trim().length === 0} onClick={() => void runImport()}>
        {busy ? <LoaderCircle className="size-3.5 animate-spin" /> : <LibraryBig className="size-3.5" />} Importar desde Zotero
      </button>
      {summary && (
        <div className="rounded-lg bg-teal-50 px-3 py-2 text-xs text-teal-900">
          <p>
            {summary.items} referencias leídas · {summary.imported} nuevas ({summary.pdfsAttached} con PDF) · {summary.linkedExisting} ya estaban en la biblioteca · {summary.alreadyLinked} importadas antes · {summary.collections} colecciones
          </p>
          {summary.errors.length > 0 && (
            <ul className="mt-1 list-disc pl-4 text-[11px] text-amber-800">{summary.errors.slice(0, 10).map((item) => <li key={item}>{item}</li>)}</ul>
          )}
        </div>
      )}
      {error && <p className="text-[11px] text-rose-700">{error}</p>}
      <p className="text-[11px] text-slate-400">Se lee una copia de la base de Zotero; Zotero puede seguir abierto y su biblioteca no se modifica. Las notas, anotaciones y elementos de la papelera no se importan. Volver a importar solo añade lo nuevo.</p>
    </div>
  );
}
