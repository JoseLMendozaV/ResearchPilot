// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ConversationDetail } from "../types";
import { AssistantPage } from "./AssistantPage";

const askAssistant = vi.fn();
const searchChunks = vi.fn();
vi.mock("../lib/backend", () => ({
  backend: {
    listConversations: vi.fn().mockResolvedValue([]),
    ollamaStatus: vi.fn().mockResolvedValue({ baseUrl: "http://127.0.0.1:11434", reachable: true, version: "0.0.0", models: [{ name: "local-chat:8b", sizeBytes: 1, capabilities: ["completion"], kind: "chat" }], recommendation: "" }),
    askAssistant: (input: unknown) => askAssistant(input),
    searchChunks: (input: unknown) => searchChunks(input),
    cancelAssistant: vi.fn(),
  },
  errorMessage: (error: unknown) => String(error),
}));

const conversation = { id: "c1", title: "¿Exactitud?", scopeType: "library" as const, messageCount: 2, createdAt: "2026-10-28T00:00:00Z", updatedAt: "2026-10-28T00:00:00Z" };

function detail(status: "answered" | "unsupported"): ConversationDetail {
  return {
    conversation,
    messages: [
      { id: "m1", conversationId: "c1", role: "user", content: "¿Qué exactitud reporta?", status: "question", citations: [], sources: null, createdAt: "2026-10-28T00:00:00Z" },
      {
        id: "m2",
        conversationId: "c1",
        role: "assistant",
        content: status === "answered" ? "Reporta 93.2% [S1]." : "Se descartó para no presentar información sin evidencia.",
        status,
        citations: status === "answered" ? [{ source: "S1", quote: "an accuracy of 93.2%", articleId: "a1", articleTitle: "Heritage Segmentation", page: 2, bbox: [10, 20, 30, 40], pageWidth: 595 }] : [],
        sources: { mode: "hybrid", warnings: [], chunks: [{ source: "S1", articleId: "a1", articleTitle: "Heritage Segmentation", page: 2 }] },
        createdAt: "2026-10-28T00:00:01Z",
      },
    ],
  };
}

function renderPage() {
  render(<MemoryRouter><AssistantPage articles={[]} collections={[]} defaultModel="local-chat:8b" /></MemoryRouter>);
}

afterEach(cleanup);

describe("AssistantPage", () => {
  it("shows verified citations that open the cited page and region", async () => {
    askAssistant.mockResolvedValueOnce(detail("answered"));
    renderPage();
    fireEvent.change(screen.getByRole("textbox", { name: "Pregunta" }), { target: { value: "¿Qué exactitud reporta?" } });
    fireEvent.click(screen.getByRole("button", { name: /Preguntar$/ }));
    expect(await screen.findByText("Respuesta con evidencia verificada")).toBeInTheDocument();
    expect(screen.getByText("“an accuracy of 93.2%”")).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /Heritage Segmentation · p. 2/ });
    expect(link).toHaveAttribute("href", "/biblioteca/a1?page=2&bbox=10.0%2C20.0%2C30.0%2C40.0&pw=595");
    expect(askAssistant).toHaveBeenCalledWith(expect.objectContaining({ question: "¿Qué exactitud reporta?", scopeType: "library", model: "local-chat:8b" }));
  });

  it("labels discarded answers instead of presenting them as evidence", async () => {
    askAssistant.mockResolvedValueOnce(detail("unsupported"));
    renderPage();
    fireEvent.change(screen.getByRole("textbox", { name: "Pregunta" }), { target: { value: "¿Exactitud?" } });
    fireEvent.click(screen.getByRole("button", { name: /Preguntar$/ }));
    expect(await screen.findByText("Respuesta descartada: citas no verificables")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /p\. 2/ })).not.toBeInTheDocument();
  });

  it("searches fragments without the language model", async () => {
    searchChunks.mockResolvedValueOnce({ mode: "lexical", warnings: ["Búsqueda semántica no disponible"], chunks: [{ chunkId: "k1", articleId: "a1", articleTitle: "Heritage Segmentation", page: 3, text: "learning rate of 0.001", score: 0.1, lexicalRank: 1 }] });
    renderPage();
    fireEvent.click(screen.getByRole("tab", { name: "Buscar fragmentos" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Búsqueda" }), { target: { value: "learning rate" } });
    fireEvent.click(screen.getByRole("button", { name: /Buscar$/ }));
    await waitFor(() => expect(screen.getByText("learning rate of 0.001")).toBeInTheDocument());
    expect(screen.getByText("Búsqueda semántica no disponible")).toBeInTheDocument();
    expect(screen.getByText(/léxico #1/)).toBeInTheDocument();
  });
});
