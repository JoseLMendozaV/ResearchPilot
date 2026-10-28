import { useEffect, useMemo, useState } from "react";
import { FileCheck2, Fingerprint, LoaderCircle, Quote, ScanText, Search } from "lucide-react";
import { Link } from "react-router-dom";
import { backend, errorMessage } from "../lib/backend";
import { statusLabels, statusTones } from "../lib/extraction";
import type { DashboardStats, EvidenceListItem, ValueStatus } from "../types";

export function EvidencePage({ stats }: { stats: DashboardStats }) {
  const [items, setItems] = useState<EvidenceListItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<ValueStatus | "all">("all");

  useEffect(() => {
    backend.recentEvidence(500).then(setItems).catch((cause) => setError(errorMessage(cause)));
  }, []);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return (items ?? []).filter((item) =>
      (status === "all" || item.valueStatus === status) &&
      (!needle || `${item.quote} ${item.fieldLabel} ${item.articleTitle} ${item.displayValue}`.toLowerCase().includes(needle)),
    );
  }, [items, query, status]);

  return (
    <div className="mx-auto max-w-[1280px] p-8">
      <div className="grid grid-cols-[1fr_1.4fr] gap-6">
        <section className="panel p-6">
          <div className="grid size-11 place-items-center rounded-xl bg-teal-50 text-teal-700"><FileCheck2 className="size-5" /></div>
          <p className="mt-5 text-4xl font-semibold tracking-[-0.05em] text-slate-900">{stats.evidenceSpans}</p>
          <p className="mt-1 text-sm font-medium text-slate-500">fragmentos de evidencia registrados</p>
          <p className="mt-4 text-xs leading-5 text-slate-400">{stats.valuesToReview} valor(es) propuestos esperan validación humana.</p>
        </section>
        <section className="panel p-6">
          <h2 className="section-title">Cadena de procedencia</h2>
          <p className="section-subtitle">Cada valor conserva cita literal, página, posición y huella del documento</p>
          <div className="mt-5 grid grid-cols-3 gap-3">
            <Feature icon={ScanText} title="Localizador" text="Página y caja en el PDF" />
            <Feature icon={Quote} title="Fragmento" text="Cita exacta conservada" />
            <Feature icon={Fingerprint} title="Integridad" text="SHA-256 del documento" />
          </div>
        </section>
      </div>

      <section className="panel mt-6 p-6">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 className="section-title">Evidencias recientes</h2>
          <div className="flex gap-2">
            <label className="relative">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-slate-300" />
              <input className="input w-64 pl-8" placeholder="Buscar en citas, campos o artículos" value={query} onChange={(event) => setQuery(event.target.value)} aria-label="Buscar evidencias" />
            </label>
            <select className="input w-40" value={status} onChange={(event) => setStatus(event.target.value as ValueStatus | "all")} aria-label="Filtrar por estado">
              <option value="all">Todos los estados</option>
              {(["needs_review", "proposed", "confirmed", "rejected"] as const).map((item) => <option key={item} value={item}>{statusLabels[item]}</option>)}
            </select>
          </div>
        </div>
        {error && <p className="mt-4 rounded-lg bg-rose-50 p-3 text-xs text-rose-700">{error}</p>}
        {!items && !error ? (
          <div className="grid h-40 place-items-center"><LoaderCircle className="size-5 animate-spin text-teal-600" /></div>
        ) : visible.length ? (
          <div className="mt-4 divide-y divide-slate-100">
            {visible.map((item) => (
              <article key={item.id} className="grid grid-cols-[180px_1fr_auto] gap-4 py-3.5">
                <div className="min-w-0">
                  <p className="text-xs font-semibold text-slate-700">{item.fieldLabel}</p>
                  <span className={`mt-1 inline-block rounded-full px-2 py-0.5 text-[10px] font-semibold ${statusTones[item.valueStatus]}`}>{statusLabels[item.valueStatus]}</span>
                </div>
                <div className="min-w-0">
                  <p className="text-xs italic leading-5 text-slate-600">“{item.quote}”</p>
                  <p className="mt-1 truncate text-[11px] text-slate-400">{item.articleTitle}</p>
                </div>
                <Link className="self-start whitespace-nowrap text-xs font-semibold text-teal-700 hover:text-teal-600" to={`/biblioteca/${item.articleId}?evidence=${item.id}`}>
                  {item.page ? `Ver p. ${item.page}` : "Abrir"}
                </Link>
              </article>
            ))}
          </div>
        ) : (
          <div className="mt-4 grid min-h-48 place-items-center text-center">
            <div className="max-w-md">
              <p className="text-sm font-semibold text-slate-700">{items?.length ? "Ninguna evidencia coincide con el filtro" : "No hay evidencias todavía"}</p>
              <p className="mt-2 text-xs leading-5 text-slate-400">Importa un PDF: el extractor local propondrá valores de la ficha científica, cada uno enlazado a su cita y página.</p>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}

function Feature({ icon: Icon, title, text }: { icon: typeof ScanText; title: string; text: string }) {
  return (
    <div className="rounded-xl bg-slate-50 p-4">
      <Icon className="size-4 text-teal-600" />
      <p className="mt-3 text-xs font-semibold text-slate-700">{title}</p>
      <p className="mt-1 text-[11px] text-slate-400">{text}</p>
    </div>
  );
}
