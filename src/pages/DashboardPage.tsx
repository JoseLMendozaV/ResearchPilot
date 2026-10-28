import {
  ArrowRight,
  BookOpen,
  FileCheck2,
  FolderKanban,
  ListTree,
  LockKeyhole,
  Sparkles,
} from "lucide-react";
import { Link } from "react-router-dom";
import { formatDate, reviewTypeLabels } from "../lib/reviews";
import type { BootstrapData } from "../types";

export function DashboardPage({ data }: { data: BootstrapData }) {
  const stats = [
    { label: "Revisiones", value: data.stats.projects, icon: FolderKanban, tone: "teal" },
    { label: "Artículos", value: data.stats.articles, icon: BookOpen, tone: "blue" },
    { label: "Preguntas RQ", value: data.stats.researchQuestions, icon: ListTree, tone: "violet" },
    { label: "Evidencias", value: data.stats.evidenceSpans, icon: FileCheck2, tone: "amber" },
  ] as const;

  return (
    <div className="mx-auto max-w-[1280px] p-8">
      <section className="relative overflow-hidden rounded-[22px] bg-slate-900 px-8 py-7 text-white shadow-sm">
        <div className="absolute -right-20 -top-28 size-80 rounded-full bg-teal-500/20 blur-3xl" />
        <div className="relative max-w-2xl">
          <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.16em] text-teal-300">
            <Sparkles className="size-4" /> Base científica local
          </div>
          <h2 className="mt-3 text-2xl font-semibold tracking-[-0.03em]">
            Convierte cada hallazgo en evidencia verificable.
          </h2>
          <p className="mt-2 max-w-xl text-sm leading-6 text-slate-300">
            Organiza revisiones, conserva la procedencia documental y mantén las decisiones humanas y de IA en un historial auditable.
          </p>
          <Link to="/revisiones" className="mt-5 inline-flex items-center gap-2 rounded-lg bg-teal-400 px-4 py-2.5 text-sm font-semibold text-slate-950 transition hover:bg-teal-300">
            Crear una revisión <ArrowRight className="size-4" />
          </Link>
        </div>
      </section>

      <section className="mt-6 grid grid-cols-4 gap-4">
        {stats.map(({ label, value, icon: Icon, tone }) => (
          <article key={label} className="panel flex items-center gap-4 p-5">
            <div className={`metric-icon metric-${tone}`}><Icon className="size-5" strokeWidth={1.8} /></div>
            <div>
              <p className="text-2xl font-semibold tracking-[-0.03em] text-slate-900">{value}</p>
              <p className="text-xs font-medium text-slate-400">{label}</p>
            </div>
          </article>
        ))}
      </section>

      <section className="mt-6 grid grid-cols-[1.5fr_1fr] gap-6">
        <div className="panel p-6">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="section-title">Revisiones recientes</h3>
              <p className="section-subtitle">Proyectos guardados en este equipo</p>
            </div>
            <Link to="/revisiones" className="text-xs font-semibold text-teal-700 hover:text-teal-600">Ver todas</Link>
          </div>
          {data.projects.length ? (
            <div className="mt-5 divide-y divide-slate-100">
              {data.projects.slice(0, 4).map((project) => (
                <div key={project.id} className="flex items-center gap-4 py-3.5 first:pt-0 last:pb-0">
                  <div className="grid size-9 place-items-center rounded-lg bg-teal-50 text-teal-700"><FolderKanban className="size-4" /></div>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-semibold text-slate-800">{project.name}</p>
                    <p className="mt-0.5 text-xs text-slate-400">{reviewTypeLabels[project.reviewType]} · {formatDate(project.updatedAt)}</p>
                  </div>
                  <span className="status-badge">Borrador</span>
                </div>
              ))}
            </div>
          ) : (
            <div className="mt-5 rounded-xl border border-dashed border-slate-200 bg-slate-50/60 px-5 py-8 text-center">
              <FolderKanban className="mx-auto size-6 text-slate-300" />
              <p className="mt-2 text-sm font-medium text-slate-600">Aún no hay revisiones</p>
              <p className="mt-1 text-xs text-slate-400">Crea la primera para comenzar el protocolo.</p>
            </div>
          )}
        </div>

        <div className="panel p-6">
          <h3 className="section-title">Entorno seguro</h3>
          <p className="section-subtitle">Estado de privacidad de la aplicación</p>
          <div className="mt-5 space-y-3">
            <StatusRow icon={LockKeyhole} title="Documentos locales" detail="Sin transferencias automáticas" />
            <StatusRow icon={FileCheck2} title="Trazabilidad activa" detail="Esquema de auditoría preparado" />
            <StatusRow icon={Sparkles} title="IA predeterminada" detail={data.settings.aiProvider === "ollama" ? (data.settings.ollamaModel ? `Ollama · ${data.settings.ollamaModel}` : "Ollama · modelo por descubrir") : data.settings.aiProvider} />
          </div>
        </div>
      </section>
    </div>
  );
}

function StatusRow({ icon: Icon, title, detail }: { icon: typeof LockKeyhole; title: string; detail: string }) {
  return (
    <div className="flex items-center gap-3 rounded-xl bg-slate-50 p-3">
      <div className="grid size-8 place-items-center rounded-lg bg-white text-emerald-600 shadow-sm"><Icon className="size-4" /></div>
      <div className="min-w-0">
        <p className="text-xs font-semibold text-slate-700">{title}</p>
        <p className="truncate text-[11px] text-slate-400">{detail}</p>
      </div>
    </div>
  );
}
