// Contracts of the systematic-review workspace (Phase 6), mirroring the Rust serde types.

export type Phase = "metadata" | "full_text";
export type Decision = "include" | "exclude" | "maybe" | "awaiting_full_text";
export type Stage = "identified" | "screening" | "eligible" | "included" | "excluded";

export interface ProjectDetail {
  id: string;
  name: string;
  description: string;
  reviewType: "systematic_review" | "systematic_mapping" | "scoping_review";
  status: "draft" | "active" | "completed" | "archived";
  screeningMode: "single" | "double";
  qualityThreshold?: number;
  qualityMissingPolicy: "zero" | "exclude_from_max";
  createdAt: string;
  updatedAt: string;
}

export interface ReviewerInfo {
  id: string;
  displayName: string;
  role: "owner" | "reviewer" | "resolver";
  isLocalProfile: boolean;
}

export interface ProtocolVersion {
  id: string;
  versionNumber: number;
  status: "draft" | "approved" | "superseded";
  content: Record<string, unknown>;
  changeSummary: string;
  createdBy: string;
  approvedAt?: string;
  createdAt: string;
  amendments: { reason: string; change: { changedSections?: string[] }; createdBy: string; createdAt: string }[];
}

export interface StageCounts {
  total: number;
  identified: number;
  screening: number;
  eligible: number;
  included: number;
  excluded: number;
  secondary: number;
}

export interface ProjectWorkspace {
  project: ProjectDetail;
  reviewers: ReviewerInfo[];
  activeReviewerId: string;
  protocol: ProtocolVersion[];
  counts: StageCounts;
  openConflicts: number;
}

export interface ProjectSettingsInput {
  id: string;
  name: string;
  description: string;
  reviewType: ProjectDetail["reviewType"];
  status: ProjectDetail["status"];
  screeningMode: ProjectDetail["screeningMode"];
  qualityThreshold?: number;
  qualityMissingPolicy: ProjectDetail["qualityMissingPolicy"];
}

export interface ControlArticle {
  id: string;
  articleId: string;
  title: string;
  doi?: string;
  year?: number;
  rationale: string;
  expectedRelevant: boolean;
  inCorpus: boolean;
  foundBy: string[];
}

export interface ControlCoverage {
  articles: ControlArticle[];
  found: number;
  total: number;
}

export type FieldType = "text" | "number" | "boolean" | "date" | "category" | "list";

export interface ExtractionField {
  id: string;
  researchQuestionId?: string;
  name: string;
  description: string;
  dataType: FieldType;
  unit?: string;
  taxonomy: string[];
  instructions: string;
  required: boolean;
  examples: string[];
  masterFieldKey?: string;
  displayOrder: number;
}

export interface ResearchQuestion {
  id: string;
  parentId?: string;
  code: string;
  question: string;
  objective: string;
  category: string;
  displayOrder: number;
  fields: ExtractionField[];
}

export interface QuestionsOverview {
  questions: ResearchQuestion[];
  generalFields: ExtractionField[];
}

export interface QuestionInput {
  id?: string;
  projectId: string;
  parentId?: string;
  code?: string;
  question: string;
  objective: string;
  category: string;
}

export interface FieldInput {
  id?: string;
  projectId: string;
  researchQuestionId?: string;
  name: string;
  description: string;
  dataType: FieldType;
  unit?: string;
  taxonomy: string[];
  instructions: string;
  required: boolean;
  examples: string[];
  masterFieldKey?: string;
}

export interface SearchStrategy {
  id: string;
  databaseName: string;
  queryText: string;
  queryDate: string;
  resultCount: number;
  notes: string;
  versionNumber: number;
  filters: Record<string, unknown>;
  language?: string;
  periodStart?: number;
  periodEnd?: number;
  recordsImported: number;
  imports: { sourcePath: string; sourceSha256: string; importedCount: number; duplicateCount: number; linkedCount: number; importedAt: string }[];
}

export interface StrategyInput {
  id?: string;
  projectId: string;
  databaseName: string;
  queryText: string;
  queryDate: string;
  resultCount: number;
  notes: string;
  filters: Record<string, unknown>;
  language?: string;
  periodStart?: number;
  periodEnd?: number;
  newVersion?: boolean;
}

export interface StrategyImportResult {
  records: number;
  newArticles: number;
  duplicates: number;
  errors: number;
  addedToCorpus: number;
  fileAlreadyImported: boolean;
}

export interface Criterion {
  id: string;
  criterionType: "inclusion" | "exclusion";
  code: string;
  name: string;
  description: string;
  active: boolean;
}

export interface CriterionInput {
  id?: string;
  projectId: string;
  criterionType: Criterion["criterionType"];
  code: string;
  name: string;
  description: string;
}

export type CriterionMark = "met" | "not_met" | "unclear";

export interface DecisionInfo {
  id: string;
  reviewerId?: string;
  reviewerName: string;
  decision: Decision;
  criterionId?: string;
  criteria: Record<string, CriterionMark>;
  reason: string;
  isFinal: boolean;
  decidedAt: string;
}

export interface ScreeningRow {
  articleId: string;
  sequence: number;
  title: string;
  authors: string[];
  year?: number;
  venue?: string;
  doi?: string;
  keywords: string[];
  abstractText?: string;
  stage: Stage;
  isSecondary: boolean;
  sourceDatabase?: string;
  hasPdf: boolean;
  reviews: DecisionInfo[];
  finalDecision?: DecisionInfo;
  conflictId?: string;
}

