import { FileText, Image, Link2, Table2 } from "lucide-react";
import { Link } from "react-router-dom";
import type { ArticleExtraction, TableFigure } from "../../types";

interface Props {
  extraction: ArticleExtraction;
  onShowPage: (page: number, bbox?: number[]) => void;
}

export function StructurePanel({ extraction, onShowPage }: Props) {
  const { pages, sections, tablesFigures, references } = extraction;
  if (!extraction.runs.some((run) => run.status === "completed")) {
    return <p className="rounded-xl border border-dashed border-slate-200 px-4 py-6 text-center text-xs text-slate-400">La estructura aparece cuando termina la extracción científica.</p>;
  }
  return (
    <div className="space-y-6">
      <div className="grid grid-cols-4 gap-2 text-center">
        <Stat label="Páginas" value={pages.total} />
        <Stat label="Con texto" value={pages.extracted} />
        <Stat label="OCR" value={pages.ocr} />
        <Stat label="Sin texto" value={pages.withoutText} warn={pages.withoutText > 0} />
      </div>

      <Section title="Secciones" count={sections.length}>
        {sections.length ? (
          <ol className="space-y-1">
            {sections.map((section, index) => (
              <li key={`${section.pageNumber}-${index}`}>
                <button className="flex w-full items-center justify-between gap-2 rounded-md px-2 py-1 text-left text-xs text-slate-600 hover:bg-slate-50" onClick={() => onShowPage(section.pageNumber, section.bbox)}>
                  <span className="truncate">{section.title}</span>
                  <span className="shrink-0 text-[10px] text-slate-400">p. {section.pageNumber}</span>
                </button>
              </li>
            ))}
          </ol>
        ) : <Empty text="No se detectaron encabezados." />}
      </Section>

      <Section title="Tablas y figuras" count={tablesFigures.length}>
        {tablesFigures.length ? (
          <div className="space-y-2">{tablesFigures.map((item) => <TableFigureCard key={item.id} item={item} onShowPage={onShowPage} />)}</div>
        ) : <Empty text="No se detectaron tablas ni figuras con leyenda." />}
      </Section>

      <Section title="Referencias" count={references.length}>
        {references.length ? (
          <ol className="space-y-2">
            {references.map((reference) => (
              <li key={reference.id} className="text-[11px] leading-4 text-slate-500">
                <span className="font-semibold text-slate-400">{reference.ordinal ?? "·"}</span> {reference.rawReference}
                <div className="mt-0.5 flex flex-wrap gap-2">
                  {reference.doi && <span className="font-mono text-[10px] text-slate-400">doi:{reference.doi}</span>}
                  {reference.matchedArticleId && <Link className="flex items-center gap-1 text-[10px] font-semibold text-teal-700" to={`/biblioteca/${reference.matchedArticleId}`}><Link2 className="size-3" /> En tu biblioteca</Link>}
                  {reference.pageNumber && <button className="text-[10px] text-slate-400 hover:text-teal-700" onClick={() => onShowPage(reference.pageNumber!)}>p. {reference.pageNumber}</button>}
                </div>
              </li>
            ))}
          </ol>
        ) : <Empty text="No se separaron referencias." />}
      </Section>
    </div>
  );
}

function TableFigureCard({ item, onShowPage }: { item: TableFigure; onShowPage: Props["onShowPage"] }) {
  const rows = item.content?.rows ?? [];
  const Icon = item.objectType === "table" ? Table2 : Image;
  return (
    <div className="rounded-xl border border-slate-100 p-2.5">
      <div className="flex items-start justify-between gap-2">
        <p className="flex items-start gap-1.5 text-xs text-slate-600"><Icon className="mt-0.5 size-3.5 shrink-0 text-teal-600" /><span><span className="font-semibold">{item.label ?? (item.objectType === "table" ? "Tabla sin leyenda" : "Figura")}</span>{item.caption && item.caption !== item.label ? ` — ${item.caption.replace(item.label ?? "", "").replace(/^[\s:.\-–]+/, "")}` : ""}</span></p>
        {item.pageNumber && <button className="shrink-0 text-[10px] font-semibold text-teal-700" onClick={() => onShowPage(item.pageNumber!, item.bbox)}>p. {item.pageNumber}</button>}
      </div>
      {rows.length > 0 ? (
        <div className="mt-2 max-h-40 overflow-auto rounded-md border border-slate-100">
          <table className="w-full text-[10px] text-slate-600">
            <tbody>{rows.slice(0, 8).map((row, index) => <tr key={index} className="border-b border-slate-50 last:border-0">{row.map((cell, cellIndex) => <td key={cellIndex} className="px-1.5 py-1 align-top">{cell}</td>)}</tr>)}</tbody>
          </table>
          {rows.length > 8 && <p className="px-1.5 py-1 text-[10px] text-slate-400">… {rows.length - 8} filas más</p>}
        </div>
      ) : item.objectType === "table" && <p className="mt-1 text-[10px] text-slate-400">Contenido no reconstruido automáticamente; consulta la página.</p>}
    </div>
  );
}

function Section({ title, count, children }: { title: string; count: number; children: React.ReactNode }) {
  return (
    <section>
      <h3 className="flex items-center gap-1.5 border-b border-slate-100 pb-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-slate-400"><FileText className="size-3.5" /> {title} <span className="font-normal normal-case tracking-normal">· {count}</span></h3>
      <div className="mt-2">{children}</div>
    </section>
  );
}

function Stat({ label, value, warn = false }: { label: string; value: number; warn?: boolean }) {
  return <div className="rounded-lg bg-slate-50 py-2"><p className={`text-base font-semibold ${warn ? "text-amber-700" : "text-slate-800"}`}>{value}</p><p className="text-[10px] text-slate-400">{label}</p></div>;
}

function Empty({ text }: { text: string }) {
  return <p className="text-[11px] text-slate-400">{text}</p>;
}
