import { useMemo, useState } from "react";
import { AlertTriangle, ChevronDown, FileText, Plus, Trash2 } from "lucide-react";
import { Link } from "react-router-dom";
import { reviewBackend } from "../../lib/reviewBackend";
import type { Criterion, CriterionInput, CriterionMark, Decision, Phase, ScreeningRow } from "../../reviewTypes";
import { Card, Empty, ErrorNote, Loading, decisionLabels, decisionTones, stageLabels, useAction, useLoad } from "./shared";

type Filter = "pending" | "all" | "conflicts" | "decided";
const markLabels: Record<CriterionMark, string> = { met: "✓", not_met: "✗", unclear: "?" };

export function ScreeningTab({ projectId, onChange }: { projectId: string; onChange: () => void }) {
  const [phase, setPhase] = useState<Phase>("metadata");
  const [filter, setFilter] = useState<Filter>("pending");
  const table = useLoad(() => reviewBackend.screening(projectId, phase), [projectId, phase]);
  const [criterion, setCriterion] = useState<CriterionInput>({ projectId, criterionType: "inclusion", code: "", name: "", description: "" });
  const action = useAction();
  const data = table.data;

  const rows = useMemo(() => (data?.rows ?? []).filter((row) =>
    filter === "all" ? true : filter === "pending" ? !row.finalDecision && !row.conflictId : filter === "conflicts" ? Boolean(row.conflictId) : Boolean(row.finalDecision),
  ), [data, filter]);

  async function saveCriterion() {
    const next = await action.run(() => reviewBackend.saveCriterion(criterion));
    if (next && data) {
      table.setData({ ...data, criteria: next });
      setCriterion({ ...criterion, code: "", name: "", description: "" });
    }
  }

  async function removeCriterion(item: Criterion) {
    const next = await action.run(() => reviewBackend.deleteCriterion(projectId, item.id));
    if (next && data) table.setData({ ...data, criteria: next });
  }

  function replaceRow(row: ScreeningRow) {
    if (!data) return;
    table.setData({ ...data, rows: data.rows.map((item) => (item.articleId === row.articleId ? row : item)) });
    onChange();
  }

  if (!data) return table.loading ? <Loading /> : <ErrorNote message={table.error} />;
  const active = data.criteria.filter((item) => item.active);
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex rounded-lg bg-slate-100 p-0.5 text-[11px] font-semibold" role="tablist">
          {([["metadata", "Metadatos (título, resumen, palabras clave)"], ["full_text", "Texto completo"]] as const).map(([key, label]) => (
            <button key={key} role="tab" aria-selected={phase === key} className={`rounded-md px-3 py-1.5 ${phase === key ? "bg-white text-slate-800 shadow-sm" : "text-slate-500"}`} onClick={() => setPhase(key)}>{label}</button>
          ))}
        </div>
        <div className="flex items-center gap-3 text-[11px] text-slate-500">
          <span>{data.decided} con decisión final · {data.pending} pendientes</span>
          {data.agreement && <span title={data.agreement.note}>κ {data.agreement.reviewerA}/{data.agreement.reviewerB}: {data.agreement.kappa?.toFixed(2) ?? "n/d"} (acuerdo {(data.agreement.observed * 100).toFixed(0)} %, n={data.agreement.compared})</span>}
          <select className="input w-40" value={filter} onChange={(event) => setFilter(event.target.value as Filter)} aria-label="Filtro">
            <option value="pending">Pendientes</option><option value="conflicts">Con conflicto</option><option value="decided">Decididos</option><option value="all">Todos</option>
          </select>
        </div>
      </div>

      <Card title="Criterios de inclusión y exclusión">
        <div className="flex flex-wrap gap-1.5">
          {data.criteria.map((item) => (
            <span key={item.id} className={`flex items-center gap-1 rounded-full px-2.5 py-1 text-[11px] ${item.criterionType === "inclusion" ? "bg-emerald-50 text-emerald-800" : "bg-rose-50 text-rose-800"} ${item.active ? "" : "line-through opacity-50"}`} title={item.description}>
              <strong>{item.code}</strong> {item.name}
              {item.active && <button aria-label={`Quitar ${item.code}`} onClick={() => void removeCriterion(item)}><Trash2 className="size-3" /></button>}
            </span>
          ))}
          {data.criteria.length === 0 && <span className="text-xs text-slate-400">Define los criterios del protocolo (ej.: idioma inglés, estudio primario, tema, área).</span>}
        </div>
        <div className="mt-3 grid grid-cols-[120px_80px_1fr_auto] gap-2">
          <select className="input" value={criterion.criterionType} onChange={(event) => setCriterion({ ...criterion, criterionType: event.target.value as CriterionInput["criterionType"] })}><option value="inclusion">Inclusión</option><option value="exclusion">Exclusión</option></select>
          <input className="input" placeholder="Código" value={criterion.code} onChange={(event) => setCriterion({ ...criterion, code: event.target.value })} />
          <input className="input" placeholder="Criterio (ej.: Escrito en inglés)" value={criterion.name} onChange={(event) => setCriterion({ ...criterion, name: event.target.value })} />
          <button className="button-secondary px-3" disabled={!criterion.code || !criterion.name || action.busy} onClick={() => void saveCriterion()}><Plus className="size-3.5" /></button>
        </div>
        <div className="mt-2"><ErrorNote message={action.error} /></div>
      </Card>

      {rows.length === 0 ? <Empty title={data.rows.length ? "Nada en este filtro" : phase === "full_text" ? "No hay estudios en texto completo" : "El corpus está vacío"} detail={phase === "full_text" ? "Aquí aparecen los estudios aprobados en el cribado por metadatos." : "Importa resultados desde Cadena o añade artículos desde Resumen."} /> : (
        <div className="space-y-3">{rows.map((row) => <ScreeningItem key={row.articleId} projectId={projectId} phase={phase} row={row} criteria={active} onSaved={replaceRow} />)}</div>
      )}
    </div>
  );
}

