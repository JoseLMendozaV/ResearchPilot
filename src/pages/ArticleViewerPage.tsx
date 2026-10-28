import { FormEvent, useCallback, useEffect, useState } from "react";
import { ArrowLeft, Check, ClipboardCheck, Edit3, FileKey2, Folder, LoaderCircle, Save, X } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";
import { PdfViewer } from "../components/PdfViewer";
import { MasterRecordPanel } from "../components/extraction/MasterRecordPanel";
import { StructurePanel } from "../components/extraction/StructurePanel";
import { backend, errorMessage } from "../lib/backend";
import { onJobChanged } from "../lib/events";
import type { ArticleExtraction, EvidenceItem, JobSummary, LibraryArticle, LibraryCollection, MasterValue, UpdateArticleMetadataInput, ViewerFocus } from "../types";

type Tab = "record" | "structure" | "metadata";

export function ArticleViewerPage({ article, collections, updateMetadata, setArticleCollection }: { article: LibraryArticle; collections: LibraryCollection[]; updateMetadata: (input: UpdateArticleMetadataInput) => Promise<LibraryArticle>; setArticleCollection: (articleId: string, collectionId: string, included: boolean) => Promise<void> }) {
  const [searchParams] = useSearchParams();
  const [tab, setTab] = useState<Tab>("record");
  const [extraction, setExtraction] = useState<ArticleExtraction | null>(null);
  const [extractionError, setExtractionError] = useState<string | null>(null);
  const [focus, setFocus] = useState<ViewerFocus | undefined>();
  const [aiJob, setAiJob] = useState<JobSummary | undefined>();

  const loadExtraction = useCallback(async () => {
    try {
      setExtraction(await backend.articleExtraction(article.id));
      setExtractionError(null);
    } catch (cause) {
      setExtractionError(errorMessage(cause));
    }
  }, [article.id]);

  useEffect(() => {
    void loadExtraction();
    return onJobChanged((job) => {
      if (job.articleId !== article.id) return;
      const active = job.status === "queued" || job.status === "running" || job.status === "paused";
      if (job.jobType === "ai_field_extraction") {
        setAiJob(active ? job : undefined);
        if (!active) void loadExtraction();
        return;
      }
      if (job.jobType !== "scientific_extraction") return;
      setExtraction((current) => (current ? { ...current, activeJob: job.status === "queued" || job.status === "running" || job.status === "paused" ? job : undefined } : current));
      if (job.status === "completed" || job.status === "failed" || job.status === "cancelled") void loadExtraction();
    });
  }, [article.id, loadExtraction]);

  // While a job is active, poll as a fallback for missed events.
  useEffect(() => {
    if (!extraction?.activeJob) return;
    const timer = window.setInterval(() => void loadExtraction(), 3000);
    return () => window.clearInterval(timer);
  }, [extraction?.activeJob, loadExtraction]);

  // Deep links from the evidence list: /biblioteca/:id?evidence=<id>
  const evidenceParam = searchParams.get("evidence");
  useEffect(() => {
    if (!evidenceParam || !extraction) return;
    const evidence = extraction.values.flatMap((value) => value.evidence).find((item) => item.id === evidenceParam);
    if (evidence?.page) setFocus({ page: evidence.page, bbox: evidence.bbox, pageWidth: evidence.pageWidth, key: Date.now() });
  }, [evidenceParam, extraction]);

  // Citation links from the assistant: ?page=N&bbox=x0,top,x1,bottom&pw=width
  const pageParam = Number(searchParams.get("page"));
  const bboxParam = searchParams.get("bbox");
  const widthParam = Number(searchParams.get("pw")) || undefined;
  useEffect(() => {
    if (!pageParam) return;
    const bbox = bboxParam?.split(",").map(Number).filter((value) => Number.isFinite(value));
    setFocus({ page: pageParam, bbox: bbox?.length === 4 ? bbox : undefined, pageWidth: widthParam, key: Date.now() });
  }, [pageParam, bboxParam, widthParam]);

  async function proposeWithAi() {
    setAiJob(await backend.enqueueAiFields(article.id));
  }

  const showEvidence = (evidence: EvidenceItem) => {
    if (evidence.page) setFocus({ page: evidence.page, bbox: evidence.bbox, pageWidth: evidence.pageWidth, key: Date.now() });
  };

  const replaceValue = (value: MasterValue) => {
    setExtraction((current) => {
      if (!current) return current;
      const exists = current.values.some((item) => item.id === value.id);
      return { ...current, values: exists ? current.values.map((item) => (item.id === value.id ? value : item)) : [...current.values, value] };
    });
    // Conflict flags depend on sibling values, so refresh from the database.
    void loadExtraction();
  };

  async function extract(force: boolean) {
    await backend.enqueueExtraction([article.id], force);
    await loadExtraction();
  }

  async function jobAction(action: "pause" | "cancel") {
    const job = extraction?.activeJob;
    if (!job) return;
    await (action === "pause" ? backend.pauseJob(job.id) : backend.cancelJob(job.id));
    await loadExtraction();
  }

  return (
    <div className="flex h-full min-h-0">
      <section className="min-w-0 flex-1 border-r border-slate-200">
        {article.documentPath ? (
          <PdfViewer path={article.documentPath} title={article.title} focus={focus} />
        ) : (
          <div className="grid h-full place-items-center text-sm text-slate-400">Este registro no tiene un PDF asociado.</div>
        )}
      </section>
      <aside className="flex w-[460px] shrink-0 flex-col bg-white">
        <div className="border-b border-slate-100 px-6 pt-5">
          <Link to="/biblioteca" className="flex items-center gap-1.5 text-xs font-semibold text-slate-500 hover:text-teal-700"><ArrowLeft className="size-4" /> Biblioteca</Link>
          <h2 className="mt-3 line-clamp-2 text-sm font-semibold leading-5 text-slate-900" title={article.title}>{article.title}</h2>
          <div className="mt-3 flex gap-4 text-xs font-semibold" role="tablist" aria-label="Paneles del artículo">
            {([["record", "Ficha científica"], ["structure", "Estructura"], ["metadata", "Metadatos"]] as const).map(([key, label]) => (
              <button key={key} role="tab" aria-selected={tab === key} className={`border-b-2 pb-2.5 transition ${tab === key ? "border-teal-600 text-teal-800" : "border-transparent text-slate-400 hover:text-slate-600"}`} onClick={() => setTab(key)}>{label}</button>
            ))}
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-6">
          {tab !== "metadata" && !extraction && !extractionError && <div className="grid h-40 place-items-center"><LoaderCircle className="size-5 animate-spin text-teal-600" /></div>}
          {tab !== "metadata" && extractionError && <p className="rounded-lg bg-rose-50 p-3 text-xs text-rose-700">{extractionError}</p>}
          {tab === "record" && extraction && (
            <MasterRecordPanel articleId={article.id} extraction={extraction} onExtract={extract} onJobAction={jobAction} onProposeWithAi={proposeWithAi} aiJob={aiJob} onValueChanged={replaceValue} onShowEvidence={showEvidence} />
          )}
          {tab === "structure" && extraction && (
            <StructurePanel extraction={extraction} onShowPage={(page, bbox) => setFocus({ page, bbox, key: Date.now() })} />
          )}
          {tab === "metadata" && (
            <MetadataPanel article={article} collections={collections} extraction={extraction} updateMetadata={updateMetadata} setArticleCollection={setArticleCollection} />
          )}
        </div>
      </aside>
    </div>
  );
}

