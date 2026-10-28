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
  autoExtractOnImport: boolean;
  ocrEnabled: boolean;
  ocrLanguages: string;
  grobidEnabled: boolean;
  grobidUrl: string;
  extractionTimeoutSeconds: number;
  extractorPythonPath: string;
  ollamaEmbeddingModel: string;
  ollamaExtractionModel: string;
  autoIndexAfterExtraction: boolean;
  ragMode: "hybrid" | "lexical" | "semantic";
  ragTopK: number;
  ragContextTokens: number;
  aiTimeoutSeconds: number;
  openaiModel: string;
  anthropicModel: string;
  externalMonthlyBudgetUsd: number;
  externalMaxOutputTokens: number;
  checkUpdatesOnStart: boolean;
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
  activeJobs: number;
  valuesToReview: number;
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
  confidential: boolean;
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

export type JobStatus = "queued" | "running" | "paused" | "completed" | "failed" | "cancelled";

export interface JobSummary {
  id: string;
  jobType: "scientific_extraction" | "extractor_setup" | string;
  status: JobStatus;
  progress: number;
  stage?: string;
  message?: string;
  errorMessage?: string;
  articleId?: string;
  articleTitle?: string;
  attempts: number;
  maxAttempts: number;
  createdAt: string;
  updatedAt: string;
  startedAt?: string;
  finishedAt?: string;
}

export interface EnqueueResult {
  articleId: string;
  status: "queued" | "already_queued" | "cached" | "no_document";
  jobId?: string;
  message?: string;
}

export interface ExtractorCapabilities {
  workerVersion: string;
  protocolVersion: number;
  rulesetVersion: string;
  python: string;
  pdfplumber: string | null;
  pypdfium2: boolean;
  tesseract: { available: boolean; version: string | null; languages: string[] };
  grobid?: { url: string; reachable: boolean };
}

export interface ExtractorStatus {
  pythonPath?: string;
  pythonSource?: "override" | "app_environment" | "development" | "system";
  workerRoot: string;
  workerPresent: boolean;
  appEnvironmentReady: boolean;
  capabilities?: ExtractorCapabilities;
  message?: string;
}

export type ValueStatus =
  | "confirmed"
  | "proposed"
  | "not_reported"
  | "not_found"
  | "not_applicable"
  | "needs_review"
  | "rejected";

export type FieldGroup = "bibliography" | "context" | "design_methods" | "evaluation_results" | "ethics_reproducibility";

export interface MasterField {
  fieldKey: string;
  groupKey: FieldGroup;
  labelEs: string;
  labelEn: string;
  dataType: "text" | "number" | "list" | "url" | "date" | "metric";
  multiValued: boolean;
}

export interface EvidenceItem {
  id: string;
  quote: string;
  page?: number;
  bbox?: number[];
  pageWidth?: number;
  pageHeight?: number;
  section?: string;
  relation: "supports" | "contradicts";
  verified?: boolean;
}

export interface MasterValue {
  id: string;
  fieldKey: string;
  value: unknown;
  displayValue: string;
  valueType: string;
  unit?: string;
  origin: "rules" | "ocr" | "grobid" | "ai" | "human" | "import";
  method?: string;
  status: ValueStatus;
  notes: string;
  revisionNumber: number;
  reviewerName?: string;
  reviewedAt?: string;
  extractionRunId?: string;
  provider?: string;
  model?: string;
  promptVersion?: string;
  conflict: boolean;
  createdAt: string;
  updatedAt: string;
  evidence: EvidenceItem[];
}

export interface ValueRevision {
  revisionNumber: number;
  value: unknown;
  displayValue: string;
  status: ValueStatus;
  notes: string;
  actorType: "human" | "ai" | "system" | "import";
  actorName: string;
  reason: string;
  createdAt: string;
}

export interface ExtractionWarning {
  code: string;
  message: string;
  page?: number;
}

export interface ExtractionRunSummary {
  id: string;
  status: "running" | "completed" | "failed" | "cancelled";
  provider: string;
  model?: string;
  promptVersion: string;
  extractorVersion?: string;
  engine?: string;
  sourceDocumentHash: string;
  startedAt: string;
  completedAt?: string;
  errorMessage?: string;
  stats: Record<string, unknown> | null;
  warnings: ExtractionWarning[] | null;
}

export interface TableFigure {
  id: string;
  objectType: "table" | "figure";
  label?: string;
  caption?: string;
  pageNumber?: number;
  bbox?: number[];
  content: { rows?: string[][]; source?: string } | null;
}

export interface ReferenceEntry {
  id: string;
  ordinal?: number;
  rawReference: string;
  doi?: string;
  pageNumber?: number;
  matchedArticleId?: string;
}

export interface SectionEntry {
  title: string;
  canonical?: string;
  pageNumber: number;
  bbox?: number[];
}

