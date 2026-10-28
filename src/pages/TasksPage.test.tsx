// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { JobSummary } from "../types";
import { TasksPage } from "./TasksPage";

const base: JobSummary = {
  id: "job-1",
  jobType: "scientific_extraction",
  status: "running",
  progress: 0.42,
  stage: "layout",
  message: "Analizando diseño: página 3 de 7",
  articleId: "article-1",
  articleTitle: "Traceable Evidence",
  attempts: 1,
  maxAttempts: 2,
  createdAt: "2026-10-28T00:00:00Z",
  updatedAt: "2026-10-28T00:00:00Z",
};

function renderPage(jobs: JobSummary[]) {
  const props = {
    jobs,
    loading: false,
    error: null,
    pause: vi.fn().mockResolvedValue(base),
    resume: vi.fn().mockResolvedValue(base),
    cancel: vi.fn().mockResolvedValue(base),
    retry: vi.fn().mockResolvedValue(base),
    refresh: vi.fn().mockResolvedValue(undefined),
  };
  render(<MemoryRouter><TasksPage {...props} /></MemoryRouter>);
  return props;
}

afterEach(cleanup);

describe("TasksPage", () => {
  it("shows real progress and controls for a running extraction", async () => {
    const props = renderPage([base]);
    expect(screen.getByText("Traceable Evidence")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "42");
    expect(screen.getByText("Analizando diseño: página 3 de 7")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Pausar/ }));
    await waitFor(() => expect(props.pause).toHaveBeenCalledWith("job-1"));
  });

  it("offers retry for failed jobs and shows the error", async () => {
    const props = renderPage([{ ...base, status: "failed", errorMessage: "PDF dañado (invalid_pdf)" }]);
    expect(screen.getByText("Historial reciente")).toBeInTheDocument();
    expect(screen.getByText("PDF dañado (invalid_pdf)")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Pausar/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Reintentar/ }));
    await waitFor(() => expect(props.retry).toHaveBeenCalledWith("job-1"));
  });

  it("shows an empty state without fabricated tasks", () => {
    renderPage([]);
    expect(screen.getByText("No hay tareas en curso")).toBeInTheDocument();
  });
});
