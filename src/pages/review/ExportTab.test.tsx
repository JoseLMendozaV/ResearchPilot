// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ExportTab } from "./ExportTab";

const mocks = vi.hoisted(() => ({ save: vi.fn(), exportProject: vi.fn(), exportHistory: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: (options: unknown) => mocks.save(options) }));
vi.mock("../../lib/reviewBackend", () => ({ reviewBackend: { exportProject: mocks.exportProject, exportHistory: mocks.exportHistory } }));

afterEach(cleanup);

describe("ExportTab", () => {
  it("asks where to save and exports the compatible seven-sheet workbook", async () => {
    mocks.exportHistory.mockResolvedValue([]);
    mocks.save.mockResolvedValueOnce("/Users/me/Mapeo.xlsx");
    mocks.exportProject.mockResolvedValueOnce({ id: "e1", path: "/Users/me/Mapeo.xlsx", format: "xlsx", bytes: 8192, sha256: "abc123", summary: "Libro con 7 hojas (formato compatible)" });
    render(<ExportTab projectId="p1" projectName="Mapeo de patrimonio" />);
    fireEvent.click(screen.getByRole("button", { name: /Excel compatible \(7 hojas\)/ }));
    await waitFor(() => expect(mocks.exportProject).toHaveBeenCalledWith({ projectId: "p1", format: "xlsx", variant: "compatible", scope: undefined, path: "/Users/me/Mapeo.xlsx" }));
    expect(mocks.save).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: "Mapeo-de-patrimonio.xlsx" }));
    expect(await screen.findByText(/SHA-256 abc123/)).toBeInTheDocument();
  });

  it("does nothing when the save dialog is cancelled", async () => {
    mocks.exportHistory.mockResolvedValue([]);
    mocks.save.mockResolvedValueOnce(null);
    render(<ExportTab projectId="p1" projectName="X" />);
    fireEvent.click(screen.getByRole("button", { name: /^BibTeX/ }));
    await waitFor(() => expect(mocks.save).toHaveBeenCalled());
    expect(mocks.exportProject).not.toHaveBeenCalled();
  });
});
