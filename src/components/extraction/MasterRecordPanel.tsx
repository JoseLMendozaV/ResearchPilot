import { FormEvent, useMemo, useState } from "react";
import { ChevronDown, LoaderCircle, MessageSquareText, Pause, Plus, RefreshCw, ScanSearch, Sparkles, Square, TriangleAlert } from "lucide-react";
import { Link } from "react-router-dom";
import { backend, errorMessage } from "../../lib/backend";
import { formatDateTime, groupLabels, jobStatusLabels, statusLabels } from "../../lib/extraction";
import type { AiProvider, AddMasterValueInput, ArticleExtraction, EvidenceItem, FieldGroup, JobSummary, MasterField, MasterValue } from "../../types";
import { ValueCard } from "./ValueCard";

type Filter = "with_values" | "review" | "all";

interface Props {
  articleId: string;
  extraction: ArticleExtraction;
  onExtract: (force: boolean) => Promise<void>;
  onJobAction: (action: "pause" | "cancel") => Promise<void>;
  /** Queue local-model proposals for open narrative fields. */
  onProposeWithAi?: (provider: AiProvider) => Promise<void>;
  aiJob?: JobSummary;
  onValueChanged: (value: MasterValue) => void;
  onShowEvidence: (evidence: EvidenceItem) => void;
}

