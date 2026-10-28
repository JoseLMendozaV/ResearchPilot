import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { ChevronLeft, ChevronRight, LoaderCircle, Minus, Plus } from "lucide-react";
import * as pdfjs from "pdfjs-dist";
import workerSrc from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import type { ViewerFocus } from "../types";

pdfjs.GlobalWorkerOptions.workerSrc = workerSrc;

interface Rendered {
  page: number;
  width: number;
  height: number;
  /** Page width in PDF points, used when evidence has no page size. */
  baseWidth: number;
}

export function PdfViewer({ path, title, focus }: { path: string; title: string; focus?: ViewerFocus }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const highlightRef = useRef<HTMLDivElement>(null);
  const [document, setDocument] = useState<pdfjs.PDFDocumentProxy | null>(null);
  const [page, setPage] = useState(1);
  const [scale, setScale] = useState(1.15);
  const [rendered, setRendered] = useState<Rendered | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    const task = pdfjs.getDocument({ url: convertFileSrc(path) });
    setLoading(true);
    setError(null);
    void task.promise
      .then((loaded) => {
        if (!active) return;
        setDocument(loaded);
        setPage(1);
        setLoading(false);
      })
      .catch((cause: unknown) => {
        if (!active) return;
        setError(cause instanceof Error ? cause.message : "No se pudo abrir el PDF");
        setLoading(false);
      });
    return () => {
      active = false;
      void task.destroy();
    };
  }, [path]);

  useEffect(() => {
    if (!focus || !document) return;
    setPage(Math.min(Math.max(1, focus.page), document.numPages));
  }, [focus, document]);

  useEffect(() => {
    if (!document || !canvasRef.current) return;
    let active = true;
    let renderTask: pdfjs.RenderTask | undefined;
    void document.getPage(page).then((pdfPage) => {
      if (!active || !canvasRef.current) return;
      const viewport = pdfPage.getViewport({ scale });
      const ratio = window.devicePixelRatio || 1;
      const canvas = canvasRef.current;
      const context = canvas.getContext("2d");
      if (!context) return;
      canvas.width = Math.floor(viewport.width * ratio);
      canvas.height = Math.floor(viewport.height * ratio);
      canvas.style.width = `${Math.floor(viewport.width)}px`;
      canvas.style.height = `${Math.floor(viewport.height)}px`;
      renderTask = pdfPage.render({ canvas, canvasContext: context, viewport, transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0] });
      return renderTask.promise.then(() => {
        if (active) setRendered({ page, width: viewport.width, height: viewport.height, baseWidth: pdfPage.getViewport({ scale: 1 }).width });
      });
    }).catch((cause: unknown) => {
      if (active && !(cause instanceof pdfjs.RenderingCancelledException)) {
        setError(cause instanceof Error ? cause.message : "No se pudo renderizar la página");
      }
    });
    return () => {
      active = false;
      renderTask?.cancel();
    };
  }, [document, page, scale]);

  // Evidence boxes are PDF points from the top-left corner (pdfplumber convention).
  const box = focus?.bbox && rendered && rendered.page === focus.page ? (() => {
    const factor = rendered.width / (focus.pageWidth || rendered.baseWidth);
    const [x0, top, x1, bottom] = focus.bbox;
    return { left: x0 * factor - 3, top: top * factor - 3, width: (x1 - x0) * factor + 6, height: (bottom - top) * factor + 6 };
  })() : null;

  useEffect(() => {
    if (box) highlightRef.current?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [box?.left, box?.top, focus?.key]);

  if (loading) return <div className="grid h-full place-items-center"><div className="text-center"><LoaderCircle className="mx-auto size-6 animate-spin text-teal-600" /><p className="mt-2 text-xs text-slate-400">Abriendo documento local…</p></div></div>;
  if (error) return <div className="grid h-full place-items-center p-8 text-center text-sm text-rose-600">{error}</div>;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-12 shrink-0 items-center justify-between border-b border-slate-200 bg-white px-3">
        <p className="max-w-[40%] truncate text-xs font-medium text-slate-600" title={title}>{title}</p>
        <div className="flex items-center gap-1">
          <button className="viewer-button" disabled={page <= 1} onClick={() => setPage((current) => current - 1)} aria-label="Página anterior"><ChevronLeft className="size-4" /></button>
          <span className="min-w-20 text-center text-[11px] font-medium text-slate-500">{page} / {document?.numPages ?? 0}</span>
          <button className="viewer-button" disabled={page >= (document?.numPages ?? 1)} onClick={() => setPage((current) => current + 1)} aria-label="Página siguiente"><ChevronRight className="size-4" /></button>
        </div>
        <div className="flex items-center gap-1">
          <button className="viewer-button" onClick={() => setScale((current) => Math.max(0.6, current - 0.15))} aria-label="Alejar"><Minus className="size-4" /></button>
          <span className="min-w-12 text-center text-[11px] text-slate-500">{Math.round(scale * 100)}%</span>
          <button className="viewer-button" onClick={() => setScale((current) => Math.min(2.4, current + 0.15))} aria-label="Acercar"><Plus className="size-4" /></button>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto bg-slate-200/70 p-6">
        <div className="relative mx-auto w-fit">
          <canvas ref={canvasRef} className="bg-white shadow-lg" aria-label={`Página ${page} de ${title}`} />
          {box && (
            <div
              ref={highlightRef}
              key={focus?.key}
              className="evidence-highlight pointer-events-none absolute rounded-sm border-2 border-amber-500 bg-amber-300/25"
              style={box}
              aria-label="Evidencia resaltada"
            />
          )}
        </div>
      </div>
    </div>
  );
}
