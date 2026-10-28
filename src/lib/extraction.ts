import type { FieldGroup, JobStatus, MasterValue, ValueStatus } from "../types";

export const groupLabels: Record<FieldGroup, string> = {
  bibliography: "Bibliografía",
  context: "Contexto científico",
  design_methods: "Diseño y métodos",
  evaluation_results: "Evaluación y resultados",
  ethics_reproducibility: "Ética y reproducibilidad",
};

export const statusLabels: Record<ValueStatus, string> = {
  confirmed: "Confirmado",
  proposed: "Propuesto",
  needs_review: "Revisar",
  not_reported: "No reportado",
  not_found: "No encontrado",
  not_applicable: "No aplica",
  rejected: "Rechazado",
};

export const statusTones: Record<ValueStatus, string> = {
  confirmed: "bg-emerald-50 text-emerald-700",
  proposed: "bg-sky-50 text-sky-700",
  needs_review: "bg-amber-50 text-amber-800",
  not_reported: "bg-slate-100 text-slate-600",
  not_found: "bg-slate-50 text-slate-400",
  not_applicable: "bg-slate-100 text-slate-500",
  rejected: "bg-rose-50 text-rose-700",
};

export const originLabels: Record<MasterValue["origin"], string> = {
  rules: "Reglas",
  ocr: "OCR",
  grobid: "GROBID",
  ai: "IA",
  human: "Manual",
  import: "Importación",
};

/** Statuses a person can assign; `proposed` and `not_found` belong to the extractor. */
export const reviewStatuses = ["confirmed", "needs_review", "not_reported", "not_applicable", "rejected"] as const;

export const jobStatusLabels: Record<JobStatus, string> = {
  queued: "En cola",
  running: "En ejecución",
  paused: "Pausada",
  completed: "Completada",
  failed: "Fallida",
  cancelled: "Cancelada",
};

export const jobStatusTones: Record<JobStatus, string> = {
  queued: "bg-slate-100 text-slate-600",
  running: "bg-teal-50 text-teal-700",
  paused: "bg-amber-50 text-amber-800",
  completed: "bg-emerald-50 text-emerald-700",
  failed: "bg-rose-50 text-rose-700",
  cancelled: "bg-slate-100 text-slate-500",
};

export const jobTypeLabels: Record<string, string> = {
  scientific_extraction: "Extracción científica",
  extractor_setup: "Preparar entorno del extractor",
  rag_index: "Indexación para búsqueda",
  ai_field_extraction: "Propuestas de IA para la ficha",
  model_pull: "Descarga de modelo en Ollama",
};

/** Link to an article page with a highlighted region (used by assistant citations). */
export function evidenceLink(articleId: string, page: number, bbox?: number[], pageWidth?: number): string {
  const params = new URLSearchParams({ page: String(page) });
  if (bbox?.length === 4) params.set("bbox", bbox.map((value) => value.toFixed(1)).join(","));
  if (pageWidth) params.set("pw", String(pageWidth));
  return `/biblioteca/${articleId}?${params.toString()}`;
}

export function isActiveJob(status: JobStatus): boolean {
  return status === "queued" || status === "running" || status === "paused";
}

/** Text shown in the edit box: lists become `a; b`, everything else its display text. */
export function editableText(value: MasterValue): string {
  if (Array.isArray(value.value)) return value.value.join("; ");
  if (value.value === null || value.value === undefined) return "";
  return value.displayValue;
}

export function formatDateTime(value: string): string {
  return new Intl.DateTimeFormat("es", {
    day: "2-digit",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}
