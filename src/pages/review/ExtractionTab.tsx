import { useState } from "react";
import { FileSearch, Plus, Wand2, X } from "lucide-react";
import { Link } from "react-router-dom";
import { evidenceLink, statusLabels, statusTones } from "../../lib/extraction";
import { reviewBackend } from "../../lib/reviewBackend";
import type { CellInput, CellValue, ExtractionField, ExtractionMatrix } from "../../reviewTypes";
import { Empty, ErrorNote, Loading, useAction, useLoad } from "./shared";

type Editing = { row: ExtractionMatrix["rows"][number]; field: ExtractionField; value?: CellValue };

export function ExtractionTab({ projectId }: { projectId: string }) {
  const [includeEligible, setIncludeEligible] = useState(false);
  const matrix = useLoad(() => reviewBackend.matrix(projectId, includeEligible), [projectId, includeEligible]);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const action = useAction();
  const data = matrix.data;

  async function prefill() {
    const copied = await action.run(() => reviewBackend.prefillMatrix(projectId));
    if (copied !== undefined) {
      setNotice(`${copied} valor(es) copiados de la ficha maestra como propuestas`);
      await matrix.reload();
    }
  }

  if (!data) return matrix.loading ? <Loading /> : <ErrorNote message={matrix.error} />;
  const fields = data.groups.flatMap((group) => group.fields);
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <label className="flex items-center gap-2 text-xs text-slate-600"><input type="checkbox" className="accent-teal-600" checked={includeEligible} onChange={(event) => setIncludeEligible(event.target.checked)} /> Mostrar también estudios elegibles (texto completo pendiente)</label>
        <div className="flex items-center gap-2">
          {notice && <span className="text-xs text-teal-700">{notice}</span>}
          <button className="button-secondary px-3 py-2" disabled={action.busy} onClick={() => void prefill()} title="Copia valores confirmados de la ficha maestra en celdas vacías de aspectos vinculados"><Wand2 className="size-3.5" /> Prellenar desde la ficha</button>
        </div>
      </div>
      <ErrorNote message={action.error} />
      {fields.length === 0 ? <Empty title="Sin aspectos de extracción" detail="Define aspectos generales y por RQ en la pestaña RQs." /> : data.rows.length === 0 ? <Empty title="No hay estudios incluidos" detail="La matriz se completa con los estudios incluidos tras el texto completo." /> : (
        <div className="panel overflow-auto">
          <table className="min-w-full border-separate border-spacing-0 text-left text-[11px]">
            <thead>
              <tr className="bg-slate-50">
                <th colSpan={4} className="sticky left-0 top-0 z-20 border-b border-slate-200 bg-slate-50 px-3 py-2 text-slate-500">Estudio</th>
                {data.groups.filter((group) => group.fields.length).map((group) => <th key={group.key} colSpan={group.fields.length} className="border-b border-l border-slate-200 px-3 py-2 font-semibold text-slate-700" title={group.question}>{group.label}</th>)}
              </tr>
              <tr className="bg-white">
                {["Id", "Reference", "Title", "Year"].map((label) => <th key={label} className="border-b border-slate-200 px-3 py-2 font-medium text-slate-400">{label}</th>)}
                {fields.map((field) => <th key={field.id} className="min-w-44 border-b border-l border-slate-200 px-3 py-2 font-medium text-slate-600" title={field.instructions}>{field.name}{field.unit ? ` (${field.unit})` : ""}</th>)}
              </tr>
            </thead>
            <tbody>
              {data.rows.map((row) => (
                <tr key={row.articleId} className="align-top">
                  <td className="border-b border-slate-100 px-3 py-2 text-slate-400">{row.sequence}</td>
                  <td className="whitespace-nowrap border-b border-slate-100 px-3 py-2 text-slate-600">{row.reference}</td>
                  <td className="min-w-64 border-b border-slate-100 px-3 py-2"><Link className="text-slate-700 hover:text-teal-700" to={`/biblioteca/${row.articleId}`}>{row.title}</Link>{row.stage !== "included" && <span className="block text-[10px] text-amber-700">elegible</span>}</td>
                  <td className="border-b border-slate-100 px-3 py-2 text-slate-500">{row.year ?? ""}</td>
                  {fields.map((field) => (
                    <td key={field.id} className="border-b border-l border-slate-100 px-2 py-1.5">
                      <div className="space-y-1">
                        {(row.cells[field.id] ?? []).map((value) => (
                          <button key={value.id} className={`block w-full rounded px-1.5 py-1 text-left ${statusTones[value.status]}`} onClick={() => setEditing({ row, field, value })} title={value.notes}>
                            {value.display || statusLabels[value.status]}{value.evidence.length > 0 && <FileSearch className="ml-1 inline size-3" />}
                          </button>
                        ))}
                        <button className="text-[10px] text-slate-300 hover:text-teal-700" aria-label={`Agregar valor a ${field.name}`} onClick={() => setEditing({ row, field })}><Plus className="inline size-3" /></button>
                      </div>
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {editing && <CellEditor projectId={projectId} editing={editing} onClose={() => setEditing(null)} onSaved={async () => { setEditing(null); await matrix.reload(); }} />}
    </div>
  );
}

function CellEditor({ projectId, editing, onClose, onSaved }: { projectId: string; editing: Editing; onClose: () => void; onSaved: () => Promise<void> }) {
  const { row, field, value } = editing;
  const [form, setForm] = useState<CellInput>({
    projectId, articleId: row.articleId, fieldId: field.id, valueId: value?.id,
    valueText: value ? (Array.isArray(value.value) ? (value.value as string[]).join("; ") : value.display.replace(field.unit ? ` ${field.unit}` : "", "")) : "",
    status: value?.status ?? "confirmed", notes: value?.notes ?? "",
  });
  const action = useAction();

  async function save() {
    if (await action.run(() => reviewBackend.saveCell(form))) await onSaved();
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-slate-900/30 p-6" role="dialog" aria-modal="true" aria-label={`Valor de ${field.name}`}>
      <div className="panel w-full max-w-lg p-5 shadow-xl">
        <div className="flex items-start justify-between gap-3">
          <div><p className="text-[11px] text-slate-400">Id {row.sequence} · {row.reference}</p><h3 className="text-sm font-semibold text-slate-800">{field.name}</h3>{field.instructions && <p className="mt-0.5 text-[11px] text-slate-500">{field.instructions}</p>}</div>
          <button className="viewer-button size-7" aria-label="Cerrar" onClick={onClose}><X className="size-4" /></button>
        </div>
        <div className="mt-4 space-y-3">
          {field.dataType === "category" ? (
            <select className="input" value={form.valueText} onChange={(event) => setForm({ ...form, valueText: event.target.value })} aria-label="Valor"><option value="">Sin valor</option>{field.taxonomy.map((item) => <option key={item} value={item}>{item}</option>)}</select>
          ) : field.dataType === "boolean" ? (
            <select className="input" value={form.valueText} onChange={(event) => setForm({ ...form, valueText: event.target.value })} aria-label="Valor"><option value="">Sin valor</option><option value="sí">Sí</option><option value="no">No</option></select>
          ) : (
            <input className="input" value={form.valueText} onChange={(event) => setForm({ ...form, valueText: event.target.value })} placeholder={field.dataType === "list" ? "Elementos separados por punto y coma" : field.dataType === "date" ? "AAAA-MM-DD o año" : ""} aria-label="Valor" />
          )}
          <label className="label">Estado
            <select className="input mt-1" value={form.status} onChange={(event) => setForm({ ...form, status: event.target.value as CellInput["status"] })}>
              {(["confirmed", "proposed", "needs_review", "not_reported", "not_found", "not_applicable", "rejected"] as const).map((status) => <option key={status} value={status}>{statusLabels[status]}</option>)}
            </select>
          </label>
          <div className="grid grid-cols-[1fr_80px] gap-2">
            <label className="label">Cita literal<input className="input mt-1" value={form.quote ?? ""} onChange={(event) => setForm({ ...form, quote: event.target.value || undefined })} /></label>
            <label className="label">Página<input className="input mt-1" inputMode="numeric" value={form.page ?? ""} onChange={(event) => setForm({ ...form, page: event.target.value ? Number(event.target.value) : undefined })} /></label>
          </div>
          <label className="label">Notas<input className="input mt-1" value={form.notes} onChange={(event) => setForm({ ...form, notes: event.target.value })} /></label>
          {value?.evidence.map((evidence) => (
            <p key={evidence.id} className="text-[11px] italic text-slate-500">“{evidence.quote}” {evidence.page && <Link className="not-italic font-semibold text-teal-700" to={evidenceLink(row.articleId, evidence.page, evidence.bbox, evidence.pageWidth)}>p. {evidence.page}</Link>}</p>
          ))}
          {value && <p className="text-[10px] text-slate-400">Registrado por {value.actorName}</p>}
          <ErrorNote message={action.error} />
          <div className="flex justify-end gap-2"><button className="button-secondary" onClick={onClose}>Cancelar</button><button className="button-primary" disabled={action.busy} onClick={() => void save()}>Guardar</button></div>
        </div>
      </div>
    </div>
  );
}
