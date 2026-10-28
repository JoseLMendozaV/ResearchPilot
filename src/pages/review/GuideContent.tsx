import { AlertTriangle, BookOpenCheck } from "lucide-react";

const steps = [
  { title: "1. Protocolo", text: "Define y aprueba el protocolo antes de buscar: objetivos, preguntas de investigación (RQs), criterios de inclusión y exclusión, bases de datos, plan de calidad y de extracción. Los cambios posteriores quedan como enmiendas con motivo." },
  { title: "2. Artículos de control", text: "Identifica al menos cinco estudios clave que esperas recuperar. Sirven para comprobar la sensibilidad de las cadenas: si una cadena no recupera artículos de control, revísala." },
  { title: "3. Cadenas de búsqueda", text: "Registra por base de datos la fecha, la sintaxis exacta y la cantidad de resultados. Exporta los resultados (RIS, BibTeX o CSV) e impórtalos: se conservan los registros originales y se eliminan duplicados por DOI sin fusionar nada automáticamente." },
  { title: "4. Cribado por metadatos (I/E)", text: "Decide con título, resumen y palabras clave aplicando los criterios. Ante la duda, pasa el estudio a texto completo. Conserva los estudios secundarios para trabajos relacionados y validación." },
  { title: "5. Elegibilidad por texto completo", text: "Lee el texto completo y registra el criterio de exclusión de cada estudio excluido: esos motivos alimentan el diagrama PRISMA." },
  { title: "6. Evaluación de calidad", text: "Responde Q1…Qn sobre el texto completo con la escala definida. La calidad es independiente del cribado y de la extracción; usa instrumentos adecuados a tu disciplina." },
  { title: "7. Extracción", text: "Completa la matriz por RQ con valores vinculados a citas y páginas del documento. Varias filas de valores por estudio son válidas cuando hay varios resultados o configuraciones." },
  { title: "8. Síntesis y reporte", text: "Resume con tablas y narrativa trazable. No compares métricas obtenidas con datasets o configuraciones distintas como si fueran equivalentes." },
];

const types = [
  { name: "Revisión sistemática (SLR)", text: "Responde preguntas específicas evaluando y sintetizando la evidencia; exige evaluación de calidad rigurosa y síntesis de resultados." },
  { name: "Mapeo sistemático", text: "Clasifica y cuantifica la investigación de un área amplia (qué se ha estudiado, cómo y dónde); la extracción suele ser categórica. Ver Petersen, Vakkalanka y Kuzniarz (2015)." },
  { name: "Revisión de alcance (scoping)", text: "Explora la extensión y naturaleza de la literatura y los conceptos clave; la evaluación de calidad es opcional según el objetivo." },
];

export function GuideContent({ reviewType }: { reviewType?: string }) {
  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <section className="panel p-6">
        <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-800"><BookOpenCheck className="size-4 text-teal-600" /> Guía metodológica</h2>
        <p className="mt-2 text-xs leading-5 text-slate-500">Orientación para planificar y ejecutar un estudio secundario reproducible. Cada paso tiene un módulo en esta revisión; la herramienta registra decisiones, no las toma por ti.</p>
        <div className="mt-4 grid grid-cols-3 gap-3">
          {types.map((type) => (
            <div key={type.name} className={`rounded-xl p-3 ${reviewType && type.name.toLowerCase().includes(reviewType === "systematic_mapping" ? "mapeo" : reviewType === "scoping_review" ? "alcance" : "sistemática (slr)") ? "bg-teal-50" : "bg-slate-50"}`}>
              <p className="text-xs font-semibold text-slate-700">{type.name}</p>
              <p className="mt-1 text-[11px] leading-4 text-slate-500">{type.text}</p>
            </div>
          ))}
        </div>
      </section>
      <section className="panel p-6">
        <ol className="space-y-3">{steps.map((step) => <li key={step.title}><p className="text-xs font-semibold text-slate-700">{step.title}</p><p className="mt-0.5 text-xs leading-5 text-slate-500">{step.text}</p></li>)}</ol>
      </section>
      <section className="rounded-xl border border-amber-200 bg-amber-50 p-4 text-xs leading-5 text-amber-900">
        <p className="flex items-center gap-2 font-semibold"><AlertTriangle className="size-4" /> Advertencias</p>
        <ul className="mt-1 list-disc space-y-1 pl-5">
          <li>PRISMA 2020 es una guía de reporte: describe qué informar, no sustituye la metodología ni decide por ti.</li>
          <li>La cobertura de los artículos de control indica sensibilidad de las cadenas, no garantiza que la búsqueda sea completa.</li>
          <li>Las propuestas de IA son sugerencias revisables; la inclusión, la calidad y los datos extraídos son decisiones de los revisores.</li>
        </ul>
      </section>
      <section className="panel p-6 text-[11px] leading-5 text-slate-500">
        <p className="font-semibold text-slate-700">Referencias</p>
        <p>Petersen, K., Vakkalanka, S., y Kuzniarz, L. (2015). Guidelines for conducting systematic mapping studies in software engineering: An update. <em>Information and Software Technology</em>, 64, 1–18.</p>
        <p>Kitchenham, B., y Charters, S. (2007). <em>Guidelines for performing systematic literature reviews in software engineering</em> (EBSE-2007-01).</p>
        <p>Page, M. J., et al. (2021). The PRISMA 2020 statement: an updated guideline for reporting systematic reviews. <em>BMJ</em>, 372, n71.</p>
        <p>Tricco, A. C., et al. (2018). PRISMA extension for scoping reviews (PRISMA-ScR). <em>Annals of Internal Medicine</em>, 169(7), 467–473.</p>
      </section>
    </div>
  );
}
