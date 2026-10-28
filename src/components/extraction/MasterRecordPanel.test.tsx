// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ArticleExtraction, MasterValue } from "../../types";
import { MasterRecordPanel } from "./MasterRecordPanel";

const updateMasterValue = vi.fn();
vi.mock("../../lib/backend", () => ({
  backend: { updateMasterValue: (input: unknown) => updateMasterValue(input), masterValueHistory: vi.fn().mockResolvedValue([]) },
  errorMessage: (error: unknown) => String(error),
}));

const doi: MasterValue = {
  id: "value-1",
  fieldKey: "bibliography.doi",
  value: "10.1234/rp",
  displayValue: "10.1234/rp",
  valueType: "text",
  origin: "rules",
  method: "rules:doi-regex",
  status: "proposed",
  notes: "",
  revisionNumber: 1,
  conflict: false,
  createdAt: "2026-10-28T00:00:00Z",
  updatedAt: "2026-10-28T00:00:00Z",
  evidence: [{ id: "evidence-1", quote: "https://doi.org/10.1234/rp", page: 2, bbox: [72, 100, 200, 110], pageWidth: 595, relation: "supports", verified: true }],
};

const extraction: ArticleExtraction = {
  fields: [
    { fieldKey: "bibliography.doi", groupKey: "bibliography", labelEs: "DOI", labelEn: "DOI", dataType: "text", multiValued: false },
    { fieldKey: "ethics_reproducibility.ethics_approval", groupKey: "ethics_reproducibility", labelEs: "Aprobación ética", labelEn: "Ethics approval", dataType: "text", multiValued: true },
  ],
  values: [
    doi,
    { ...doi, id: "value-2", fieldKey: "ethics_reproducibility.ethics_approval", value: null, displayValue: "", status: "not_found", evidence: [] },
  ],
  runs: [{ id: "run-1", status: "completed", provider: "researchpilot-extractor", model: "pdfplumber", promptVersion: "rules-v1", extractorVersion: "0.3.0", sourceDocumentHash: "abc", startedAt: "2026-10-28T00:00:00Z", completedAt: "2026-10-28T00:01:00Z", stats: {}, warnings: [{ code: "ocr_review", message: "Se aplicó OCR local" }] }],
  tablesFigures: [],
  references: [],
  sections: [],
  pages: { total: 3, extracted: 3, ocr: 0, withoutText: 0 },
};

function renderPanel(overrides: Partial<React.ComponentProps<typeof MasterRecordPanel>> = {}) {
  const props = {
    articleId: "article-1",
    extraction,
    onExtract: vi.fn().mockResolvedValue(undefined),
    onJobAction: vi.fn().mockResolvedValue(undefined),
    onValueChanged: vi.fn(),
    onShowEvidence: vi.fn(),
    ...overrides,
  };
  render(<MasterRecordPanel {...props} />);
  return props;
}

afterEach(cleanup);

describe("MasterRecordPanel", () => {
  it("shows proposals with literal evidence and navigates to the page", () => {
    const props = renderPanel();
    expect(screen.getByText("Ficha generada")).toBeInTheDocument();
    expect(screen.getByText("Se aplicó OCR local")).toBeInTheDocument();
    expect(screen.getByText("Propuesto")).toBeInTheDocument();
    expect(screen.getByText("“https://doi.org/10.1234/rp”")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Ver en página 2/ }));
    expect(props.onShowEvidence).toHaveBeenCalledWith(doi.evidence[0]);
  });

  it("hides not-found fields by default and explains them when shown", () => {
    renderPanel();
    expect(screen.queryByText("Aprobación ética")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "Todos los campos" }));
    expect(screen.getByText("Aprobación ética")).toBeInTheDocument();
    expect(screen.getByText(/no confirma que el artículo no lo reporte/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "No reportado" })).toBeEnabled();
  });

  it("confirms a proposal through the backend", async () => {
    const confirmed = { ...doi, status: "confirmed" as const, revisionNumber: 2 };
    updateMasterValue.mockResolvedValueOnce(confirmed);
    const props = renderPanel();
    fireEvent.click(screen.getByRole("button", { name: /Confirmar/ }));
    await waitFor(() => expect(props.onValueChanged).toHaveBeenCalledWith(confirmed));
    expect(updateMasterValue).toHaveBeenCalledWith(expect.objectContaining({ valueId: "value-1", status: "confirmed" }));
  });

  it("offers extraction when the article has no record yet", () => {
    const props = renderPanel({ extraction: { ...extraction, values: [], runs: [] } });
    fireEvent.click(screen.getByRole("button", { name: /Extraer ahora/ }));
    expect(props.onExtract).toHaveBeenCalledWith(false);
  });
});