function ScreeningItem({ projectId, phase, row, criteria, onSaved }: { projectId: string; phase: Phase; row: ScreeningRow; criteria: Criterion[]; onSaved: (row: ScreeningRow) => void }) {
  const [open, setOpen] = useState(false);
  const [marks, setMarks] = useState<Record<string, CriterionMark>>({});
  const [criterionId, setCriterionId] = useState("");
  const [reason, setReason] = useState("");
  const action = useAction();
  const exclusions = criteria.filter((item) => item.criterionType === "exclusion");

  async function decide(decision: Decision) {
    const next = await action.run(() => reviewBackend.decide({ projectId, articleId: row.articleId, phase, decision, criterionId: criterionId || undefined, criteria: marks, reason }));
    if (next) {
      onSaved(next);
      setReason("");
    }
  }

  async function toggleSecondary() {
    await action.run(() => reviewBackend.setCorpusFlags(projectId, row.articleId, !row.isSecondary));
    onSaved({ ...row, isSecondary: !row.isSecondary });
  }

  return (
    <article className={`panel p-4 ${row.conflictId ? "border-amber-200" : ""}`}>
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <p className="text-[11px] text-slate-400">Id {row.sequence} · {stageLabels[row.stage]}{row.sourceDatabase ? ` · ${row.sourceDatabase}` : ""}{row.isSecondary ? " · estudio secundario" : ""}</p>
          <h4 className="mt-0.5 text-sm font-semibold text-slate-800">{row.title}</h4>
          <p className="text-xs text-slate-500">{row.authors.join("; ") || "Sin autores"} · {row.year ?? "s. f."}{row.venue ? ` · ${row.venue}` : ""}</p>
          {row.keywords.length > 0 && <p className="mt-1 text-[11px] text-slate-400">{row.keywords.join(" · ")}</p>}
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1">
          {row.finalDecision && <span className={`rounded-full px-2 py-0.5 text-[10px] font-semibold ${decisionTones[row.finalDecision.decision]}`}>Final: {decisionLabels[row.finalDecision.decision]}</span>}
          {row.conflictId && <span className="flex items-center gap-1 rounded-full bg-amber-100 px-2 py-0.5 text-[10px] font-semibold text-amber-800"><AlertTriangle className="size-3" /> Conflicto entre revisores</span>}
          {row.hasPdf && <Link className="flex items-center gap-1 text-[11px] font-semibold text-teal-700" to={`/biblioteca/${row.articleId}`}><FileText className="size-3" /> PDF</Link>}
        </div>
      </div>
      {row.abstractText && (
        <button className="mt-2 flex items-center gap-1 text-[11px] font-semibold text-slate-500" onClick={() => setOpen(!open)}><ChevronDown className={`size-3 transition ${open ? "" : "-rotate-90"}`} /> Resumen</button>
      )}
      {open && <p className="mt-1 text-xs leading-5 text-slate-600">{row.abstractText}</p>}
      {row.reviews.length > 0 && (
        <ul className="mt-2 flex flex-wrap gap-1.5">
          {row.reviews.map((review) => <li key={review.id} className={`rounded-full px-2 py-0.5 text-[10px] ${decisionTones[review.decision]}`} title={review.reason}>{review.reviewerName}: {decisionLabels[review.decision]}</li>)}
        </ul>
      )}
      {criteria.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-2">
          {criteria.map((item) => (
            <label key={item.id} className="flex items-center gap-1 text-[11px] text-slate-600" title={item.name}>
              <span className="font-semibold">{item.code}</span>
              <select className="rounded border border-slate-200 bg-white px-1 py-0.5 text-[11px]" value={marks[item.id] ?? ""} onChange={(event) => setMarks((current) => { const next = { ...current }; if (event.target.value) next[item.id] = event.target.value as CriterionMark; else delete next[item.id]; return next; })} aria-label={`Criterio ${item.code}`}>
                <option value="">·</option>{Object.entries(markLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
              </select>
            </label>
          ))}
        </div>
      )}
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <select className="input w-56" value={criterionId} onChange={(event) => setCriterionId(event.target.value)} aria-label="Criterio de exclusión aplicado">
          <option value="">Criterio de exclusión aplicado…</option>
          {exclusions.map((item) => <option key={item.id} value={item.id}>{item.code} · {item.name}</option>)}
        </select>
        <input className="input min-w-48 flex-1" placeholder="Justificación (queda en el diario de decisiones)" value={reason} onChange={(event) => setReason(event.target.value)} />
        {(["include", "exclude", "maybe", ...(phase === "metadata" ? ["awaiting_full_text"] : [])] as Decision[]).map((decision) => (
          <button key={decision} className={`rounded-lg px-3 py-2 text-[11px] font-semibold ${decisionTones[decision]} disabled:opacity-50`} disabled={action.busy} onClick={() => void decide(decision)}>{decisionLabels[decision]}</button>
        ))}
        <button className="button-secondary px-2.5 py-1.5 text-[11px]" onClick={() => void toggleSecondary()}>{row.isSecondary ? "Quitar de secundarios" : "Marcar secundario"}</button>
      </div>
      <div className="mt-2"><ErrorNote message={action.error} /></div>
    </article>
  );
}
