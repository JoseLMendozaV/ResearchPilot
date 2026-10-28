import { useState } from "react";
import { CheckCircle2, Save } from "lucide-react";
import { reviewBackend } from "../../lib/reviewBackend";
import { formatDateTime } from "../../lib/extraction";
import type { ProtocolVersion } from "../../reviewTypes";
import { Card, ErrorNote, useAction } from "./shared";

const sections = [
  ["title", "Título"],
  ["rationale", "Justificación"],
  ["objectives", "Objetivos"],
  ["scope", "Alcance (población, intervención, contexto)"],
  ["databases", "Bases de datos y fuentes"],
  ["searchPlan", "Estrategia de búsqueda"],
  ["evaluators", "Evaluadores y roles"],
  ["schedule", "Fechas y cronograma"],
  ["qualityPlan", "Plan de evaluación de calidad"],
  ["extractionPlan", "Plan de extracción y síntesis"],
] as const;

const statusLabels = { draft: "Borrador", approved: "Aprobado", superseded: "Reemplazado" } as const;

export function ProtocolTab({ projectId, versions, onChange }: { projectId: string; versions: ProtocolVersion[]; onChange: () => void }) {
  const latest = versions[0];
  const [content, setContent] = useState<Record<string, string>>(() => Object.fromEntries(sections.map(([key]) => [key, String(latest?.content[key] ?? "")])));
  const [reason, setReason] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const action = useAction();
  const approved = latest?.status === "approved";

  async function save() {
    const result = await action.run(() => reviewBackend.saveProtocol(projectId, content, approved ? reason : undefined));
    if (result) {
      setNotice(approved ? "Enmienda registrada como nueva versión en borrador" : "Borrador guardado");
      setReason("");
      onChange();
    }
  }

  async function approve() {
    const result = await action.run(() => reviewBackend.approveProtocol(projectId));
    if (result) {
      setNotice("Protocolo aprobado: se congeló una instantánea de RQs, criterios, calidad y cadenas");
      onChange();
    }
  }

  return (
    <div className="grid grid-cols-[1.6fr_1fr] gap-6">
      <Card title={latest ? `Protocolo · versión ${latest.versionNumber} (${statusLabels[latest.status]})` : "Protocolo · nuevo"}>
        <div className="space-y-3">
          {sections.map(([key, label]) => (
            <label key={key} className="label">{label}
              <textarea className="input mt-1 min-h-14 resize-y" value={content[key]} onChange={(event) => setContent({ ...content, [key]: event.target.value })} />
            </label>
          ))}
          {approved && (
            <label className="label">Motivo de la enmienda (obligatorio para modificar un protocolo aprobado)
              <input className="input mt-1" value={reason} onChange={(event) => setReason(event.target.value)} />
            </label>
          )}
          <ErrorNote message={action.error} />
          <div className="flex items-center justify-end gap-2">
            {notice && <span className="text-xs text-emerald-700">{notice}</span>}
            <button className="button-secondary" disabled={action.busy || (approved && reason.trim().length < 5)} onClick={() => void save()}><Save className="size-4" /> {approved ? "Registrar enmienda" : "Guardar borrador"}</button>
            {latest?.status === "draft" && <button className="button-primary" disabled={action.busy} onClick={() => void approve()}><CheckCircle2 className="size-4" /> Aprobar versión</button>}
          </div>
        </div>
      </Card>
      <Card title="Historial de versiones">
        {versions.length === 0 ? <p className="text-xs text-slate-400">Sin versiones todavía.</p> : (
          <ol className="space-y-2">
            {versions.map((version) => (
              <li key={version.id} className="rounded-lg bg-slate-50 p-3 text-[11px]">
                <p className="font-semibold text-slate-700">v{version.versionNumber} · {statusLabels[version.status]}</p>
                <p className="text-slate-400">{version.changeSummary} · {version.createdBy} · {formatDateTime(version.createdAt)}</p>
                {version.approvedAt && <p className="text-emerald-700">Aprobado {formatDateTime(version.approvedAt)}</p>}
                {version.amendments.map((amendment, index) => <p key={index} className="mt-1 text-amber-800">Enmienda: {amendment.reason}{amendment.change.changedSections?.length ? ` (${amendment.change.changedSections.join(", ")})` : ""}</p>)}
              </li>
            ))}
          </ol>
        )}
      </Card>
    </div>
  );
}
