import { useState } from "react";
import { AlertTriangle, Plus, Trash2 } from "lucide-react";
import { reviewBackend } from "../../lib/reviewBackend";
import type { QualityCriterion, QualityCriterionInput, QualityRow, QualityTable, ScaleOption, ScoreEntry } from "../../reviewTypes";
import { Card, Empty, ErrorNote, Loading, useAction, useLoad } from "./shared";

const defaultScale: ScaleOption[] = [{ label: "Sí", value: 1 }, { label: "Parcialmente", value: 0.5 }, { label: "No", value: 0 }];

export function QualityTab({ projectId, onChange }: { projectId: string; onChange: () => void }) {
  const table = useLoad(() => reviewBackend.quality(projectId), [projectId]);
  const [form, setForm] = useState<QualityCriterionInput>({ projectId, question: "", description: "", weight: 1, scale: defaultScale });
  const action = useAction();
  const data = table.data;

  async function saveCriterion() {
    const next = await action.run(() => reviewBackend.saveQualityCriterion(form));
    if (next) {
      await table.reload();
      setForm({ projectId, question: "", description: "", weight: 1, scale: defaultScale });
    }
  }

  async function removeCriterion(item: QualityCriterion) {
    if (await action.run(() => reviewBackend.deleteQualityCriterion(projectId, item.id))) await table.reload();
  }

  if (!data) return table.loading ? <Loading /> : <ErrorNote message={table.error} />;
  return (
    <div className="space-y-6">
      <Card title={`Preguntas de calidad · total máximo ${data.maxTotal.toFixed(2)}${data.threshold !== undefined && data.threshold !== null ? ` · umbral ${data.threshold} %` : ""}`}>
        <table className="w-full text-left text-xs">
          <thead className="text-[11px] text-slate-400"><tr><th className="py-1 font-medium">Código</th><th className="font-medium">Pregunta</th><th className="font-medium">Peso</th><th className="font-medium">Escala</th><th /></tr></thead>
          <tbody>
            {data.criteria.map((item) => (
              <tr key={item.id} className="border-t border-slate-100">
                <td className="py-1.5 font-semibold text-slate-700">{item.code}</td><td className="text-slate-600">{item.question}</td><td>{item.weight}</td>
                <td className="text-slate-500">{item.scale.map((option) => `${option.label} (${option.value})`).join(" · ")}</td>
                <td><button className="viewer-button size-7" aria-label={`Eliminar ${item.code}`} onClick={() => void removeCriterion(item)}><Trash2 className="size-3.5" /></button></td>
              </tr>
            ))}
          </tbody>
        </table>
        <div className="mt-3 grid grid-cols-[1fr_80px_1fr_auto] gap-2">
          <input className="input" placeholder="Pregunta (Qn), ej.: ¿Se describe claramente el dataset?" value={form.question} onChange={(event) => setForm({ ...form, question: event.target.value })} />
          <input className="input" type="number" min={0} step={0.5} value={form.weight} onChange={(event) => setForm({ ...form, weight: Number(event.target.value) })} aria-label="Peso" />
          <input className="input" value={form.scale.map((option) => `${option.label}=${option.value}`).join("; ")} onChange={(event) => setForm({ ...form, scale: event.target.value.split(";").map((part) => part.split("=")).filter((pair) => pair.length === 2).map(([label, value]) => ({ label: label.trim(), value: Number(value) })) })} aria-label="Escala (etiqueta=valor; …)" />
          <button className="button-secondary px-3" disabled={form.question.trim().length < 5 || action.busy} onClick={() => void saveCriterion()}><Plus className="size-3.5" /></button>
        </div>
        <p className="mt-2 text-[11px] text-slate-400">Escala como «Sí=1; Parcialmente=0.5; No=0». El total es la suma de peso × valor; con revisión doble, solo el consenso cuenta como resultado.</p>
        <div className="mt-2"><ErrorNote message={action.error} /></div>
      </Card>
      {data.criteria.length === 0 ? null : data.rows.length === 0 ? <Empty title="No hay estudios en texto completo" detail="La calidad se evalúa sobre estudios elegibles o incluidos." /> : (
        <div className="space-y-3">{data.rows.map((row) => <QualityItem key={row.articleId} projectId={projectId} table={data} row={row} onSaved={(next) => { table.setData(next); onChange(); }} />)}</div>
      )}
    </div>
  );
}

