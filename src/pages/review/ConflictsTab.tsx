import { useState } from "react";
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
        {open.length === 0 ? <Empty title="Sin conflictos abiertos" detail="Aparecen cuando dos revisores difieren en el cribado o en la calidad." /> : (
          <div className="space-y-3">{open.map((item) => <ConflictItem key={item.id} conflict={item} onResolved={async () => { await conflicts.reload(); onChange(); }} onOpenQuality={onOpenQuality} />)}</div>
        )}
      </Card>
      {resolved.length > 0 && (
        <Card title={`Resueltos (${resolved.length})`}>
          <ul className="space-y-1 text-xs text-slate-500">{resolved.map((item) => <li key={item.id}>Id {item.sequence} · {item.title} · {item.conflictType === "screening" ? "cribado" : "calidad"} · {item.resolution} · {item.resolvedAt ? formatDateTime(item.resolvedAt) : ""}</li>)}</ul>
        </Card>
      )}
    </div>
  );
}

function ConflictItem({ conflict, onResolved, onOpenQuality }: { conflict: ConflictInfo; onResolved: () => Promise<void>; onOpenQuality: () => void }) {
  const [decision, setDecision] = useState<Decision>("include");
  const [reason, setReason] = useState("");
  const action = useAction();

  async function resolve() {
    if (await action.run(() => reviewBackend.resolveScreening(conflict.id, decision, undefined, reason))) await onResolved();
  }

  return (
    <article className="rounded-xl border border-amber-200 bg-amber-50/40 p-3">
      <p className="text-xs font-semibold text-slate-800">Id {conflict.sequence} · {conflict.title}</p>
      <p className="text-[11px] text-slate-500">{conflict.conflictType === "screening" ? `Cribado (${conflict.phase === "full_text" ? "texto completo" : "metadatos"})` : "Calidad"} · desde {formatDateTime(conflict.createdAt)}</p>
      {conflict.details.decisions?.map((item, index) => <p key={index} className="text-[11px] text-slate-600">{item.reviewer}: {decisionLabels[item.decision]}{item.reason ? ` — ${item.reason}` : ""}</p>)}
      {conflict.details.criteria && <p className="text-[11px] text-slate-600">Difieren en: {conflict.details.criteria.join(", ")}</p>}
      {conflict.conflictType === "screening" ? (
        <div className="mt-2 flex flex-wrap gap-2">
          <select className="input w-48" value={decision} onChange={(event) => setDecision(event.target.value as Decision)} aria-label="Decisión de consenso">{(Object.keys(decisionLabels) as Decision[]).map((item) => <option key={item} value={item}>{decisionLabels[item]}</option>)}</select>
          <input className="input flex-1" placeholder="Motivo de la resolución" value={reason} onChange={(event) => setReason(event.target.value)} />
          <button className="button-primary px-3 py-2" disabled={reason.trim().length < 3 || action.busy} onClick={() => void resolve()}>Resolver</button>
        </div>
      ) : <button className="button-secondary mt-2 px-3 py-1.5 text-[11px]" onClick={onOpenQuality}>Registrar consenso en Calidad</button>}
      <div className="mt-2"><ErrorNote message={action.error} /></div>
    </article>
  );
}
