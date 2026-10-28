import { useState } from "react";
import { maintenance } from "../../lib/maintenance";
import { reviewBackend } from "../../lib/reviewBackend";
import { formatDateTime } from "../../lib/extraction";
import type { ConflictInfo, Decision } from "../../reviewTypes";
import { Card, Empty, ErrorNote, Loading, decisionLabels, useAction, useLoad } from "./shared";

export function ConflictsTab({ projectId, onChange, onOpenQuality }: { projectId: string; onChange: () => void; onOpenQuality: () => void }) {
  const conflicts = useLoad(() => reviewBackend.conflicts(projectId), [projectId]);
  if (!conflicts.data) return conflicts.loading ? <Loading /> : <ErrorNote message={conflicts.error} />;
  const open = conflicts.data.filter((item) => item.status === "open");
  const resolved = conflicts.data.filter((item) => item.status === "resolved");
  return (
    <div className="space-y-6">
      <Card title={`Conflictos abiertos (${open.length})`}>
        {open.length === 0 ? <Empty title="Sin conflictos abiertos" detail="Aparecen cuando dos revisores difieren en el cribado, la calidad o un valor extraído." /> : (
          <div className="space-y-3">{open.map((item) => <ConflictItem key={item.id} conflict={item} onResolved={async () => { await conflicts.reload(); onChange(); }} onOpenQuality={onOpenQuality} />)}</div>
        )}
      </Card>
      {resolved.length > 0 && (
        <Card title={`Resueltos (${resolved.length})`}>
          <ul className="space-y-1 text-xs text-slate-500">{resolved.map((item) => <li key={item.id}>Id {item.sequence} · {item.title} · {conflictTypeLabels[item.conflictType]} · {item.resolution} · {item.resolvedAt ? formatDateTime(item.resolvedAt) : ""}</li>)}</ul>
        </Card>
      )}
    </div>
  );
}

const conflictTypeLabels: Record<ConflictInfo["conflictType"], string> = { screening: "cribado", quality: "calidad", extraction: "extracción" };

function displayValue(value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "string") return value;
  if (Array.isArray(value)) return value.map(displayValue).join(", ");
  return JSON.stringify(value);
}

function ConflictItem({ conflict, onResolved, onOpenQuality }: { conflict: ConflictInfo; onResolved: () => Promise<void>; onOpenQuality: () => void }) {
  const [decision, setDecision] = useState<Decision>("include");
  const [reason, setReason] = useState("");
  const [keep, setKeep] = useState<string | null>(null);
  const action = useAction();

  async function resolve() {
    if (await action.run(() => reviewBackend.resolveScreening(conflict.id, decision, undefined, reason))) await onResolved();
  }

  async function resolveExtraction() {
    if (!keep) return;
    const done = await action.run(async () => {
      await maintenance.resolveExtractionConflict(conflict.id, keep, reason);
      return true;
    });
    if (done) await onResolved();
  }

  return (
    <article className="rounded-xl border border-amber-200 bg-amber-50/40 p-3">
      <p className="text-xs font-semibold text-slate-800">Id {conflict.sequence} · {conflict.title}</p>
      <p className="text-[11px] text-slate-500">{conflict.conflictType === "screening" ? `Cribado (${conflict.phase === "full_text" ? "texto completo" : "metadatos"})` : conflict.conflictType === "extraction" ? `Extracción · ${conflict.details.fieldName ?? "aspecto"}` : "Calidad"} · desde {formatDateTime(conflict.createdAt)}</p>
      {conflict.details.decisions?.map((item, index) => <p key={index} className="text-[11px] text-slate-600">{item.reviewer}: {decisionLabels[item.decision]}{item.reason ? ` — ${item.reason}` : ""}</p>)}
      {conflict.details.criteria && <p className="text-[11px] text-slate-600">Difieren en: {conflict.details.criteria.join(", ")}</p>}
      {conflict.conflictType === "screening" ? (
        <div className="mt-2 flex flex-wrap gap-2">
          <select className="input w-48" value={decision} onChange={(event) => setDecision(event.target.value as Decision)} aria-label="Decisión de consenso">{(Object.keys(decisionLabels) as Decision[]).map((item) => <option key={item} value={item}>{decisionLabels[item]}</option>)}</select>
          <input className="input flex-1" placeholder="Motivo de la resolución" value={reason} onChange={(event) => setReason(event.target.value)} />
          <button className="button-primary px-3 py-2" disabled={reason.trim().length < 3 || action.busy} onClick={() => void resolve()}>Resolver</button>
        </div>
      ) : conflict.conflictType === "extraction" ? (
        <fieldset className="mt-2 space-y-1">
          <legend className="sr-only">Valor que se conserva</legend>
          {conflict.details.values?.map((item) => (
            <label key={item.valueId} className="flex items-center gap-2 text-[11px] text-slate-700">
              <input type="radio" name={`keep-${conflict.id}`} className="accent-teal-600" checked={keep === item.valueId} onChange={() => setKeep(item.valueId)} />
              <span><span className="font-semibold">{item.reviewer}:</span> {displayValue(item.value)}</span>
            </label>
          ))}
          <div className="flex flex-wrap gap-2 pt-1">
            <input className="input flex-1" placeholder="Motivo de la resolución" value={reason} onChange={(event) => setReason(event.target.value)} />
            <button className="button-primary px-3 py-2" disabled={!keep || reason.trim().length < 3 || action.busy} onClick={() => void resolveExtraction()}>Conservar valor</button>
          </div>
          <p className="text-[10px] text-slate-400">Los demás valores se marcan como rechazados; no se borran.</p>
        </fieldset>
      ) : <button className="button-secondary mt-2 px-3 py-1.5 text-[11px]" onClick={onOpenQuality}>Registrar consenso en Calidad</button>}
      <div className="mt-2"><ErrorNote message={action.error} /></div>
    </article>
  );
}