function QualityItem({ projectId, table, row, onSaved }: { projectId: string; table: QualityTable; row: QualityRow; onSaved: (table: QualityTable) => void }) {
  const [scores, setScores] = useState<Record<string, ScoreEntry>>({});
  const [reason, setReason] = useState("");
  const action = useAction();
  const entries = (): ScoreEntry[] => table.criteria.map((criterion) => scores[criterion.id] ?? { criterionId: criterion.id, score: null, justification: "" });

  async function save() {
    const next = await action.run(() => row.conflictId ? reviewBackend.resolveQuality(row.conflictId, entries(), reason) : reviewBackend.saveScores(projectId, row.articleId, entries()));
    if (next) onSaved(next);
  }

  const final = row.finalAssessment;
  return (
    <article className="panel p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-[11px] text-slate-400">Id {row.sequence}</p>
          <h4 className="text-sm font-semibold text-slate-800">{row.title}</h4>
          <p className="text-[11px] text-slate-500">{row.reviews.map((review) => `${review.reviewerName}: ${review.total.toFixed(2)}/${review.maxTotal.toFixed(2)}${review.complete ? "" : " (incompleto)"}`).join(" · ") || "Sin evaluaciones"}</p>
        </div>
        <div className="text-right text-xs">
          {final ? <p className="font-semibold text-slate-800">Total {final.total.toFixed(2)} / {final.maxTotal.toFixed(2)}{final.percent !== undefined && final.percent !== null ? ` · ${final.percent.toFixed(0)} %` : ""}</p> : <p className="text-slate-400">Sin resultado final</p>}
          {row.meetsThreshold !== undefined && row.meetsThreshold !== null && <p className={row.meetsThreshold ? "text-emerald-700" : "text-rose-700"}>{row.meetsThreshold ? "Cumple el umbral" : "No cumple el umbral"}</p>}
          {row.conflictId && <p className="mt-1 flex items-center justify-end gap-1 font-semibold text-amber-800"><AlertTriangle className="size-3" /> Conflicto: registra el consenso</p>}
        </div>
      </div>
      <div className="mt-3 grid grid-cols-2 gap-2">
        {table.criteria.map((criterion) => (
          <div key={criterion.id} className="rounded-lg bg-slate-50 p-2">
            <p className="text-[11px] text-slate-600"><strong>{criterion.code}</strong> {criterion.question}</p>
            <div className="mt-1 flex gap-1.5">
              <select className="input w-36" value={scores[criterion.id]?.score ?? ""} onChange={(event) => setScores({ ...scores, [criterion.id]: { criterionId: criterion.id, score: event.target.value === "" ? null : Number(event.target.value), justification: scores[criterion.id]?.justification ?? "" } })} aria-label={`Puntaje ${criterion.code}`}>
                <option value="">Sin responder</option>{criterion.scale.map((option) => <option key={option.label} value={option.value}>{option.label} ({option.value})</option>)}
              </select>
              <input className="input" placeholder="Justificación" value={scores[criterion.id]?.justification ?? ""} onChange={(event) => setScores({ ...scores, [criterion.id]: { criterionId: criterion.id, score: scores[criterion.id]?.score ?? null, justification: event.target.value } })} />
            </div>
          </div>
        ))}
      </div>
      <div className="mt-3 flex items-center justify-end gap-2">
        {row.conflictId && <input className="input max-w-sm" placeholder="Motivo del consenso" value={reason} onChange={(event) => setReason(event.target.value)} />}
        <button className="button-primary px-3 py-2" disabled={action.busy || (Boolean(row.conflictId) && reason.trim().length < 3)} onClick={() => void save()}>{row.conflictId ? "Registrar consenso" : "Guardar mi evaluación"}</button>
      </div>
      <div className="mt-2"><ErrorNote message={action.error} /></div>
    </article>
  );
}
