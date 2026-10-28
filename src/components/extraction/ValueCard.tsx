import { FormEvent, useEffect, useState } from "react";
import { AlertTriangle, Check, FileSearch, History, LoaderCircle, Pencil, RotateCcw, X } from "lucide-react";
import { backend, errorMessage } from "../../lib/backend";
import { editableText, formatDateTime, originLabels, reviewStatuses, statusLabels, statusTones } from "../../lib/extraction";
import type { EvidenceItem, MasterField, MasterValue, UpdateMasterValueInput, ValueRevision } from "../../types";

interface Props {
  field: MasterField;
  value: MasterValue;
  onChanged: (value: MasterValue) => void;
  onShowEvidence: (evidence: EvidenceItem) => void;
}

export function ValueCard({ field, value, onChanged, onShowEvidence }: Props) {
  const [mode, setMode] = useState<"view" | "edit" | "history">("view");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);

  async function review(input: Omit<UpdateMasterValueInput, "valueId">) {
    setSaving(true);
    setError(null);
    try {
      onChanged(await backend.updateMasterValue({ valueId: value.id, ...input }));
      setMode("view");
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSaving(false);
    }
  }

  const empty = value.status === "not_found" || value.value === null;
  const long = value.displayValue.length > 220;

  return (
    <div className={`rounded-xl border p-3 ${value.conflict ? "border-amber-200 bg-amber-50/30" : "border-slate-100 bg-white"}`}>
      <div className="flex flex-wrap items-center gap-1.5">
        <span className={`rounded-full px-2 py-0.5 text-[10px] font-semibold ${statusTones[value.status]}`}>{statusLabels[value.status]}</span>
        <span className="rounded-full bg-slate-50 px-2 py-0.5 text-[10px] font-medium text-slate-500" title={value.method ?? undefined}>{originLabels[value.origin]}</span>
        {value.conflict && <span className="flex items-center gap-1 rounded-full bg-amber-100 px-2 py-0.5 text-[10px] font-semibold text-amber-800"><AlertTriangle className="size-3" /> Valores en conflicto</span>}
        {value.revisionNumber > 1 && <span className="text-[10px] text-slate-400">rev. {value.revisionNumber}</span>}
      </div>

      {empty ? (
        <p className="mt-2 text-xs text-slate-400">
          {value.status === "not_found" ? "El extractor buscó este dato sin encontrarlo. Esto no confirma que el artículo no lo reporte." : "Sin valor registrado."}
        </p>
      ) : (
        <p className={`mt-2 whitespace-pre-wrap break-words text-xs leading-5 text-slate-700 ${long && !expanded ? "line-clamp-4" : ""}`}>{value.displayValue}</p>
      )}
      {long && !empty && <button className="mt-1 text-[10px] font-semibold text-teal-700" onClick={() => setExpanded(!expanded)}>{expanded ? "Mostrar menos" : "Mostrar todo"}</button>}

      {value.evidence.map((evidence) => (
        <blockquote key={evidence.id} className="mt-2 border-l-2 border-teal-200 pl-2.5">
          <p className="text-[11px] italic leading-4 text-slate-500">“{evidence.quote}”</p>
          <div className="mt-1 flex items-center gap-2">
            {evidence.page ? (
              <button className="flex items-center gap-1 text-[10px] font-semibold text-teal-700 hover:text-teal-600" onClick={() => onShowEvidence(evidence)}>
                <FileSearch className="size-3" /> Ver en página {evidence.page}
              </button>
            ) : (
              <span className="text-[10px] text-slate-400">Sin página</span>
            )}
            {evidence.verified === false && <span className="text-[10px] text-amber-700">Cita no localizada en el texto de la página</span>}
          </div>
        </blockquote>
      ))}
      {value.notes && <p className="mt-2 text-[10px] leading-4 text-slate-400">{value.notes}</p>}
      {value.reviewerName && value.reviewedAt && <p className="mt-1 text-[10px] text-slate-400">Revisado por {value.reviewerName} · {formatDateTime(value.reviewedAt)}</p>}
      {error && <p className="mt-2 rounded-md bg-rose-50 px-2 py-1.5 text-[11px] text-rose-700">{error}</p>}

      {mode === "view" && (
        <div className="mt-2.5 flex flex-wrap gap-1.5">
          {!empty && value.status !== "confirmed" && <SmallButton disabled={saving} onClick={() => void review({ status: "confirmed", notes: value.notes, reason: "Confirmado contra la evidencia" })}><Check className="size-3" /> Confirmar</SmallButton>}
          {!empty && value.status !== "rejected" && <SmallButton disabled={saving} onClick={() => void review({ status: "rejected", notes: value.notes, reason: "Rechazado en revisión" })}><X className="size-3" /> Rechazar</SmallButton>}
          {value.status === "not_found" && (
            <>
              <SmallButton disabled={saving} onClick={() => void review({ status: "not_reported", notes: value.notes, reason: "El artículo no reporta este dato" })}>No reportado</SmallButton>
              <SmallButton disabled={saving} onClick={() => void review({ status: "not_applicable", notes: value.notes, reason: "No aplica a este estudio" })}>No aplica</SmallButton>
            </>
          )}
          <SmallButton onClick={() => setMode("edit")}><Pencil className="size-3" /> Editar</SmallButton>
          <SmallButton onClick={() => setMode("history")}><History className="size-3" /> Historial</SmallButton>
          {saving && <LoaderCircle className="size-3.5 animate-spin text-teal-600" />}
        </div>
      )}
      {mode === "edit" && <EditForm field={field} value={value} saving={saving} onCancel={() => setMode("view")} onSubmit={review} />}
      {mode === "history" && <HistoryList value={value} onClose={() => setMode("view")} onReverted={(updated) => { onChanged(updated); setMode("view"); }} />}
    </div>
  );
}