export interface ScreeningTable {
  phase: Phase;
  criteria: Criterion[];
  rows: ScreeningRow[];
  agreement?: { reviewerA: string; reviewerB: string; compared: number; observed: number; kappa?: number; note: string };
  decided: number;
  pending: number;
}

export interface DecisionInput {
  projectId: string;
  articleId: string;
  phase: Phase;
  decision: Decision;
  criterionId?: string;
  criteria: Record<string, CriterionMark>;
  reason: string;
}

export interface ConflictInfo {
  id: string;
  articleId: string;
  sequence?: number;
  title: string;
  conflictType: "screening" | "quality" | "extraction";
  phase?: string;
  status: "open" | "resolved";
  details: {
    decisions?: { reviewer: string; decision: Decision; reason: string }[];
    criteria?: string[];
    reviewers?: string[];
    // Extraction conflicts: the differing values of one field.
    fieldId?: string;
    fieldName?: string;
    values?: { valueId: string; reviewer: string; value: unknown }[];
  };
  resolution?: string;
  createdAt: string;
  resolvedAt?: string;
}

export interface ScaleOption {
  label: string;
  value: number;
}

export interface QualityCriterion {
  id: string;
  code: string;
  question: string;
  description: string;
  weight: number;
  scale: ScaleOption[];
  maxScore: number;
}

export interface QualityCriterionInput {
  id?: string;
  projectId: string;
  code?: string;
  question: string;
  description: string;
  weight: number;
  scale: ScaleOption[];
}

export interface ScoreEntry {
  criterionId: string;
  score?: number | null;
  justification: string;
}

export interface Assessment {
  reviewerName: string;
  scores: ScoreEntry[];
  total: number;
  maxTotal: number;
  percent?: number;
  complete: boolean;
}

export interface QualityRow {
  articleId: string;
  sequence: number;
  title: string;
  authors: string[];
  year?: number;
  venue?: string;
  stage: Stage;
  reviews: Assessment[];
  finalAssessment?: Assessment;
  meetsThreshold?: boolean;
  conflictId?: string;
}

export interface QualityTable {
  criteria: QualityCriterion[];
  rows: QualityRow[];
  maxTotal: number;
  threshold?: number;
  missingPolicy: "zero" | "exclude_from_max";
}

export interface CellValue {
  id: string;
  value: unknown;
  display: string;
  status: "confirmed" | "proposed" | "not_reported" | "not_found" | "not_applicable" | "needs_review" | "rejected";
  notes: string;
  actorType: string;
  actorName: string;
  updatedAt: string;
  evidence: { id: string; quote: string; page?: number; bbox?: number[]; pageWidth?: number }[];
}

export interface ExtractionMatrix {
  groups: { key: string; label: string; question?: string; fields: ExtractionField[] }[];
  rows: { articleId: string; sequence: number; reference: string; title: string; year?: number; stage: Stage; cells: Record<string, CellValue[]> }[];
}

export interface CellInput {
  projectId: string;
  articleId: string;
  fieldId: string;
  valueId?: string;
  valueText?: string;
  status: CellValue["status"];
  notes: string;
  evidenceId?: string;
  quote?: string;
  page?: number;
}

export interface AuditEntry {
  entityType: string;
  entityId?: string;
  action: string;
  actorType: string;
  actorName: string;
  before?: unknown;
  after?: unknown;
  createdAt: string;
}

export type ExportFormat = "xlsx" | "csv" | "json" | "ris" | "bibtex" | "markdown" | "latex" | "prisma";

export interface ExportRequest {
  projectId: string;
  format: ExportFormat;
  variant?: "compatible" | "extended";
  scope?: "included" | "corpus";
  path: string;
}

export interface ExportResult {
  id: string;
  path: string;
  format: ExportFormat;
  bytes: number;
  sha256: string;
  summary: string;
}

export interface ExportRecord {
  id: string;
  format: ExportFormat;
  outputPath: string;
  outputSha256?: string;
  options: { variant?: string; scope?: string; appVersion?: string; protocolVersion?: number };
  createdAt: string;
}

export interface PrismaCounts {
  databases: { source: string; records: number }[];
  recordsFromDatabases: number;
  recordsFromOtherSources: number;
  duplicatesRemoved: number;
  removedOtherReasons: number;
  recordsScreened: number;
  recordsExcluded: number;
  screeningPending: number;
  reportsSought: number;
  reportsNotRetrieved: number;
  reportsAssessed: number;
  reportsExcluded: number;
  exclusionReasons: { reason: string; reports: number }[];
  fullTextPending: number;
  studiesIncluded: number;
  secondaryStudies: number;
  issues: string[];
  complete: boolean;
}

export interface Frequency {
  value: string;
  count: number;
  studies: number[];
}

export interface FieldSynthesis {
  fieldId: string;
  group: string;
  name: string;
  dataType: string;
  studiesWithData: number;
  unverifiedValues: number;
  frequencies: Frequency[];
  numeric: { unit: string; n: number; min: number; max: number; median: number; values: [number, number][] }[];
  warnings: string[];
}

export interface Synthesis {
  included: number;
  byYear: Frequency[];
  byVenue: Frequency[];
  bySource: Frequency[];
  fields: FieldSynthesis[];
}

export type DraftSection = "methods" | "results" | "limitations";

export interface Draft {
  section: DraftSection;
  content: string;
  generatedContent: string;
  edited: boolean;
  generatedAt: string;
  updatedAt: string;
}
