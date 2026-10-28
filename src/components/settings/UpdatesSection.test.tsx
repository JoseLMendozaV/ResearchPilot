// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UpdatesSection } from "./UpdatesSection";

const mocks = vi.hoisted(() => ({ check: vi.fn(), install: vi.fn(), restart: vi.fn() }));
vi.mock("../../lib/updates", () => ({
  checkForUpdate: () => mocks.check(),
  installUpdate: (update: unknown, onProgress: (value: number) => void) => mocks.install(update, onProgress),
  currentVersion: () => Promise.resolve("0.6.1"),
  restartApp: () => mocks.restart(),
}));

afterEach(cleanup);

describe("UpdatesSection", () => {
  it("installs a signed update only after the user asks and then offers a restart", async () => {
    const update = { version: "0.7.0", body: "Exportación a Excel", date: "2026-11-01T00:00:00Z" };
    mocks.check.mockResolvedValueOnce(update);
    mocks.install.mockImplementationOnce(async (_update, onProgress) => onProgress(1));
    render(<UpdatesSection checkOnStart={false} onCheckOnStartChange={vi.fn()} />);
    expect(await screen.findByText("0.6.1")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Buscar actualizaciones/ }));
    expect(await screen.findByText(/ResearchPilot 0.7.0 disponible/)).toBeInTheDocument();
    expect(screen.getByText("Exportación a Excel")).toBeInTheDocument();
    expect(mocks.install).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /Descargar e instalar/ }));
    fireEvent.click(await screen.findByRole("button", { name: /Reiniciar para terminar/ }));
    expect(mocks.restart).toHaveBeenCalled();
  });

  it("reports when the installed version is the latest", async () => {
    mocks.check.mockResolvedValueOnce(null);
    render(<UpdatesSection checkOnStart onCheckOnStartChange={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Buscar actualizaciones/ }));
    expect(await screen.findByText("Tienes la versión más reciente publicada.")).toBeInTheDocument();
  });
});
