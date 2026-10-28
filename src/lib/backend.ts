import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  BootstrapData,
  CreateProjectInput,
  ImportItemResult,
  LibraryArticle,
  LibraryCollection,
  ReviewProject,
  UpdateArticleMetadataInput,
} from "../types";

export const backend = {
  bootstrap: () => invoke<BootstrapData>("bootstrap_app"),
  createProject: (input: CreateProjectInput) =>
    invoke<ReviewProject>("create_review_project", { input }),
  saveSettings: (settings: AppSettings) =>
    invoke<AppSettings>("save_app_settings", { settings }),
  importPdfFiles: (paths: string[]) =>
    invoke<ImportItemResult[]>("import_pdf_files", { paths }),
  importReferenceFiles: (paths: string[]) =>
    invoke<ImportItemResult[]>("import_reference_files", { paths }),
  searchLibrary: (query: string) =>
    invoke<LibraryArticle[]>("search_library", { query }),
  updateArticleMetadata: (input: UpdateArticleMetadataInput) =>
    invoke<LibraryArticle>("update_article_metadata", { input }),
  archiveArticle: (articleId: string) =>
    invoke<void>("archive_library_article", { articleId }),
  createCollection: (name: string) =>
    invoke<LibraryCollection>("create_library_collection", { name }),
  setArticleCollection: (articleId: string, collectionId: string, included: boolean) =>
    invoke<void>("set_article_collection", { articleId, collectionId, included }),
};

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Ocurrió un error inesperado";
}
