import { reviewBackend } from "../../lib/reviewBackend";
import { formatDateTime } from "../../lib/extraction";
import { Card, Empty, ErrorNote, Loading, useLoad } from "./shared";

export function AuditTab({ projectId }: { projectId: string }) {
  const audit = useLoad(() => reviewBackend.audit(projectId), [projectId]);
  if (!audit.data) return audit.loading ? <Loading /> : <ErrorNote message={audit.error} />;
  return (
    <Card title="Diario de decisiones y cambios">
      {audit.data.length === 0 ? <Empty title="Sin eventos" detail="Cada cambio del protocolo, cribado, calidad y extracción queda registrado aquí." /> : (
        <ol className="space-y-1.5">
          {audit.data.map((entry, index) => (
            <li key={index} className="grid grid-cols-[150px_160px_1fr] gap-3 border-b border-slate-50 pb-1.5 text-[11px]">
              <span className="text-slate-400">{formatDateTime(entry.createdAt)}</span>
              <span className="font-medium text-slate-600">{entry.actorName}</span>
              <span className="text-slate-500"><strong className="text-slate-700">{entry.entityType}</strong> · {entry.action}{entry.after ? ` · ${JSON.stringify(entry.after).slice(0, 160)}` : ""}</span>
            </li>
          ))}
        </ol>
      )}
    </Card>
  );
}