export function MasterRecordPanel({ articleId, extraction, onExtract, onJobAction, onProposeWithAi, aiJob, onValueChanged, onShowEvidence }: Props) {
  const [filter, setFilter] = useState<Filter>("with_values");
  const [collapsed, setCollapsed] = useState<Set<FieldGroup>>(new Set());
  const [busy, setBusy] = useState(false);
  const [aiProvider, setAiProvider] = useState<AiProvider>("ollama");
  const [error, setError] = useState<string | null>(null);
  const run = extraction.runs.find((item) => item.provider !== "ollama") ?? extraction.runs[0];
  const aiRun = extraction.runs.find((item) => item.provider === "ollama");
  const job = extraction.activeJob;

  const byField = useMemo(() => {
    const map = new Map<string, MasterValue[]>();
    for (const value of extraction.values) map.set(value.fieldKey, [...(map.get(value.fieldKey) ?? []), value]);
    return map;
  }, [extraction.values]);

  const counts = useMemo(() => {
    const result = { proposed: 0, needs_review: 0, confirmed: 0, conflict: 0, not_found: 0 };
    for (const value of extraction.values) {
      if (value.status === "proposed" || value.status === "needs_review" || value.status === "confirmed" || value.status === "not_found") result[value.status] += 1;
      if (value.conflict) result.conflict += 1;
    }
    return result;
  }, [extraction.values]);

  const visibleFields = (fields: MasterField[]) =>
    fields.filter((field) => {
      const values = byField.get(field.fieldKey) ?? [];
      if (filter === "all") return true;
      if (filter === "review") return values.some((value) => value.status === "proposed" || value.status === "needs_review");
      return values.some((value) => value.status !== "not_found");
    });

  const groups = (Object.keys(groupLabels) as FieldGroup[]).map((group) => ({
    group,
    fields: visibleFields(extraction.fields.filter((field) => field.groupKey === group)),
  }));

  async function act(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-4">
      <section className="rounded-xl bg-slate-50 p-3.5">
        {job ? (
          <div>
            <div className="flex items-center justify-between gap-2">
              <p className="text-xs font-semibold text-slate-700">Extracción {jobStatusLabels[job.status].toLowerCase()}</p>
              <div className="flex gap-1">
                {job.status !== "paused" && <button className="viewer-button size-7" title="Pausar" aria-label="Pausar extracción" disabled={busy} onClick={() => void act(() => onJobAction("pause"))}><Pause className="size-3.5" /></button>}
                <button className="viewer-button size-7" title="Cancelar" aria-label="Cancelar extracción" disabled={busy} onClick={() => void act(() => onJobAction("cancel"))}><Square className="size-3.5" /></button>
              </div>
            </div>
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-white" role="progressbar" aria-valuenow={Math.round(job.progress * 100)} aria-valuemin={0} aria-valuemax={100}>
              <div className="h-full rounded-full bg-teal-500 transition-all" style={{ width: `${Math.round(job.progress * 100)}%` }} />
            </div>
            <p className="mt-1.5 text-[11px] text-slate-400">{job.message ?? "En cola"}</p>
          </div>
        ) : run ? (
          <div>
            <div className="flex items-start justify-between gap-2">
              <div>
                <p className="text-xs font-semibold text-slate-700">{run.status === "completed" ? "Ficha generada" : `Última extracción: ${run.status === "failed" ? "fallida" : "cancelada"}`}</p>
                <p className="mt-0.5 text-[10px] text-slate-400">{formatDateTime(run.completedAt ?? run.startedAt)} · {run.model ?? run.provider} · {run.promptVersion} · extractor {run.extractorVersion ?? "?"}</p>
                {aiRun && <p className="mt-0.5 text-[10px] text-slate-400">IA local: {aiRun.model} · {aiRun.promptVersion} · {formatDateTime(aiRun.completedAt ?? aiRun.startedAt)}</p>}
              </div>
              <button className="button-secondary shrink-0 px-2.5 py-1.5 text-[11px]" disabled={busy} onClick={() => void act(() => onExtract(true))}>
                {busy ? <LoaderCircle className="size-3 animate-spin" /> : <RefreshCw className="size-3" />} Re-extraer
              </button>
            </div>
            {run.errorMessage && <p className="mt-2 text-[11px] text-rose-700">{run.errorMessage}</p>}
            {run.status === "completed" && run.warnings && run.warnings.length > 0 && (
              <ul className="mt-2 space-y-1">
                {run.warnings.map((warning, index) => (
                  <li key={`${warning.code}-${index}`} className="flex items-start gap-1.5 text-[11px] leading-4 text-amber-800"><TriangleAlert className="mt-px size-3 shrink-0" />{warning.message}{warning.page ? ` (p. ${warning.page})` : ""}</li>
                ))}
              </ul>
            )}
          </div>
        ) : (
          <div className="text-center">
            <p className="text-xs font-semibold text-slate-700">Este artículo aún no tiene ficha científica</p>
            <p className="mt-1 text-[11px] leading-4 text-slate-400">El extractor local analiza el PDF y propone valores con su cita literal y página. Nada sale del equipo.</p>
            <button className="button-primary mx-auto mt-3 px-3 py-2" disabled={busy} onClick={() => void act(() => onExtract(false))}>
              {busy ? <LoaderCircle className="size-3.5 animate-spin" /> : <ScanSearch className="size-3.5" />} Extraer ahora
            </button>
          </div>
        )}
        {run?.status === "completed" && !job && (
          <div className="mt-3 flex flex-wrap gap-2 border-t border-slate-200/70 pt-3">
            {onProposeWithAi && (
              <span className="flex items-center gap-1">
                <button className="button-secondary px-2.5 py-1.5 text-[11px]" disabled={busy || Boolean(aiJob)} onClick={() => void act(() => onProposeWithAi(aiProvider))} title="Un modelo de IA propone los campos narrativos vacíos; solo se guardan propuestas con cita verificada">
                  <Sparkles className="size-3" /> Proponer campos con IA
                </button>
                <select className="input w-28 px-2 py-1.5 text-[11px]" value={aiProvider} onChange={(event) => setAiProvider(event.target.value as AiProvider)} aria-label="Proveedor para las propuestas">
                  <option value="ollama">Local</option>
                  <option value="openai">OpenAI</option>
                  <option value="anthropic">Anthropic</option>
                </select>
              </span>
            )}
            <Link className="button-secondary px-2.5 py-1.5 text-[11px]" to={`/asistente?scope=article&id=${articleId}`}><MessageSquareText className="size-3" /> Preguntar a este artículo</Link>
          </div>
        )}
        {aiJob && (
          <p className="mt-2 flex items-center gap-1.5 text-[11px] text-slate-500"><LoaderCircle className="size-3 animate-spin text-teal-600" /> IA local: {aiJob.message ?? "En cola"} ({Math.round(aiJob.progress * 100)}%)</p>
        )}
        {error && <p className="mt-2 text-[11px] text-rose-700">{error}</p>}
      </section>

      {extraction.values.length > 0 && (
        <div className="flex flex-wrap gap-1.5 text-[10px]">
          <Count label="Propuestos" value={counts.proposed} />
          <Count label="Por revisar" value={counts.needs_review} tone="text-amber-700" />
          <Count label="Confirmados" value={counts.confirmed} tone="text-emerald-700" />
          {counts.conflict > 0 && <Count label="En conflicto" value={counts.conflict} tone="text-amber-800" />}
          <Count label="No encontrados" value={counts.not_found} />
        </div>
      )}

      <div className="flex rounded-lg bg-slate-100 p-0.5 text-[11px] font-semibold" role="tablist" aria-label="Filtro de campos">
        {([["with_values", "Con datos"], ["review", "Por revisar"], ["all", "Todos los campos"]] as const).map(([key, label]) => (
          <button key={key} role="tab" aria-selected={filter === key} className={`flex-1 rounded-md px-2 py-1.5 transition ${filter === key ? "bg-white text-slate-800 shadow-sm" : "text-slate-500"}`} onClick={() => setFilter(key)}>{label}</button>
        ))}
      </div>

      {groups.every(({ fields }) => fields.length === 0) && (
        <p className="rounded-xl border border-dashed border-slate-200 px-4 py-6 text-center text-xs text-slate-400">
          {filter === "review" ? "No hay valores pendientes de revisión." : "Sin valores todavía. Usa «Todos los campos» para registrar datos manualmente."}
        </p>
      )}

      {groups.filter(({ fields }) => fields.length > 0).map(({ group, fields }) => (
        <section key={group}>
          <button
            className="flex w-full items-center justify-between border-b border-slate-100 pb-2 text-left"
            aria-expanded={!collapsed.has(group)}
            onClick={() => setCollapsed((current) => { const next = new Set(current); if (next.has(group)) next.delete(group); else next.add(group); return next; })}
          >
            <h3 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-slate-400">{groupLabels[group]} <span className="font-normal normal-case tracking-normal">· {fields.length}</span></h3>
            <ChevronDown className={`size-4 text-slate-300 transition ${collapsed.has(group) ? "-rotate-90" : ""}`} />
          </button>
          {!collapsed.has(group) && (
            <div className="mt-3 space-y-4">
              {fields.map((field) => (
                <FieldBlock key={field.fieldKey} articleId={articleId} field={field} values={byField.get(field.fieldKey) ?? []} onValueChanged={onValueChanged} onShowEvidence={onShowEvidence} />
              ))}
            </div>
          )}
        </section>
      ))}
    </div>
  );
}

