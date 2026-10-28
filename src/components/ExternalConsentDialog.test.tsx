// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ExternalEstimate } from "../types";
import { ExternalConsentDialog } from "./ExternalConsentDialog";

const estimate: ExternalEstimate = {
  provider: "anthropic",
  model: "claude-opus-5-5",
  requests: 1,
  inputTokens: 2400,
  expectedOutputTokens: 600,
  maxOutputTokens: 3000,
  pricingKnown: true,
  expectedCostUsd: 0.0216,
  worstCaseCostUsd: 0.0888,
  monthSpentUsd: 0.5,
  monthlyBudgetUsd: 0,
  withinBudget: true,
  articles: [{ id: "a1", title: "Heritage Segmentation" }],
  excludedConfidential: 2,
  excerpts: 6,
};

afterEach(cleanup);

describe("ExternalConsentDialog", () => {
  it("shows what will be sent and binds the consent to the worst-case cost", () => {
    const onConfirm = vi.fn();
    render(<ExternalConsentDialog estimate={estimate} purpose="Prueba." onConfirm={onConfirm} onCancel={vi.fn()} />);
    expect(screen.getByText("Heritage Segmentation")).toBeInTheDocument();
    expect(screen.getByText(/2 extracto\(s\) de artículos confidenciales se excluyeron/)).toBeInTheDocument();
    expect(screen.getByText("0.0888 USD")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Autorizar este envío" }));
    expect(onConfirm).toHaveBeenCalledWith({ provider: "anthropic", model: "claude-opus-5-5", maxCostUsd: 0.0888 });
  });

  it("cannot be confirmed when the budget would be exceeded", () => {
    render(<ExternalConsentDialog estimate={{ ...estimate, monthlyBudgetUsd: 0.05, withinBudget: false }} purpose="" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Autorizar este envío" })).toBeDisabled();
    expect(screen.getByText(/superaría el presupuesto/)).toBeInTheDocument();
  });
});