function MetadataPanel({ article, collections, extraction, updateMetadata, setArticleCollection }: { article: LibraryArticle; collections: LibraryCollection[]; extraction: ArticleExtraction | null; updateMetadata: (input: UpdateArticleMetadataInput) => Promise<LibraryArticle>; setArticleCollection: (articleId: string, collectionId: string, included: boolean) => Promise<void> }) {
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState(() => toForm(article));

  useEffect(() => setForm(toForm(article)), [article]);

  const confirmed = confirmedBibliography(extraction);
  const canPrefill = Object.keys(confirmed).length > 0;

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      await updateMetadata({
        id: article.id,
        title: form.title,
        abstractText: form.abstractText || undefined,
        publicationYear: form.publicationYear ? Number(form.publicationYear) : undefined,
        doi: form.doi || undefined,
        authors: splitList(form.authors),
        keywords: splitList(form.keywords),
        venue: form.venue || undefined,
      });
      setEditing(false);
      setSaved(true);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSaving(false);
    }
  }

  async function toggleCollection(collectionId: string, included: boolean) {
    setError(null);
    try {
      await setArticleCollection(article.id, collectionId, included);
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  return (
    <div>
      <div className="flex items-center justify-end gap-2">
        {editing && canPrefill && (
          <button className="button-secondary px-3 py-2" type="button" onClick={() => setForm({ ...form, ...confirmed })} title="Solo usa valores confirmados por una persona">
            <ClipboardCheck className="size-3.5" /> Usar valores confirmados de la ficha
          </button>
        )}
        {!editing && <button className="button-secondary px-3 py-2" onClick={() => setEditing(true)}><Edit3 className="size-3.5" /> Editar</button>}
      </div>

      {editing ? (
        <form className="mt-4 space-y-4" onSubmit={submit}>
          <Field label="Título"><textarea className="input min-h-20 resize-none" required value={form.title} onChange={(event) => setForm({ ...form, title: event.target.value })} /></Field>
          <Field label="Autores" hint="Separados por punto y coma"><input className="input" value={form.authors} onChange={(event) => setForm({ ...form, authors: event.target.value })} /></Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label="Año"><input className="input" inputMode="numeric" value={form.publicationYear} onChange={(event) => setForm({ ...form, publicationYear: event.target.value })} /></Field>
            <Field label="DOI"><input className="input" value={form.doi} onChange={(event) => setForm({ ...form, doi: event.target.value })} /></Field>
          </div>
          <Field label="Revista o conferencia"><input className="input" value={form.venue} onChange={(event) => setForm({ ...form, venue: event.target.value })} /></Field>
          <Field label="Palabras clave" hint="Separadas por punto y coma"><input className="input" value={form.keywords} onChange={(event) => setForm({ ...form, keywords: event.target.value })} /></Field>
          <Field label="Resumen"><textarea className="input min-h-32 resize-y" value={form.abstractText} onChange={(event) => setForm({ ...form, abstractText: event.target.value })} /></Field>
          {error && <p className="rounded-lg bg-rose-50 p-3 text-xs text-rose-700">{error}</p>}
          <div className="flex justify-end gap-2 pt-2">
            <button type="button" className="button-secondary" onClick={() => { setEditing(false); setForm(toForm(article)); }}><X className="size-4" /> Cancelar</button>
            <button className="button-primary" disabled={saving}>{saving ? <LoaderCircle className="size-4 animate-spin" /> : <Save className="size-4" />} Guardar</button>
          </div>
        </form>
      ) : (
        <div className="mt-4">
          {saved && <p className="mb-4 flex items-center gap-2 rounded-lg bg-emerald-50 px-3 py-2 text-xs font-medium text-emerald-700"><Check className="size-4" /> Metadatos guardados y auditados</p>}
          <p className="text-sm leading-6 text-slate-500">{article.authors.length ? article.authors.join("; ") : "Autor no identificado"}</p>
          <dl className="mt-6 space-y-4 border-t border-slate-100 pt-5">
            <Metadata label="Año" value={article.publicationYear?.toString()} />
            <Metadata label="DOI" value={article.doi} />
            <Metadata label="Publicación" value={article.venue} />
            <Metadata label="Páginas" value={article.pageCount.toString()} />
            <Metadata label="Archivo" value={article.fileName} />
          </dl>
          {article.keywords.length > 0 && <div className="mt-6 flex flex-wrap gap-1.5">{article.keywords.map((keyword) => <span key={keyword} className="rounded-full bg-teal-50 px-2.5 py-1 text-[10px] font-medium text-teal-700">{keyword}</span>)}</div>}
          {collections.length > 0 && <div className="mt-6 border-t border-slate-100 pt-5"><h3 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.12em] text-slate-400"><Folder className="size-3.5" /> Colecciones</h3><div className="mt-3 space-y-2">{collections.map((collection) => <label key={collection.id} className="flex items-center gap-2 text-xs text-slate-600"><input type="checkbox" className="accent-teal-600" checked={article.collectionIds.includes(collection.id)} onChange={(event) => void toggleCollection(collection.id, event.target.checked)} /> {collection.name}</label>)}</div></div>}
          {article.abstractText && <div className="mt-6"><h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-slate-400">Resumen</h3><p className="mt-2 text-xs leading-6 text-slate-500">{article.abstractText}</p></div>}
          {error && <p className="mt-4 rounded-lg bg-rose-50 p-3 text-xs text-rose-700">{error}</p>}
          {article.documentSha256 && <div className="mt-6 rounded-xl bg-slate-50 p-3"><div className="flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.1em] text-slate-400"><FileKey2 className="size-3.5" /> SHA-256</div><p className="mt-2 break-all font-mono text-[9px] leading-4 text-slate-500">{article.documentSha256}</p></div>}
        </div>
      )}
    </div>
  );
}

/** Map human-confirmed bibliography values onto metadata form fields. */
function confirmedBibliography(extraction: ArticleExtraction | null): Partial<ReturnType<typeof toForm>> {
  const result: Partial<ReturnType<typeof toForm>> = {};
  const pick = (fieldKey: string) => extraction?.values.find((value) => value.fieldKey === fieldKey && value.status === "confirmed");
  const asList = (value?: MasterValue) => (value && Array.isArray(value.value) ? (value.value as string[]).join("; ") : value?.displayValue);
  const entries: [keyof ReturnType<typeof toForm>, string | undefined][] = [
    ["title", pick("bibliography.title")?.displayValue],
    ["doi", pick("bibliography.doi")?.displayValue],
    ["abstractText", pick("bibliography.abstract")?.displayValue],
    ["venue", pick("bibliography.venue")?.displayValue],
    ["keywords", asList(pick("bibliography.keywords"))],
    ["authors", asList(pick("bibliography.authors"))],
    ["publicationYear", pick("bibliography.year")?.displayValue],
  ];
  for (const [key, value] of entries) if (value) result[key] = value;
  return result;
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return <label className="label">{label}{hint && <span className="ml-1 font-normal text-slate-400">· {hint}</span>}<div className="mt-1.5">{children}</div></label>;
}

function Metadata({ label, value }: { label: string; value?: string }) {
  return <div className="grid grid-cols-[100px_1fr] gap-3"><dt className="text-xs font-medium text-slate-400">{label}</dt><dd className="break-words text-xs text-slate-600">{value || "Sin dato"}</dd></div>;
}

function toForm(article: LibraryArticle) {
  return {
    title: article.title,
    authors: article.authors.join("; "),
    publicationYear: article.publicationYear?.toString() ?? "",
    doi: article.doi ?? "",
    venue: article.venue ?? "",
    keywords: article.keywords.join("; "),
    abstractText: article.abstractText ?? "",
  };
}

function splitList(value: string): string[] {
  return value.split(";").map((item) => item.trim()).filter(Boolean);
}