function FieldBlock({ articleId, field, values, onValueChanged, onShowEvidence }: { articleId: string; field: MasterField; values: MasterValue[]; onValueChanged: (value: MasterValue) => void; onShowEvidence: (evidence: EvidenceItem) => void }) {
  const [adding, setAdding] = useState(false);
  return (
    <div>
      <div className="flex items-center justify-between">
        <h4 className="text-xs font-semibold text-slate-700">{field.labelEs}</h4>
        <button className="flex items-center gap-1 text-[10px] font-semibold text-slate-400 hover:text-teal-700" onClick={() => setAdding(!adding)} aria-label={`Agregar valor a ${field.labelEs}`}><Plus className="size-3" /> Agregar</button>
      </div>
      <div className="mt-1.5 space-y-2">
        {values.length === 0 && !adding && <p className="text-[11px] text-slate-300">Sin extraer</p>}
        {values.map((value) => <ValueCard key={value.id} field={field} value={value} onChanged={onValueChanged} onShowEvidence={onShowEvidence} />)}
        {adding && <AddValueForm articleId={articleId} field={field} onDone={(value) => { if (value) onValueChanged(value); setAdding(false); }} />}
      </div>
    </div>
  );
}

function AddValueForm({ articleId, field, onDone }: { articleId: string; field: MasterField; onDone: (value?: MasterValue) => void }) {
  const [form, setForm] = useState({ valueText: "", status: "confirmed" as AddMasterValueInput["status"], quote: "", page: "", notes: "" });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
      onDone(await backend.addMasterValue({
        articleId,
        fieldKey: field.fieldKey,
        valueText: form.valueText || undefined,
        status: form.status,
        quote: form.quote || undefined,
        page: form.page ? Number(form.page) : undefined,
        notes: form.notes,
      }));
    } catch (cause) {
      setError(errorMessage(cause));
      setSaving(false);
    }
  }

  return (
    <form className="space-y-2 rounded-xl border border-teal-100 bg-teal-50/30 p-3" onSubmit={submit}>
      <label className="label">Valor<textarea className="input mt-1 min-h-14" value={form.valueText} onChange={(event) => setForm({ ...form, valueText: event.target.value })} placeholder={field.dataType === "list" ? "Elementos separados por punto y coma" : undefined} /></label>
      <label className="label">Estado
        <select className="input mt-1" value={form.status} onChange={(event) => setForm({ ...form, status: event.target.value as AddMasterValueInput["status"] })}>
          {(["confirmed", "needs_review", "not_reported", "not_applicable"] as const).map((status) => <option key={status} value={status}>{statusLabels[status]}</option>)}
        </select>
      </label>
      <div className="grid grid-cols-[1fr_72px] gap-2">
        <label className="label">Cita literal<input className="input mt-1" value={form.quote} onChange={(event) => setForm({ ...form, quote: event.target.value })} placeholder="Opcional" /></label>
        <label className="label">Página<input className="input mt-1" inputMode="numeric" value={form.page} onChange={(event) => setForm({ ...form, page: event.target.value.replace(/\D/g, "") })} /></label>
      </div>
      <label className="label">Notas<input className="input mt-1" value={form.notes} onChange={(event) => setForm({ ...form, notes: event.target.value })} /></label>
      {error && <p className="text-[11px] text-rose-700">{error}</p>}
      <div className="flex justify-end gap-1.5">
        <button type="button" className="button-secondary px-3 py-1.5 text-[11px]" onClick={() => onDone()}>Cancelar</button>
        <button className="button-primary px-3 py-1.5 text-[11px]" disabled={saving}>{saving && <LoaderCircle className="size-3 animate-spin" />} Registrar</button>
      </div>
    </form>
  );
}

function Count({ label, value, tone = "text-slate-600" }: { label: string; value: number; tone?: string }) {
  return <span className="rounded-full border border-slate-100 bg-white px-2 py-1 text-slate-400"><span className={`font-semibold ${tone}`}>{value}</span> {label}</span>;
}
