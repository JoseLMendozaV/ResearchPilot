import { invoke } from "@tauri-apps/api/core";
import type {
  AddMasterValueInput,
  AskInput,
  ConversationDetail,
  ConversationSummary,
  IndexStatus,
  OllamaStatus,
  SearchInput,
  SearchOutcome,
  AppSettings,
  ArticleExtraction,
  BootstrapData,
  CreateProjectInput,
  EnqueueResult,
  EvidenceListItem,
  ExtractorStatus,
  ImportItemResult,
  JobSummary,
  LibraryArticle,
  LibraryCollection,
  MasterValue,
  ReviewProject,
  UpdateArticleMetadataInput,
  UpdateMasterValueInput,
  ValueRevision,
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
  extractorStatus: () => invoke<ExtractorStatus>("extractor_status"),
  prepareExtractorEnvironment: () => invoke<JobSummary>("prepare_extractor_environment"),
  enqueueExtraction: (articleIds: string[], force: boolean) =>
    invoke<EnqueueResult[]>("enqueue_extraction", { articleIds, force }),
  enqueuePendingExtractions: () => invoke<EnqueueResult[]>("enqueue_pending_extractions"),
  listJobs: (limit = 200) => invoke<JobSummary[]>("list_jobs", { limit }),
  pauseJob: (jobId: string) => invoke<JobSummary>("pause_job", { jobId }),
  resumeJob: (jobId: string) => invoke<JobSummary>("resume_job", { jobId }),
  cancelJob: (jobId: string) => invoke<JobSummary>("cancel_job", { jobId }),
  retryJob: (jobId: string) => invoke<JobSummary>("retry_job", { jobId }),
  articleExtraction: (articleId: string) =>
    invoke<ArticleExtraction>("get_article_extraction", { articleId }),
  updateMasterValue: (input: UpdateMasterValueInput) =>
    invoke<MasterValue>("update_master_value", { input }),
  addMasterValue: (input: AddMasterValueInput) => invoke<MasterValue>("add_master_value", { input }),
  masterValueHistory: (valueId: string) => invoke<ValueRevision[]>("master_value_history", { valueId }),
  revertMasterValue: (valueId: string, revisionNumber: number) =>
    invoke<MasterValue>("revert_master_value", { valueId, revisionNumber }),
  recentEvidence: (limit = 200) => invoke<EvidenceListItem[]>("list_recent_evidence", { limit }),
  ollamaStatus: () => invoke<OllamaStatus>("ollama_status"),
  pullOllamaModel: (model: string) => invoke<JobSummary>("pull_ollama_model", { model }),
  indexStatus: () => invoke<IndexStatus>("index_status"),
  enqueueIndexing: (articleIds?: string[]) => invoke<EnqueueResult[]>("enqueue_indexing", { articleIds }),
  enqueueAiFields: (articleId: string) => invoke<JobSummary>("enqueue_ai_fields", { articleId }),
  searchChunks: (input: SearchInput) => invoke<SearchOutcome>("search_chunks", { input }),
  askAssistant: (input: AskInput) => invoke<ConversationDetail>("ask_assistant", { input }),
  cancelAssistant: (requestId: string) => invoke<void>("cancel_assistant", { requestId }),
  listConversations: () => invoke<ConversationSummary[]>("list_conversations"),
  getConversation: (conversationId: string) => invoke<ConversationDetail>("get_conversation", { conversationId }),
  deleteConversation: (conversationId: string) => invoke<void>("delete_conversation", { conversationId }),
};

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Ocurrió un error inesperado";
}
