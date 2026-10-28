// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PrismaCounts } from "../../reviewTypes";
import { SynthesisTab } from "./SynthesisTab";

const mocks = vi.hoisted(() => ({ prismaCounts: vi.fn(), prismaSvg: vi.fn(), synthesis: vi.fn(), drafts: vi.fn(), generateDraft: vi.fn(), saveDraft: vi.fn(), exportProject: vi.fn() }));
vi.mock("../../lib/reviewBackend", () => ({ reviewBackend: mocks }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const counts: PrismaCounts = {
  databases: [{ source: "Scopus", records: 3 }], recordsFromDatabases: 3, recordsFromOtherSources: 0, duplicatesRemoved: 0, removedOtherReasons: 0,
  recordsScreened: 3, recordsExcluded: 1, screeningPending: 1, reportsSought: 1, reportsNotRetrieved: 0, reportsAssessed: 1, reportsExcluded: 0,
  exclusionReasons: [], fullTextPending: 0, studiesIncluded: 1, secondaryStudies: 0, issues: [], complete: false,
};

afterEach(cleanup);

describe("SynthesisTab", () => {
  it("shows the PRISMA diagram as provisional, charts with a table view and asks before regenerating an edited draft", async () => {
    mocks.prismaCounts.mockResolvedValue(counts);
    mocks.prismaSvg.mockResolvedValue('<svg aria-label="Diagrama de flujo PRISMA 2020"><text>Registros cribados</text></svg>');
    mocks.synthesis.mockResolvedValue({ included: 1, byYear: [{ value: "2024", count: 1, studies: [1] }], byVenue: [], bySource: [{ value: "Scopus", count: 1, studies: [1] }], fields: [] });
    mocks.drafts.mockResolvedValue([{ section: "results", content: "Mi texto", generatedContent: "Generado", edited: true, generatedAt: "2026-10-28T00:00:00Z", updatedAt: "2026-10-28T00:00:00Z" }]);
    mocks.generateDraft.mockResolvedValue([]);
    render(<SynthesisTab projectId="p1" projectName="Mapeo" />);
    expect(await screen.findByText("Registros cribados")).toBeInTheDocument();
    expect(screen.getByText(/Provisional: 1 registro\(s\) sin decisión/)).toBeInTheDocument();
    expect(await screen.findByRole("img", { name: "Estudios por año: 2024 1" })).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "Ver tabla" })[0]);
    expect(screen.getByRole("cell", { name: "Id 1" })).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: /Regenerar$/ }));
    expect(mocks.generateDraft).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Regenerar igualmente" }));
    await waitFor(() => expect(mocks.generateDraft).toHaveBeenCalledWith("p1", "results"));
  });
});
