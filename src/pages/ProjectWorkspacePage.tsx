import { useState } from "react";
import { ArrowLeft, Users } from "lucide-react";
import { Link, useParams } from "react-router-dom";
import { reviewBackend } from "../lib/reviewBackend";
import { reviewTypeLabels } from "../lib/reviews";
import type { LibraryArticle } from "../types";
import { AuditTab } from "./review/AuditTab";
import { ConflictsTab } from "./review/ConflictsTab";
import { ControlTab } from "./review/ControlTab";
import { ExportTab } from "./review/ExportTab";
import { ExtractionTab } from "./review/ExtractionTab";
import { GuideContent } from "./review/GuideContent";
import { OverviewTab } from "./review/OverviewTab";
import { ProtocolTab } from "./review/ProtocolTab";
import { QualityTab } from "./review/QualityTab";
import { QuestionsTab } from "./review/QuestionsTab";
import { ScreeningTab } from "./review/ScreeningTab";
import { SearchTab } from "./review/SearchTab";
import { ErrorNote, Loading, useLoad } from "./review/shared";

const tabs = [
  { key: "overview", label: "Resumen" },
  { key: "guide", label: "Leer" },
  { key: "protocol", label: "Protocolo" },
  { key: "control", label: "Control" },
  { key: "questions", label: "RQs" },
  { key: "search", label: "Cadena" },
  { key: "screening", label: "I/E" },
  { key: "quality", label: "Calidad" },
  { key: "extraction", label: "Extracción" },
  { key: "conflicts", label: "Conflictos" },
  { key: "export", label: "Exportar" },
  { key: "audit", label: "Auditoría" },
] as const;

type TabKey = (typeof tabs)[number]["key"];

export function ProjectWorkspacePage({ articles }: { articles: LibraryArticle[] }) {
  const { projectId = "" } = useParams();
  const [tab, setTab] = useState<TabKey>("overview");
  const workspace = useLoad(() => reviewBackend.workspace(projectId), [projectId]);
  const data = workspace.data;

  async function switchReviewer(reviewerId: string) {
    workspace.setData(await reviewBackend.setActiveReviewer(projectId, reviewerId));
  }

  if (workspace.loading && !data) return <Loading />;
  if (!data) return <div className="p-8"><ErrorNote message={workspace.error ?? "Revisión no encontrada"} /></div>;
  const refresh = () => void workspace.reload();

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-b border-slate-200 bg-white px-8 pt-4">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <Link to="/revisiones" className="flex items-center gap-1.5 text-xs font-semibold text-slate-500 hover:text-teal-700"><ArrowLeft className="size-4" /> Revisiones</Link>
            <h2 className="mt-2 truncate text-lg font-semibold tracking-[-0.02em] text-slate-900">{data.project.name}</h2>
            <p className="text-xs text-slate-400">{reviewTypeLabels[data.project.reviewType]} · revisión {data.project.screeningMode === "double" ? "doble" : "simple"} · {data.counts.total} estudios · {data.counts.included} incluidos{data.openConflicts ? ` · ${data.openConflicts} conflicto(s) abiertos` : ""}</p>
          </div>
          <label className="flex shrink-0 items-center gap-2 text-xs text-slate-500">
            <Users className="size-4" /> Registrando como
            <select className="input w-48" value={data.activeReviewerId} onChange={(event) => void switchReviewer(event.target.value)} aria-label="Revisor activo">
              {data.reviewers.map((reviewer) => <option key={reviewer.id} value={reviewer.id}>{reviewer.displayName}{reviewer.isLocalProfile ? " (yo)" : ""}</option>)}
            </select>
          </label>
        </div>
        <nav className="mt-3 flex gap-1 overflow-x-auto" role="tablist" aria-label="Módulos de la revisión">
          {tabs.map((item) => (
            <button key={item.key} role="tab" aria-selected={tab === item.key} onClick={() => setTab(item.key)} className={`whitespace-nowrap border-b-2 px-3 pb-2.5 text-xs font-semibold transition ${tab === item.key ? "border-teal-600 text-teal-800" : "border-transparent text-slate-400 hover:text-slate-600"}`}>
              {item.label}{item.key === "conflicts" && data.openConflicts > 0 ? ` (${data.openConflicts})` : ""}
            </button>
          ))}
        </nav>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-8">
        {tab === "overview" && <OverviewTab workspace={data} onChange={(next) => workspace.setData(next)} articles={articles} />}
        {tab === "guide" && <GuideContent reviewType={data.project.reviewType} />}
        {tab === "protocol" && <ProtocolTab projectId={projectId} versions={data.protocol} onChange={refresh} />}
        {tab === "control" && <ControlTab projectId={projectId} articles={articles} />}
        {tab === "questions" && <QuestionsTab projectId={projectId} />}
        {tab === "search" && <SearchTab projectId={projectId} onChange={refresh} />}
        {tab === "screening" && <ScreeningTab projectId={projectId} onChange={refresh} />}
        {tab === "quality" && <QualityTab projectId={projectId} onChange={refresh} />}
        {tab === "extraction" && <ExtractionTab projectId={projectId} />}
        {tab === "conflicts" && <ConflictsTab projectId={projectId} onChange={refresh} onOpenQuality={() => setTab("quality")} />}
        {tab === "export" && <ExportTab projectId={projectId} projectName={data.project.name} />}
        {tab === "audit" && <AuditTab projectId={projectId} />}
      </div>
    </div>
  );
}
