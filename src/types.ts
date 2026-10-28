export type ReviewType =
  | "systematic_review"
  | "systematic_mapping"
  | "scoping_review";

export interface AppSettings {
  researcherName: string;
  defaultLanguage: "es" | "en";
  appearance: "light" | "dark" | "system";
  aiProvider: "ollama" | "openai" | "anthropic";
  localOnly: boolean;
  allowExternalProcessing: boolean;
  ollamaBaseUrl: string;
  ollamaModel: string;
}

export interface ReviewProject {
  id: string;
  name: string;
  description: string;
  reviewType: ReviewType;
  status: "draft" | "active" | "completed" | "archived";
  createdAt: string;
  updatedAt: string;
}

export interface LibraryCollection {
  id: string;
  name: string;
  description: string;
  articleCount: number;
  createdAt: string;
}

export interface DashboardStats {
  projects: number;
  articles: number;
  researchQuestions: number;
  evidenceSpans: number;
}

export interface BootstrapData {
  settings: AppSettings;
  projects: ReviewProject[];
  stats: DashboardStats;
  databasePath: string;
  articles: LibraryArticle[];
  collections: LibraryCollection[];
}

export interface LibraryArticle {
  id: string;
  title: string;
  abstractText?: string;
  publicationYear?: number;
  doi?: string;
  authors: string[];
  keywords: string[];
  venue?: string;
  documentPath?: string;
  fileName?: string;
  documentSha256?: string;
  byteSize?: number;
  pageCount: number;
  importedAt: string;
  updatedAt: string;
  collectionIds: string[];
}

export interface ImportItemResult {
  sourcePath: string;
  status: "imported" | "duplicate" | "error";
  articleId?: string;
  title?: string;
  message?: string;
}

export interface UpdateArticleMetadataInput {
  id: string;
  title: string;
  abstractText?: string;
  publicationYear?: number;
  doi?: string;
  authors: string[];
  keywords: string[];
  venue?: string;
}

export interface CreateProjectInput {
  name: string;
  description: string;
  reviewType: ReviewType;
}