function EditForm({ field, value, saving, onCancel, onSubmit }: { field: MasterField; value: MasterValue; saving: boolean; onCancel: () => void; onSubmit: (input: Omit<UpdateMasterValueInput, "valueId">) => Promise<void> }) {
  const [text, setText] = useState(editableText(value));
  const [status, setStatus] = useState<UpdateMasterValueInput["status"]>(value.status === "proposed" || value.status === "not_found" ? "confirmed" : value.status);
  const [notes, setNotes] = useState(value.notes);
  const [reason, setReason] = useState("");
  const editable = field.dataType !== "metric";

  function submit(event: FormEvent) {
    event.preventDefault();
    void onSubmit({ valueText: editable ? text : undefined, status, notes, reason });
  }

  return (
    <form className="mt-3 space-y-2 border-t border-slate-100 pt-3" onSubmit={submit}>
      {editable ? (
        <label className="label">Valor{field.dataType === "list" && <span className="ml-1 font-normal text-slate-400">· separado por punto y coma</span>}
          <textarea className="input mt-1 min-h-16 resize-y" value={text} onChange={(event) => setText(event.target.value)} />
        </label>
      ) : (
        <p className="text-[11px] text-slate-400">Las métricas estructuradas conservan su valor; registra correcciones en las notas o agrega un valor manual.</p>
      )}
      <label className="label">Estado
        <select className="input mt-1" value={status} onChange={(event) => setStatus(event.target.value as UpdateMasterValueInput["status"])}>
          {reviewStatuses.map((item) => <option key={item} value={item}>{statusLabels[item]}</option>)}
        </select>
      </label>
      <label className="label">Notas<input className="input mt-1" value={notes} onChange={(event) => setNotes(event.target.value)} /></label>
      <label className="label">Motivo del cambio<input className="input mt-1" value={reason} onChange={(event) => setReason(event.target.value)} placeholder="Queda en el historial" /></label>
      <div className="flex justify-end gap-1.5">
        <SmallButton type="button" onClick={onCancel}>Cancelar</SmallButton>
        <button className="button-primary px-3 py-1.5 text-[11px]" disabled={saving}>{saving && <LoaderCircle className="size-3 animate-spin" />} Guardar revisión</button>
      </div>
    </form>
  );
}

function HistoryList({ value, onClose, onReverted }: { value: MasterValue; onClose: () => void; onReverted: (value: MasterValue) => void }) {
  const [revisions, setRevisions] = useState<ValueRevision[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    backend
      .masterValueHistory(value.id)
      .then((items) => active && setRevisions(items))
      .catch((cause) => active && setError(errorMessage(cause)));
    return () => {
      active = false;
    };
  }, [value.id, value.revisionNumber]);

  async function revert(revisionNumber: number) {
    try {
      onReverted(await backend.revertMasterValue(value.id, revisionNumber));
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  return (
    <div className="mt-3 border-t border-slate-100 pt-3">
      <div className="flex items-center justify-between">
        <p className="text-[10px] font-semibold uppercase tracking-[0.1em] text-slate-400">Historial de revisiones</p>
        <SmallButton onClick={onClose}>Cerrar</SmallButton>
      </div>
      {error && <p className="mt-2 text-[11px] text-rose-700">{error}</p>}
      {!revisions ? <LoaderCircle className="mt-2 size-4 animate-spin text-teal-600" /> : (
        <ol className="mt-2 space-y-2">
          {revisions.map((revision) => (
            <li key={revision.revisionNumber} className="rounded-lg bg-slate-50 p-2">
              <div className="flex items-center justify-between gap-2">
                <span className="text-[10px] font-semibold text-slate-600">Rev. {revision.revisionNumber} · {statusLabels[revision.status]}</span>
                {revision.revisionNumber !== value.revisionNumber && (
                  <button className="flex items-center gap-1 text-[10px] font-semibold text-teal-700" onClick={() => void revert(revision.revisionNumber)}><RotateCcw className="size-3" /> Restaurar</button>
                )}
              </div>
              <p className="mt-1 break-words text-[11px] text-slate-600">{revision.displayValue || "—"}</p>
              <p className="mt-1 text-[10px] text-slate-400">{revision.actorName} ({revision.actorType}) · {formatDateTime(revision.createdAt)}{revision.reason ? ` · ${revision.reason}` : ""}</p>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

function SmallButton({ children, type = "button", ...props }: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type={type} className="inline-flex items-center gap-1 rounded-md border border-slate-200 bg-white px-2 py-1 text-[10px] font-semibold text-slate-600 transition hover:bg-slate-50 disabled:opacity-50" {...props}>{children}</button>;
}
