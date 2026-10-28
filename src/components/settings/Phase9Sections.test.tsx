// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { BackupSection } from "./BackupSection";
import { ZoteroSection } from "./ZoteroSection";

const mocks = vi.hoisted(() => ({
  save: vi.fn(),
  open: vi.fn(),
  relaunch: vi.fn(),
  createBackup: vi.fn(),
  inspectBackup: vi.fn(),
  stageRestore: vi.fn(),
  cancelRestore: vi.fn(),
  diagnostics: vi.fn(),
  zoteroDefaultDir: vi.fn(),
  importZotero: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: (o: unknown) => mocks.save(o), open: (o: unknown) => mocks.open(o) }));
vi.mock("../../lib/updates", () => ({ restartApp: () => mocks.relaunch() }));
vi.mock("../../lib/maintenance", async (original) => ({
  ...(await original<typeof import("../../lib/maintenance")>()),
  maintenance: {
    createBackup: mocks.createBackup,
    inspectBackup: mocks.inspectBackup,
    stageRestore: mocks.stageRestore,
    cancelRestore: mocks.cancelRestore,
    diagnostics: mocks.diagnostics,
    zoteroDefaultDir: mocks.zoteroDefaultDir,
    importZotero: mocks.importZotero,
  },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const info = { createdAt: "2026-10-20T10:00:00Z", appVersion: "0.9.0", schemaVersion: 11, articles: 12, projects: 2, files: 13, bytes: 4_194_304 };

describe("BackupSection", () => {
  it("creates a backup where the user chooses and shows its hash", async () => {
    mocks.save.mockResolvedValueOnce("/Users/me/respaldo.zip");
    mocks.createBackup.mockResolvedValueOnce({ path: "/Users/me/respaldo.zip", sha256: "feed", bytes: 2048, files: 3, createdAt: "2026-10-28T09:00:00Z" });
    render(<BackupSection />);
    fireEvent.click(screen.getByRole("button", { name: /Crear respaldo/ }));
    await waitFor(() => expect(mocks.createBackup).toHaveBeenCalledWith("/Users/me/respaldo.zip"));
    expect(await screen.findByText(/SHA-256 feed/)).toBeInTheDocument();
    expect(screen.getByText(/3 archivos · 2.0 KB/)).toBeInTheDocument();
  });

  it("verifies a backup, asks for confirmation and only then stages the restore", async () => {
    mocks.open.mockResolvedValueOnce("/Users/me/viejo.zip");
    mocks.inspectBackup.mockResolvedValueOnce(info);
    mocks.stageRestore.mockResolvedValueOnce({ ...info, preRestoreBackup: "/data/backups/pre-restore.zip" });
    render(<BackupSection />);
    fireEvent.click(screen.getByRole("button", { name: /Restaurar/ }));
    expect(await screen.findByRole("alertdialog", { name: "Confirmar restauración" })).toHaveTextContent("12 artículos · 2 revisiones");
    expect(mocks.stageRestore).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /Preparar restauración/ }));
    expect(await screen.findByText(/pre-restore.zip/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Reiniciar y restaurar/ }));
    expect(mocks.relaunch).toHaveBeenCalled();
  });

  it("reports a damaged backup without staging anything", async () => {
    mocks.open.mockResolvedValueOnce("/Users/me/roto.zip");
    mocks.inspectBackup.mockRejectedValueOnce("Huella incorrecta en library/a/original.pdf");
    render(<BackupSection />);
    fireEvent.click(screen.getByRole("button", { name: /Restaurar/ }));
    expect(await screen.findByText(/Huella incorrecta/)).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("shows the diagnostics", async () => {
    mocks.diagnostics.mockResolvedValueOnce({ schemaVersion: 11, supportedSchemaVersion: 11, integrity: "ok", foreignKeyViolations: 0, databaseBytes: 1024, libraryFiles: 2, libraryBytes: 4096, articles: 2, projects: 1, missingPdfs: 1, missingPdfTitles: ["Estudio perdido"], pendingRestore: false });
    render(<BackupSection />);
    fireEvent.click(screen.getByRole("button", { name: /Diagnóstico/ }));
    expect(await screen.findByText("Hay aspectos que revisar")).toBeInTheDocument();
    expect(screen.getByText(/Faltan: Estudio perdido/)).toBeInTheDocument();
  });
});

describe("ZoteroSection", () => {
  it("prefills the default Zotero folder and summarizes the import", async () => {
    mocks.zoteroDefaultDir.mockResolvedValueOnce("/Users/me/Zotero");
    mocks.importZotero.mockResolvedValueOnce({ items: 5, imported: 3, linkedExisting: 1, alreadyLinked: 1, pdfsAttached: 2, collections: 2, errors: ["Sin título: referencia sin título"] });
    const onImported = vi.fn();
    render(<ZoteroSection onImported={onImported} />);
    await waitFor(() => expect(screen.getByLabelText("Carpeta de datos de Zotero")).toHaveValue("/Users/me/Zotero"));
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: /Importar desde Zotero/ }));
    await waitFor(() => expect(mocks.importZotero).toHaveBeenCalledWith("/Users/me/Zotero", false));
    expect(await screen.findByText(/3 nuevas \(2 con PDF\)/)).toBeInTheDocument();
    expect(screen.getByText(/referencia sin título/)).toBeInTheDocument();
    expect(onImported).toHaveBeenCalled();
  });
});