export interface ArticleExtraction {
  fields: MasterField[];
  values: MasterValue[];
  runs: ExtractionRunSummary[];
  activeJob?: JobSummary;
  tablesFigures: TableFigure[];
  references: ReferenceEntry[];
  sections: SectionEntry[];
  pages: { total: number; extracted: number; ocr: number; withoutText: number };
}

export interface UpdateMasterValueInput {
  valueId: string;
  valueText?: string;
  status: Exclude<ValueStatus, "proposed" | "not_found">;
  notes: string;
  reason: string;
}

export interface AddMasterValueInput {
  articleId: string;
  fieldKey: string;
  valueText?: string;
  status: Exclude<ValueStatus, "proposed" | "not_found" | "rejected">;
  quote?: string;
  page?: number;
  notes: string;
}

export interface EvidenceListItem {
  id: string;
  articleId: string;
  articleTitle: string;
  valueId: string;
  fieldKey: string;
  fieldLabel: string;
  valueStatus: ValueStatus;
  displayValue: string;
  quote: string;
  page?: number;
  createdAt: string;
}

/** Where the PDF viewer should navigate and what to highlight. */
export interface ViewerFocus {
  page: number;
  bbox?: number[];
  pageWidth?: number;
  key: number;
}

export interface OllamaModel {
  name: string;
  sizeBytes: number;
  family?: string;
  parameterSize?: string;
  quantization?: string;
  capabilities: string[];
  kind: "chat" | "embedding" | "unknown";
  contextLength?: number;
  memoryFit?: "comfortable" | "tight" | "too_large";
}

export interface OllamaStatus {
  baseUrl: string;
  reachable: boolean;
  version?: string;
  models: OllamaModel[];
  systemMemoryBytes?: number;
  recommendation: string;
  message?: string;
}

export interface IndexStatus {
  articles: number;
  indexedArticles: number;
  chunks: number;
  embeddedChunks: number;
  embeddingModel: string;
}

export type ScopeType = "library" | "collection" | "article";

export interface RetrievedChunk {
  chunkId: string;
  articleId: string;
  articleTitle: string;
  publicationYear?: number;
  page: number;
  section?: string;
  text: string;
  pageWidth?: number;
  score: number;
  lexicalRank?: number;
  semanticRank?: number;
}

export interface SearchOutcome {
  chunks: RetrievedChunk[];
  mode: "hybrid" | "lexical" | "semantic";
  warnings: string[];
}

export interface SearchInput {
  query: string;
  scopeType: ScopeType;
  scopeId?: string;
  yearFrom?: number;
  yearTo?: number;
  mode?: AppSettings["ragMode"];
}

export interface VerifiedCitation {
  source: string;
  quote: string;
  articleId: string;
  articleTitle: string;
  page: number;
  section?: string;
  bbox?: number[];
  pageWidth?: number;
}

export interface AssistantMessage {
  id: string;
  conversationId: string;
  role: "user" | "assistant";
  content: string;
  status: "question" | "answered" | "not_found" | "unsupported" | "failed";
  citations: VerifiedCitation[];
  sources: { mode?: string; warnings?: string[]; chunks?: { source: string; articleId: string; articleTitle: string; page: number; section?: string }[] } | null;
  aiRunId?: string;
  createdAt: string;
}

export interface ConversationSummary {
  id: string;
  title: string;
  scopeType: ScopeType;
  scopeId?: string;
  scopeLabel?: string;
  messageCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface ConversationDetail {
  conversation: ConversationSummary;
  messages: AssistantMessage[];
}

export interface AskInput {
  requestId: string;
  conversationId?: string;
  scopeType: ScopeType;
  scopeId?: string;
  yearFrom?: number;
  yearTo?: number;
  question: string;
  model?: string;
  provider?: AiProvider;
  consent?: ExternalConsent;
}

export type AiProvider = "ollama" | "openai" | "anthropic";

export interface ExternalConsent {
  provider: Exclude<AiProvider, "ollama">;
  model: string;
  maxCostUsd?: number;
}

export interface ExternalEstimate {
  provider: Exclude<AiProvider, "ollama">;
  model: string;
  requests: number;
  inputTokens: number;
  expectedOutputTokens: number;
  maxOutputTokens: number;
  pricingKnown: boolean;
  expectedCostUsd?: number;
  worstCaseCostUsd?: number;
  monthSpentUsd: number;
  monthlyBudgetUsd: number;
  withinBudget: boolean;
  articles: { id: string; title: string }[];
  excludedConfidential: number;
  excerpts: number;
}

export interface ProviderStatus {
  provider: "openai" | "anthropic";
  hasKey: boolean;
  keyHint?: string;
}

export interface ModelPrice {
  provider: "openai" | "anthropic";
  model: string;
  inputUsdPerMtok: number;
  outputUsdPerMtok: number;
  source: "reference" | "user";
  updatedAt: string;
}

export interface ExternalLog {
  totalRequests: number;
  monthSpentUsd: number;
  entries: { createdAt: string; provider: string; model: string; operation: string; status: string; promptTokens?: number; completionTokens?: number; costUsd?: number; articleTitle?: string }[];
}
