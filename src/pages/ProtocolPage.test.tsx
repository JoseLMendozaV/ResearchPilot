// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ProtocolPage, workflowModules } from "./ProtocolPage";

describe("ProtocolPage", () => {
  it("covers the seven sheets from the evidence workbook", () => {
    render(<ProtocolPage />);
    expect(workflowModules.map((module) => module.sheet)).toEqual([
      "Leer",
      "Control",
      "RQs",
      "Cadena",
      "I_E",
      "Calidad",
      "Extraccion",
    ]);
    expect(screen.getByTestId("workflow-modules").children).toHaveLength(7);
  });
});
