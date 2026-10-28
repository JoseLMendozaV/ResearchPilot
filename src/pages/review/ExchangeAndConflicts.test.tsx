// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConflictsTab } from "./ConflictsTab";
import { ExchangeCard } from "./ExchangeCard";

const mocks = vi.hoisted(() => ({
  save: vi.fn(),
  open: vi.fn(),
  exportExchange: vi.fn(),
  importExchange: vi.fn(),
  exchangeHistory: vi.fn(),
  resolveExtractionConflict: vi.fn(),
  conflicts: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: (o: unknown) => mocks.save(o), open: (o: unknown) => mocks.open(o) }));
vi.mock("../../lib/maintenance", async (original) => ({
  ...(await original<typeof import("../../lib/maintenance")>()),
  maintenance: {
    exportExchange: mocks.exportExchange,
    importExchange: mocks.importExchange,
    exchangeHistory: mocks.exchangeHistory,
    resolveExtractionConflict: mocks.resolveExtractionConflict,
  },
}));
vi.mock("../../lib/reviewBackend", () => ({ reviewBackend: { conflicts: mocks.conflicts } }));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ExchangeCard", () => {
  it("exports a package and imports the colleague's one", async () => {
    mocks.exchangeHistory.mockResolvedValue([]);
    mocks.save.mockResolvedValueOnce("/Users/me/Mapeo-intercambio.json");
    mocks.exportExchange.mockResolvedValueOnce({ path: "/Users/me/Mapeo-intercambio.json", sha256: "beef", articles: 3, decisions: 3, scores: 0, values: 1 });
    const onImported = vi.fn().mockResolvedValue(undefined);
    render(<ExchangeCard projectId="p1" projectName="Mapeo" onImported={onImported} />);
    fireEvent.click(screen.getByRole("button", { name: /Exportar paquete/ }));
    await waitFor(() => expect(mocks.exportExchange).toHaveBeenCalledWith("p1", "/Users/me/Mapeo-intercambio.json"));
    expect(mocks.save).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: "Mapeo-intercambio.json" }));
    expect(await screen.findByText(/3 estudios · 3 decisiones/)).toBeInTheDocument();

    mocks.open.mockResolvedValueOnce("/Users/me/bruno.json");
    mocks.importExchange.mockResolvedValueOnce({ projectId: "p1", projectName: "Mapeo", reviewerName: "Bruno", alreadyImported: false, projectCreated: false, articlesMatched: 3, articlesCreated: 0, decisionsAdded: 3, scoresAdded: 0, valuesAdded: 1, openConflicts: 2 });
    fireEvent.click(screen.getByRole("button", { name: /Importar paquete/ }));
    expect(await screen.findByText(/3 decisiones, 0 puntajes y 1 valores de Bruno · 2 conflictos abiertos/)).toBeInTheDocument();
    expect(onImported).toHaveBeenCalled();
  });
});

describe("ConflictsTab extraction conflicts", () => {
  it("keeps the chosen value with a reason", async () => {
    mocks.conflicts.mockResolvedValue([
      { id: "c1", articleId: "a1", sequence: 4, title: "Murales", conflictType: "extraction", status: "open", createdAt: "2026-10-28T09:00:00Z", details: { fieldId: "f1", fieldName: "Enfoque", values: [{ valueId: "v1", reviewer: "Ana", value: "Supervisado" }, { valueId: "v2", reviewer: "Bruno", value: "Semisupervisado" }] } },
    ]);
    mocks.resolveExtractionConflict.mockResolvedValueOnce(null);
    render(<ConflictsTab projectId="p1" onChange={vi.fn()} onOpenQuality={vi.fn()} />);
    expect(await screen.findByText(/Extracción · Enfoque/)).toBeInTheDocument();
    const keep = screen.getByRole("button", { name: "Conservar valor" });
    expect(keep).toBeDisabled();
    fireEvent.click(screen.getByLabelText(/Bruno: Semisupervisado/));
    fireEvent.change(screen.getByPlaceholderText("Motivo de la resolución"), { target: { value: "Usa pocas etiquetas" } });
    fireEvent.click(keep);
    await waitFor(() => expect(mocks.resolveExtractionConflict).toHaveBeenCalledWith("c1", "v2", "Usa pocas etiquetas"));
    await waitFor(() => expect(mocks.conflicts).toHaveBeenCalledTimes(2));
  });
});
