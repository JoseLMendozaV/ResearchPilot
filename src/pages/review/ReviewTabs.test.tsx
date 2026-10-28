// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ExtractionMatrix, QualityTable, ScreeningTable } from "../../reviewTypes";
import { ExtractionTab } from "./ExtractionTab";
import { QualityTab } from "./QualityTab";
import { ScreeningTab } from "./ScreeningTab";

const mocks = vi.hoisted(() => ({ screening: vi.fn(), decide: vi.fn(), quality: vi.fn(), matrix: vi.fn() }));
vi.mock("../../lib/reviewBackend", () => ({ reviewBackend: mocks }));

const screening: ScreeningTable = {
  phase: "metadata",
  criteria: [
    { id: "c1", criterionType: "inclusion", code: "I1", name: "Escrito en inglés", description: "", active: true },
    { id: "c2", criterionType: "exclusion", code: "E1", name: "No aborda patrimonio", description: "", active: true },
  ],
  rows: [{
    articleId: "a1", sequence: 1, title: "Semi-supervised segmentation of murals", authors: ["Researcher, Ana"], year: 2024, keywords: ["murals"], abstractText: "Few labels.",
    stage: "identified", isSecondary: false, sourceDatabase: "Scopus", hasPdf: false,
    reviews: [{ id: "d1", reviewerName: "Revisora B", decision: "exclude", criteria: {}, reason: "", isFinal: false, decidedAt: "2026-10-28T00:00:00Z" }],
    conflictId: "k1",
  }],
  agreement: { reviewerA: "A", reviewerB: "B", compared: 4, observed: 0.75, kappa: 0.5, note: "Muestra pequeña" },
  decided: 0,
  pending: 1,
};

afterEach(cleanup);

describe("review workspace tabs", () => {
  it("records a screening decision with criterion marks and shows other reviewers and conflicts", async () => {
    mocks.screening.mockResolvedValue(screening);
    mocks.decide.mockResolvedValue({ ...screening.rows[0], finalDecision: { id: "f", reviewerName: "Yo", decision: "include", criteria: {}, reason: "", isFinal: true, decidedAt: "" } });
    render(<MemoryRouter><ScreeningTab projectId="p1" onChange={vi.fn()} /></MemoryRouter>);
    fireEvent.change(await screen.findByRole("combobox", { name: "Filtro" }), { target: { value: "all" } });
    expect(screen.getByText("Semi-supervised segmentation of murals")).toBeInTheDocument();
    expect(screen.getByText("Revisora B: Excluir")).toBeInTheDocument();
    expect(screen.getByText(/Conflicto entre revisores/)).toBeInTheDocument();
    expect(screen.getByText(/κ A\/B: 0.50/)).toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: "Criterio I1" }), { target: { value: "met" } });
    fireEvent.click(screen.getByRole("button", { name: "Incluir" }));
    await waitFor(() => expect(mocks.decide).toHaveBeenCalledWith(expect.objectContaining({ articleId: "a1", phase: "metadata", decision: "include", criteria: { c1: "met" } })));
  });

  it("shows weighted totals against the scale and the threshold result", async () => {
    const table: QualityTable = {
      criteria: [{ id: "q1", code: "Q1", question: "¿Objetivos claros?", description: "", weight: 1, scale: [{ label: "Sí", value: 1 }, { label: "No", value: 0 }], maxScore: 1 }],
      rows: [{ articleId: "a1", sequence: 1, title: "Paper", authors: [], stage: "included", reviews: [{ reviewerName: "Yo", scores: [], total: 1, maxTotal: 1, percent: 100, complete: true }], finalAssessment: { reviewerName: "Consenso", scores: [], total: 1, maxTotal: 1, percent: 100, complete: true }, meetsThreshold: true }],
      maxTotal: 1,
      threshold: 60,
      missingPolicy: "zero",
    };
    mocks.quality.mockResolvedValue(table);
    render(<QualityTab projectId="p1" onChange={vi.fn()} />);
    expect(await screen.findByText(/total máximo 1.00 · umbral 60 %/)).toBeInTheDocument();
    expect(screen.getByText("Total 1.00 / 1.00 · 100 %")).toBeInTheDocument();
    expect(screen.getByText("Cumple el umbral")).toBeInTheDocument();
  });

  it("renders the matrix grouped by General and RQ with cell status", async () => {
    const matrix: ExtractionMatrix = {
      groups: [
        { key: "general", label: "General", fields: [] },
        { key: "rq1", label: "RQ1", question: "¿Qué enfoque?", fields: [{ id: "f1", name: "Enfoque", description: "", dataType: "category", taxonomy: ["Supervisado"], instructions: "", required: false, examples: [], displayOrder: 1 }] },
      ],
      rows: [{ articleId: "a1", sequence: 1, reference: "Researcher (2024)", title: "Paper", year: 2024, stage: "included", cells: { f1: [{ id: "v1", value: "Supervisado", display: "Supervisado", status: "confirmed", notes: "", actorType: "human", actorName: "Yo", updatedAt: "", evidence: [] }] } }],
    };
    mocks.matrix.mockResolvedValue(matrix);
    render(<MemoryRouter><ExtractionTab projectId="p1" /></MemoryRouter>);
    expect(await screen.findByText("Researcher (2024)")).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "RQ1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Supervisado" })).toHaveClass("bg-emerald-50");
  });
});
