import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { FileInput, FileOutput } from "lucide-react";
import { formatDateTime } from "../../lib/extraction";
import { exchangeSummaryText, maintenance, type ExchangeExportResult } from "../../lib/maintenance";
import { Card, ErrorNote, useAction, useLoad } from "./shared";

// Deferred double review: hand a package to the other reviewer and import theirs.
export function ExchangeCard({ projectId, projectName, onImported }: { projectId: string; projectName: string; onImported: () => Promise<void> }) {
  const history = useLoad(() => maintenance.exchangeHistory(projectId), [projectId]);
  const [exported, setExported] = useState<ExchangeExportResult | null>(null);
  const [imported, setImported] = useState<string | null>(null);
  const action = useAction();
  const base = projectName.replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-|-$/g, "").slice(0, 60) || "revision";

  async function exportPackage() {
    const path = await save({ defaultPath: `${base}-intercambio.json`, filters: [{ name: "Paquete de intercambio", extensions: ["json"] }] });
    if (!path) return;
    const result = await action.run(() => maintenance.exportExchange(projectId, path));
    if (result) {
      setExported(result);
      setImported(null);
    }
  }

  async function importPackage() {
    const path = await open({ multiple: false, filters: [{ name: "Paquete de intercambio", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    const summary = await action.run(() => maintenance.importExchange(path));
    if (!summary) return;
    setExported(null);
    setImported(summary.projectId === projectId ? exchangeSummaryText(summary) : `El paquete pertenece a otra revisión («${summary.projectName}»); se importó allí.`);
    await history.reload();
    await onImported();
  }

  return (
    <Card title="Paquete de intercambio">
      <p className="text-[11px] leading-4 text-slate-500">Para revisión doble sin servidor: exporta tus decisiones, puntajes y valores para el otro revisor e importa los suyos. Los acuerdos se finalizan y los desacuerdos abren conflictos.</p>
      <div className="mt-3 flex flex-wrap gap-2">
        <button className="button-secondary px-3 py-1.5 text-[11px]" disabled={action.busy} onClick={() => void exportPackage()}><FileOutput className="size-3.5" /> Exportar paquete</button>
        <button className="button-secondary px-3 py-1.5 text-[11px]" disabled={action.busy} onClick={() => void importPackage()}><FileInput className="size-3.5" /> Importar paquete</button>
      </div>
      {exported && (
        <p className="mt-3 rounded-lg bg-teal-50 px-3 py-2 text-[11px] text-teal-900">
          {exported.articles} estudios · {exported.decisions} decisiones · {exported.scores} puntajes · {exported.values} valores
          <span className="block break-all font-mono text-[10px] text-teal-700">{exported.path}</span>
          <span className="block break-all font-mono text-[10px] text-teal-700">SHA-256 {exported.sha256}</span>
        </p>
      )}
      {imported && <p className="mt-3 rounded-lg bg-teal-50 px-3 py-2 text-[11px] text-teal-900">{imported}</p>}
      <div className="mt-2"><ErrorNote message={action.error} /></div>
      {history.data && history.data.length > 0 && (
        <ul className="mt-3 space-y-1 border-t border-slate-100 pt-2 text-[11px] text-slate-500">
          {history.data.map((entry) => (
            <li key={entry.sha256}>{formatDateTime(entry.importedAt)} · de {entry.reviewerName} · {entry.summary.decisionsAdded} decisiones, {entry.summary.valuesAdded} valores</li>
          ))}
        </ul>
      )}
      <p className="mt-3 text-[11px] leading-4 text-slate-400">El paquete contiene metadatos, no PDF. Cada revisor debe tener su nombre en Configuración → Perfil local.</p>
    </Card>
  );
}
