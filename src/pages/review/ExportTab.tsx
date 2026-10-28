import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Download, FileSpreadsheet } from "lucide-react";
import { formatDateTime } from "../../lib/extraction";
import { reviewBackend } from "../../lib/reviewBackend";
import type { ExportFormat, ExportResult } from "../../reviewTypes";
import { Card, ErrorNote, useAction, useLoad } from "./shared";

const formats: { format: ExportFormat; title: string; detail: string; extension: string; variant?: "compatible" | "extended"; scoped?: boolean }[] = [
  { format: "xlsx", variant: "compatible", title: "Excel compatible (7 hojas)", detail: "Leer, Control, RQs, Cadena, I_E, Calidad y Extraccion con las cabeceras del formato de evidencia.", extension: "xlsx" },
  { format: "xlsx", variant: "extended", title: "Excel ampliado con procedencia", detail: "Las 7 hojas más procedencia de cada valor (estado, cita, página), decisiones por revisor, calidad por revisor, auditoría y metadatos.", extension: "xlsx" },
  { format: "csv", title: "CSV de la matriz", detail: "Una fila por estudio incluido y una columna por aspecto (UTF-8).", extension: "csv" },
  { format: "json", title: "JSON completo", detail: "Instantánea versionada del proyecto para reproducibilidad o intercambio.", extension: "json" },
  { format: "ris", title: "RIS", detail: "Bibliografía para gestores de referencias.", extension: "ris", scoped: true },
  { format: "bibtex", title: "BibTeX", detail: "Bibliografía para LaTeX, con caracteres especiales escapados.", extension: "bib", scoped: true },
  { format: "markdown", title: "Markdown", detail: "Informe con cadenas, incluidos, tablas por RQ y anexo de evidencias.", extension: "md" },
  { format: "latex", title: "LaTeX", detail: "Documento con tablas por RQ (longtable), incluidos y anexo de evidencias.", extension: "tex" },
];

export function ExportTab({ projectId, projectName }: { projectId: string; projectName: string }) {
  const history = useLoad(() => reviewBackend.exportHistory(projectId), [projectId]);
  const [scope, setScope] = useState<"included" | "corpus">("included");
  const [result, setResult] = useState<ExportResult | null>(null);
  const action = useAction();
  const base = projectName.replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-|-$/g, "").slice(0, 60) || "revision";

  async function run(item: (typeof formats)[number]) {
    const suffix = item.variant === "extended" ? "-ampliado" : "";
    const path = await save({ defaultPath: `${base}${suffix}.${item.extension}`, filters: [{ name: item.title, extensions: [item.extension] }] });
    if (!path) return;
    const exported = await action.run(() => reviewBackend.exportProject({ projectId, format: item.format, variant: item.variant, scope: item.scoped ? scope : undefined, path }));
    if (exported) {
      setResult(exported);
      await history.reload();
    }
  }

  return (
    <div className="grid grid-cols-[1.5fr_1fr] gap-6">
      <Card title="Exportar" actions={
        <label className="flex items-center gap-2 text-[11px] text-slate-500">Bibliografía de
          <select className="input w-40" value={scope} onChange={(event) => setScope(event.target.value as "included" | "corpus")}><option value="included">Estudios incluidos</option><option value="corpus">Todo el corpus</option></select>
        </label>
      }>
        <div className="grid grid-cols-2 gap-3">
          {formats.map((item) => (
            <button key={`${item.format}-${item.variant ?? ""}`} className="flex items-start gap-3 rounded-xl border border-slate-100 p-3 text-left transition hover:border-teal-200 hover:bg-teal-50/30 disabled:opacity-50" disabled={action.busy} onClick={() => void run(item)}>
              <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-teal-50 text-teal-700">{item.format === "xlsx" ? <FileSpreadsheet className="size-4" /> : <Download className="size-4" />}</div>
              <span><span className="block text-xs font-semibold text-slate-700">{item.title}</span><span className="mt-0.5 block text-[11px] leading-4 text-slate-400">{item.detail}</span></span>
            </button>
          ))}
        </div>
        {result && (
          <p className="mt-4 rounded-lg bg-teal-50 px-3 py-2 text-xs text-teal-800">
            {result.summary} · {(result.bytes / 1024).toFixed(1)} KB<span className="block break-all font-mono text-[10px] text-teal-700">{result.path}</span><span className="block font-mono text-[10px] text-teal-700">SHA-256 {result.sha256}</span>
          </p>
        )}
        <div className="mt-3"><ErrorNote message={action.error} /></div>
        <p className="mt-3 text-[11px] leading-4 text-slate-400">El formato compatible usa contenido propio de ResearchPilot y las cabeceras del formato; no redistribuye la plantilla original. Los valores rechazados no se exportan y los estados sin valor se escriben explícitamente (por ejemplo, «No reportado»). PRISMA se exportará en la Fase 8.</p>
      </Card>
      <Card title="Historial de exportaciones">
        {history.data?.length ? (
          <ol className="space-y-2">
            {history.data.map((record) => (
              <li key={record.id} className="rounded-lg bg-slate-50 p-2.5 text-[11px]">
                <p className="font-semibold text-slate-700">{record.format.toUpperCase()}{record.options.variant && record.format === "xlsx" ? ` · ${record.options.variant === "extended" ? "ampliado" : "compatible"}` : ""} · {formatDateTime(record.createdAt)}</p>
                <p className="break-all text-slate-500">{record.outputPath}</p>
                {record.outputSha256 && <p className="break-all font-mono text-[10px] text-slate-400">{record.outputSha256}</p>}
              </li>
            ))}
          </ol>
        ) : <p className="text-xs text-slate-400">Cada exportación queda registrada con su huella SHA-256 y la versión del protocolo.</p>}
      </Card>
    </div>
  );
}
