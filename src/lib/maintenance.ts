import { invoke } from "@tauri-apps/api/core";

export interface BackupResult {
  path: string;
  sha256: string;
  bytes: number;
  files: number;
  createdAt: string;
}

export interface BackupInfo {
  createdAt: string;
  appVersion: string;
  schemaVersion: number;
  articles: number;
  projects: number;
  files: number;
  bytes: number;
  preRestoreBackup?: string | null;
}

export interface Diagnostics {
  schemaVersion: number;
  supportedSchemaVersion: number;
  integrity: string;
  foreignKeyViolations: number;
  databaseBytes: number;
  libraryFiles: number;
  libraryBytes: number;
  articles: number;
  projects: number;
  missingPdfs: number;
  missingPdfTitles: string[];
  pendingRestore: boolean;
}

export type RestoreNotice =
  | { status: "restored"; outcome: { sourceSha256: string; backupCreatedAt: string; preRestoreBackup?: string | null } }
  | { status: "failed"; message: string };

export interface ZoteroImportSummary {
  items: number;
  imported: number;
  linkedExisting: number;
  alreadyLinked: number;
  pdfsAttached: number;
  collections: number;
  errors: string[];
}

export interface ExchangeExportResult {
  path: string;
  sha256: string;
  articles: number;
  decisions: number;
  scores: number;
  values: number;
}

export interface ExchangeImportSummary {
  projectId: string;
  projectName: string;
  reviewerName: string;
  alreadyImported: boolean;
  projectCreated: boolean;
  articlesMatched: number;
  articlesCreated: number;
  decisionsAdded: number;
  scoresAdded: number;
  valuesAdded: number;
  openConflicts: number;
}

export interface ExchangeHistoryEntry {
  reviewerName: string;
  sha256: string;
  summary: ExchangeImportSummary;
  importedAt: string;
}

export const maintenance = {
  createBackup: (path: string) => invoke<BackupResult>("create_backup", { path }),
  inspectBackup: (path: string) => invoke<BackupInfo>("inspect_backup", { path }),
  stageRestore: (path: string) => invoke<BackupInfo>("stage_restore", { path }),
  cancelRestore: () => invoke<void>("cancel_restore"),
  diagnostics: () => invoke<Diagnostics>("database_diagnostics"),
  takeRestoreNotice: () => invoke<RestoreNotice | null>("take_restore_notice"),
  zoteroDefaultDir: () => invoke<string | null>("zotero_default_dir"),
  importZotero: (dataDir: string, includePdfs: boolean) => invoke<ZoteroImportSummary>("import_zotero", { dataDir, includePdfs }),
  exportExchange: (projectId: string, path: string) => invoke<ExchangeExportResult>("export_exchange_package", { projectId, path }),
  importExchange: (path: string) => invoke<ExchangeImportSummary>("import_exchange_package", { path }),
  exchangeHistory: (projectId: string) => invoke<ExchangeHistoryEntry[]>("exchange_history", { projectId }),
  resolveExtractionConflict: (conflictId: string, keepValueId: string, reason: string) =>
    invoke<void>("resolve_extraction_conflict", { conflictId, keepValueId, reason }),
};

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function exchangeSummaryText(summary: ExchangeImportSummary): string {
  if (summary.alreadyImported) return `Este paquete de ${summary.reviewerName} ya se había importado; no hubo cambios.`;
  const parts = [
    summary.projectCreated ? `Revisión «${summary.projectName}» creada` : `Revisión «${summary.projectName}» actualizada`,
    `${summary.articlesCreated} estudios nuevos y ${summary.articlesMatched} ya presentes`,
    `${summary.decisionsAdded} decisiones, ${summary.scoresAdded} puntajes y ${summary.valuesAdded} valores de ${summary.reviewerName}`,
    summary.openConflicts > 0 ? `${summary.openConflicts} conflictos abiertos` : "sin conflictos abiertos",
  ];
  return `${parts.join(" · ")}.`;
}
