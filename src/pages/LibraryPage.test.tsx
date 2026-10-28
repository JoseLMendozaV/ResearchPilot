// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { LibraryPage } from "./LibraryPage";

describe("LibraryPage", () => {
  it("renders persisted article metadata and real import actions", () => {
    render(
      <MemoryRouter>
        <LibraryPage
          articles={[{
            id: "paper-1",
            title: "Verifiable Evidence",
            authors: ["Ana Researcher"],
            keywords: ["traceability"],
            documentPath: "/managed/original.pdf",
            fileName: "paper.pdf",
            pageCount: 12,
            byteSize: 2048,
            importedAt: "2026-10-28T00:00:00Z",
            updatedAt: "2026-10-28T00:00:00Z",
            collectionIds: [],
            confidential: false,
          }]}
          collections={[]}
          importPdfFiles={vi.fn()}
          importReferenceFiles={vi.fn()}
          searchLibrary={vi.fn()}
          archiveArticle={vi.fn()}
          createCollection={vi.fn()}
        />
      </MemoryRouter>,
    );

    expect(screen.getByText("Verifiable Evidence")).toBeInTheDocument();
    expect(screen.getByText("Ana Researcher")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Importar PDF" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Importar RIS/BibTeX/CSV" })).toBeEnabled();
  });
});
