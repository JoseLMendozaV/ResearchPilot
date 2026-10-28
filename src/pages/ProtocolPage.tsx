import { GuideContent } from "./review/GuideContent";
import {
  BookOpenCheck,
  CheckCircle2,
  ClipboardCheck,
  FileSpreadsheet,
  Filter,
  ListChecks,
  Network,
} from "lucide-react";

export const workflowModules = [
  { sheet: "Leer", title: "Guía metodológica", description: "Contexto, referencias base y orientación reproducible.", icon: BookOpenCheck },
  { sheet: "Control", title: "Artículos de control", description: "Conjunto semilla para validar cobertura de las búsquedas.", icon: CheckCircle2 },
  { sheet: "RQs", title: "Preguntas de investigación", description: "Preguntas, objetivos y aspectos configurables.", icon: ListChecks },
  { sheet: "Cadena", title: "Estrategias de búsqueda", description: "Cadenas, bases consultadas, fechas y resultados.", icon: Network },
  { sheet: "I_E", title: "Inclusión y exclusión", description: "Criterios y decisiones de cribado con responsable.", icon: Filter },
  { sheet: "Calidad", title: "Evaluación de calidad", description: "Rúbricas ponderadas, puntajes y justificaciones.", icon: ClipboardCheck },
  { sheet: "Extraccion", title: "Extracción de datos", description: "Campos por RQ, valores y evidencia documental.", icon: FileSpreadsheet },
];

export function ProtocolPage() {
  return (
    <div className="mx-auto max-w-[1280px] p-8">
      <div className="panel p-6">
        <div className="flex items-start justify-between gap-8">
          <div>
            <h2 className="section-title">Cobertura del formato de evidencia</h2>
            <p className="mt-1 max-w-2xl text-sm leading-6 text-slate-500">
              Cada hoja del formato de evidencia es un módulo interactivo dentro de cada revisión: abre una revisión y usa sus pestañas Leer, Control, RQs, Cadena, I/E, Calidad y Extracción.
            </p>
          </div>
          <span className="shrink-0 rounded-full bg-teal-50 px-3 py-1.5 text-xs font-semibold text-teal-700">7 de 7 con editor</span>
        </div>
        <div className="mt-6 grid grid-cols-2 gap-3" data-testid="workflow-modules">
          {workflowModules.map(({ sheet, title, description, icon: Icon }, index) => (
            <article key={sheet} className="flex gap-4 rounded-xl border border-slate-100 bg-slate-50/70 p-4">
              <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-white text-teal-700 shadow-sm"><Icon className="size-5" strokeWidth={1.7} /></div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <h3 className="text-sm font-semibold text-slate-800">{title}</h3>
                  <span className="rounded bg-slate-200/70 px-1.5 py-0.5 font-mono text-[10px] text-slate-500">{sheet}</span>
                </div>
                <p className="mt-1 text-xs leading-5 text-slate-400">{description}</p>
              </div>
              <div className="text-[11px] font-medium text-slate-300">0{index + 1}</div>
            </article>
          ))}
        </div>
      </div>
      <div className="mt-5 rounded-xl border border-slate-200 bg-white px-4 py-3 text-xs leading-5 text-slate-500">
        Desde la pestaña Exportar de cada revisión se genera el libro de Excel con las siete hojas (formato compatible) o la versión ampliada con procedencia, además de CSV, JSON, RIS, BibTeX, Markdown y LaTeX.
      </div>
      <div className="mt-6"><GuideContent /></div>
    </div>
  );
}
