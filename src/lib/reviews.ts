import type { ReviewType } from "../types";

export const reviewTypeLabels: Record<ReviewType, string> = {
  systematic_review: "Revisión sistemática",
  systematic_mapping: "Mapeo sistemático",
  scoping_review: "Revisión de alcance",
};

export const projectStatusLabels: Record<"draft" | "active" | "completed" | "archived", string> = {
  draft: "Borrador",
  active: "En curso",
  completed: "Completada",
  archived: "Archivada",
};

export function formatDate(value: string): string {
  return new Intl.DateTimeFormat("es", {
    day: "2-digit",
    month: "short",
    year: "numeric",
  }).format(new Date(value));
}
