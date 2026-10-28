import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { CopyPlus, FileUp, Pencil, Plus, Trash2 } from "lucide-react";
import { reviewBackend } from "../../lib/reviewBackend";
import type { SearchStrategy, StrategyImportResult, StrategyInput } from "../../reviewTypes";
import { Card, Empty, ErrorNote, Loading, useAction, useLoad } from "./shared";

const today = () => new Date().toISOString().slice(0, 10);

export function SearchTab({ projectId, onChange }: { projectId: string; onChange: () => void }) {
  const strategies = useLoad(() => reviewBackend.strategies(projectId), [projectId]);
  const [form, setForm] = useState<StrategyInput | null>(null);
  const [imported, setImported] = useState<StrategyImportResult | null>(null);
  const action = useAction();
  const data = strategies.data;

  const edit = (strategy: SearchStrategy, newVersion = false): StrategyInput => ({
    id: strategy.id, projectId, databaseName: strategy.databaseName, queryText: strategy.queryText, queryDate: newVersion ? today() : strategy.queryDate,
    resultCount: strategy.resultCount, notes: strategy.notes, filters: strategy.filters, language: strategy.language, periodStart: strategy.periodStart, periodEnd: strategy.periodEnd, newVersion,
  });

  async function save() {
    if (!form) return;
    const next = await action.run(() => reviewBackend.saveStrategy(form));
    if (next) {
      strategies.setData(next);
      setForm(null);
    }
  }

  async function importFile(strategy: SearchStrategy) {
    const selected = await open({ multiple: false, directory: false, filters: [{ name: "Resultados exportados", extensions: ["ris", "bib", "bibtex", "csv"] }] });
    if (!selected || Array.isArray(selected)) return;
    const result = await action.run(() => reviewBackend.importStrategyResults(strategy.id, selected));
    if (result) {
      setImported(result);
      await strategies.reload();
      onChange();
    }
  }

  async function remove(strategy: SearchStrategy) {
    const next = await action.run(() => reviewBackend.deleteStrategy(projectId, strategy.id));
    if (next) strategies.setData(next);
  }

  if (!data) return strategies.loading ? <Loading /> : <ErrorNote message={strategies.error} />;
  return (
    <div className="space-y-6">
      <Card title="Cadenas de búsqueda" actions={<button className="button-primary px-3 py-2" onClick={() => setForm({ projectId, databaseName: "", queryText: "", queryDate: today(), resultCount: 0, notes: "", filters: {} })}><Plus className="size-4" /> Nueva cadena</button>}>
        {data.length === 0 ? <Empty title="Sin cadenas registradas" detail="Registra por base de datos la fecha, la sintaxis exacta y la cantidad de resultados; luego importa el archivo exportado." /> : (
          <table className="w-full text-left text-xs">
            <thead className="text-[11px] text-slate-400"><tr><th className="py-2 font-medium">Base de datos</th><th className="font-medium">Fecha</th><th className="font-medium">Cadena</th><th className="font-medium">Cantidad</th><th className="font-medium">Importados</th><th /></tr></thead>
            <tbody>
              {data.map((strategy) => (
                <tr key={strategy.id} className="border-t border-slate-100 align-top">
                  <td className="py-2 pr-2 font-medium text-slate-700">{strategy.databaseName}<span className="block text-[10px] font-normal text-slate-400">versión {strategy.versionNumber}</span></td>
                  <td className="pr-2 text-slate-500">{strategy.queryDate}</td>
                  <td className="max-w-md pr-2"><code className="block whitespace-pre-wrap break-words text-[11px] text-slate-600">{strategy.queryText}</code>{(strategy.periodStart || strategy.language) && <span className="text-[10px] text-slate-400">{strategy.periodStart ? `${strategy.periodStart}–${strategy.periodEnd ?? ""}` : ""} {strategy.language ?? ""}</span>}</td>
                  <td className="pr-2 text-slate-600">{strategy.resultCount}</td>
                  <td className="pr-2 text-slate-600">{strategy.recordsImported}{strategy.recordsImported !== strategy.resultCount && strategy.imports.length > 0 && <span className="block text-[10px] text-amber-700">no coincide con la cantidad reportada</span>}</td>
                  <td className="whitespace-nowrap">
                    <button className="viewer-button size-7" title="Importar resultados (RIS/BibTeX/CSV)" aria-label="Importar resultados" onClick={() => void importFile(strategy)}><FileUp className="size-3.5" /></button>
                    <button className="viewer-button size-7" title="Editar" aria-label="Editar cadena" onClick={() => setForm(edit(strategy))}><Pencil className="size-3.5" /></button>
                    <button className="viewer-button size-7" title="Nueva versión" aria-label="Nueva versión de la cadena" onClick={() => setForm(edit(strategy, true))}><CopyPlus className="size-3.5" /></button>
                    <button className="viewer-button size-7" title="Archivar" aria-label="Archivar cadena" onClick={() => void remove(strategy)}><Trash2 className="size-3.5" /></button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {imported && (
          <p className="mt-3 rounded-lg bg-teal-50 px-3 py-2 text-xs text-teal-800">
            {imported.fileAlreadyImported ? "Este archivo ya estaba vinculado a la cadena. " : ""}{imported.records} registro(s): {imported.newArticles} nuevos, {imported.duplicates} duplicados por DOI (sin fusión automática), {imported.errors} con error · {imported.addedToCorpus} añadidos al corpus.
          </p>
        )}
        <p className="mt-3 text-[11px] text-slate-400">No se consultan bases con licencia de forma automática: exporta los resultados desde cada base e impórtalos aquí. Procedimiento sugerido: exportar, ordenar por año, eliminar duplicados y cribar por metadatos.</p>
      </Card>
      {form && (
        <Card title={form.newVersion ? "Nueva versión de la cadena" : form.id ? "Editar cadena" : "Nueva cadena"}>
          <div className="grid grid-cols-4 gap-3">
            <label className="label">Base de datos<input className="input mt-1" list="databases" value={form.databaseName} onChange={(event) => setForm({ ...form, databaseName: event.target.value })} /><datalist id="databases">{["Scopus", "IEEE Xplore", "Web of Science", "ACM Digital Library", "PubMed", "SpringerLink", "ScienceDirect"].map((name) => <option key={name} value={name} />)}</datalist></label>
            <label className="label">Fecha<input className="input mt-1" type="date" value={form.queryDate} onChange={(event) => setForm({ ...form, queryDate: event.target.value })} /></label>
            <label className="label">Cantidad reportada<input className="input mt-1" type="number" min={0} value={form.resultCount} onChange={(event) => setForm({ ...form, resultCount: Number(event.target.value) })} /></label>
            <label className="label">Idioma<input className="input mt-1" value={form.language ?? ""} onChange={(event) => setForm({ ...form, language: event.target.value || undefined })} /></label>
            <label className="label col-span-4">Cadena (sintaxis original de la base)<textarea className="input mt-1 min-h-20 font-mono" value={form.queryText} onChange={(event) => setForm({ ...form, queryText: event.target.value })} /></label>
            <label className="label">Desde (año)<input className="input mt-1" inputMode="numeric" value={form.periodStart ?? ""} onChange={(event) => setForm({ ...form, periodStart: event.target.value ? Number(event.target.value) : undefined })} /></label>
            <label className="label">Hasta (año)<input className="input mt-1" inputMode="numeric" value={form.periodEnd ?? ""} onChange={(event) => setForm({ ...form, periodEnd: event.target.value ? Number(event.target.value) : undefined })} /></label>
            <label className="label col-span-2">Notas / filtros aplicados<input className="input mt-1" value={form.notes} onChange={(event) => setForm({ ...form, notes: event.target.value })} /></label>
          </div>
          <div className="mt-3"><ErrorNote message={action.error} /></div>
          <div className="mt-3 flex justify-end gap-2"><button className="button-secondary" onClick={() => setForm(null)}>Cancelar</button><button className="button-primary" disabled={action.busy} onClick={() => void save()}>Guardar</button></div>
        </Card>
      )}
      {!form && <ErrorNote message={action.error} />}
    </div>
  );
}
