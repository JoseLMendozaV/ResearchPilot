import { FormEvent, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  BookOpen,
  CheckCircle2,
  CopyCheck,
  FilePlus2,
  FileText,
  FileUp,
  Folder,
  FolderPlus,
  LoaderCircle,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { Link } from "react-router-dom";
import { errorMessage } from "../lib/backend";
import { formatDate } from "../lib/reviews";
import type { ImportItemResult, LibraryArticle, LibraryCollection } from "../types";

interface Props {
  articles: LibraryArticle[];
  collections: LibraryCollection[];
  importPdfFiles: (paths: string[]) => Promise<ImportItemResult[]>;
  importReferenceFiles: (paths: string[]) => Promise<ImportItemResult[]>;
  searchLibrary: (query: string) => Promise<LibraryArticle[]>;
  archiveArticle: (articleId: string) => Promise<void>;
  createCollection: (name: string) => Promise<LibraryCollection>;
}

export function LibraryPage({ articles, collections, importPdfFiles, importReferenceFiles, searchLibrary, archiveArticle, createCollection }: Props) {
  const [visible, setVisible] = useState(articles);
  const [query, setQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [importing, setImporting] = useState(false);
  const [importingReferences, setImportingReferences] = useState(false);
  const [results, setResults] = useState<ImportItemResult[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedCollection, setSelectedCollection] = useState<string | null>(null);
  const [newCollection, setNewCollection] = useState("");
  const [creatingCollection, setCreatingCollection] = useState(false);

  const displayed = selectedCollection
    ? visible.filter((article) => article.collectionIds.includes(selectedCollection))
    : visible;

  useEffect(() => {
    if (!query.trim()) setVisible(articles);
  }, [articles, query]);

  async function choosePdfs() {
    setError(null);
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [{ name: "Documentos PDF", extensions: ["pdf"] }],
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    setImporting(true);
    try {
      setResults(await importPdfFiles(paths));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setImporting(false);
    }
  }

  async function chooseReferences() {
    setError(null);
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [{ name: "Referencias bibliográficas", extensions: ["ris", "bib", "bibtex", "csv"] }],
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    setImportingReferences(true);
    try {
      setResults(await importReferenceFiles(paths));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setImportingReferences(false);
    }
  }

  async function search(event: FormEvent) {
    event.preventDefault();
    setSearching(true);
    setError(null);
    try {
      setVisible(await searchLibrary(query));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSearching(false);
    }
  }

  function clearSearch() {
    setQuery("");
    setVisible(articles);
  }

  async function archive(article: LibraryArticle) {
    if (!window.confirm(`¿Archivar “${article.title}”? El PDF se conservará y la acción quedará auditada.`)) return;
    try {
      await archiveArticle(article.id);
      setVisible((current) => current.filter((item) => item.id !== article.id));
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  async function submitCollection(event: FormEvent) {
    event.preventDefault();
    if (!newCollection.trim()) return;
    setCreatingCollection(true);
    setError(null);
    try {
      const collection = await createCollection(newCollection);
      setNewCollection("");
      setSelectedCollection(collection.id);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setCreatingCollection(false);
    }
  }

  return (
    <div className="mx-auto max-w-[1280px] p-8">
      <div className="flex items-center justify-between gap-6">
        <form className="relative w-[420px]" onSubmit={search}>
          <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-slate-400" />
          <input
            className="input pl-9 pr-16"
            placeholder="Buscar título, resumen, autores o palabras clave…"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            aria-label="Buscar artículos"
          />
          {query && (
            <button type="button" aria-label="Limpiar búsqueda" className="absolute right-10 top-1/2 -translate-y-1/2 text-slate-400" onClick={clearSearch}><X className="size-4" /></button>
          )}
          <button type="submit" aria-label="Ejecutar búsqueda" className="absolute right-3 top-1/2 -translate-y-1/2 text-teal-600">
            {searching ? <LoaderCircle className="size-4 animate-spin" /> : <Search className="size-4" />}
          </button>
        </form>
        <div className="flex items-center gap-2">
          <button className="button-secondary" onClick={chooseReferences} disabled={importingReferences}>
            {importingReferences ? <LoaderCircle className="size-4 animate-spin" /> : <FileUp className="size-4" />}
            {importingReferences ? "Importando…" : "Importar RIS/BibTeX/CSV"}
          </button>
          <button className="button-primary" onClick={choosePdfs} disabled={importing}>
            {importing ? <LoaderCircle className="size-4 animate-spin" /> : <FilePlus2 className="size-4" />}
            {importing ? "Procesando PDF…" : "Importar PDF"}
          </button>
        </div>
      </div>

      {error && (
        <div className="mt-4 flex items-center gap-2 rounded-xl border border-rose-200 bg-rose-50 px-4 py-3 text-xs text-rose-700">
          <AlertCircle className="size-4" /> {error}
        </div>
      )}

      {results && <ImportSummary results={results} close={() => setResults(null)} />}

      <div className="mt-5 flex items-center gap-2 overflow-x-auto pb-1">
        <button className={`collection-chip ${selectedCollection === null ? "collection-chip-active" : ""}`} onClick={() => setSelectedCollection(null)}><Folder className="size-3.5" /> Todos <span>{articles.length}</span></button>
        {collections.map((collection) => (
          <button key={collection.id} className={`collection-chip ${selectedCollection === collection.id ? "collection-chip-active" : ""}`} onClick={() => setSelectedCollection(collection.id)}><Folder className="size-3.5" /> {collection.name} <span>{collection.articleCount}</span></button>
        ))}
        <form className="flex shrink-0 items-center gap-1" onSubmit={submitCollection}>
          <input className="input w-40 py-1.5" value={newCollection} onChange={(event) => setNewCollection(event.target.value)} placeholder="Nueva colección" aria-label="Nombre de nueva colección" />
          <button className="viewer-button bg-white" disabled={creatingCollection || newCollection.trim().length < 2} aria-label="Crear colección">{creatingCollection ? <LoaderCircle className="size-4 animate-spin" /> : <FolderPlus className="size-4" />}</button>
        </form>
      </div>

      {displayed.length ? (
        <div className="panel mt-6 overflow-hidden">
          <div className="grid grid-cols-[minmax(0,1fr)_140px_100px_110px_44px] gap-4 border-b border-slate-100 bg-slate-50/70 px-5 py-3 text-[10px] font-semibold uppercase tracking-[0.12em] text-slate-400">
            <span>Artículo</span><span>Documento</span><span>Año</span><span>Importado</span><span />
          </div>
          <div className="divide-y divide-slate-100">
            {displayed.map((article) => (
              <article key={article.id} className="grid grid-cols-[minmax(0,1fr)_140px_100px_110px_44px] items-center gap-4 px-5 py-4 transition hover:bg-slate-50/60">
                <Link to={`/biblioteca/${article.id}`} className="group min-w-0">
                  <div className="flex gap-3">
                    <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-rose-50 text-rose-600"><FileText className="size-5" /></div>
                    <div className="min-w-0">
                      <h2 className="truncate text-sm font-semibold text-slate-800 group-hover:text-teal-700">{article.title}</h2>
                      <p className="mt-1 truncate text-xs text-slate-400">{article.authors.length ? article.authors.join("; ") : "Autor no identificado"}{article.venue ? ` · ${article.venue}` : ""}</p>
                      {article.keywords.length > 0 && <p className="mt-1 truncate text-[10px] text-slate-300">{article.keywords.join(" · ")}</p>}
                    </div>
                  </div>
                </Link>
                <div>
                  <p className="truncate text-xs font-medium text-slate-600" title={article.fileName}>{article.fileName ?? "Solo metadatos"}</p>
                  <p className="mt-1 text-[10px] text-slate-400">{article.pageCount} pág. · {formatBytes(article.byteSize)}</p>
                </div>
                <p className="text-xs text-slate-500">{article.publicationYear ?? "—"}</p>
                <p className="text-xs text-slate-400">{formatDate(article.importedAt)}</p>
                <button className="rounded-lg p-2 text-slate-300 transition hover:bg-rose-50 hover:text-rose-600" aria-label={`Archivar ${article.title}`} onClick={() => archive(article)}><Trash2 className="size-4" /></button>
              </article>
            ))}
          </div>
        </div>
      ) : (
        <div className="panel mt-6 grid min-h-[480px] place-items-center p-10 text-center">
          <div className="max-w-md">
            <div className="mx-auto grid size-14 place-items-center rounded-2xl bg-teal-50 text-teal-700"><BookOpen className="size-6" /></div>
            <h2 className="mt-4 text-lg font-semibold text-slate-800">{query || selectedCollection ? "No se encontraron artículos" : "Tu biblioteca está vacía"}</h2>
            <p className="mt-2 text-sm leading-6 text-slate-500">
              {query || selectedCollection ? "Prueba con otros términos o selecciona otra colección; la búsqueda usa el índice FTS5 local." : "Importa uno o varios PDF. Se conservará una copia local con su hash, metadatos y texto por página."}
            </p>
            {!query && !selectedCollection && <button className="button-primary mx-auto mt-5" onClick={choosePdfs}><FilePlus2 className="size-4" /> Seleccionar PDF</button>}
          </div>
        </div>
      )}
    </div>
  );
}

function ImportSummary({ results, close }: { results: ImportItemResult[]; close: () => void }) {
  const imported = results.filter((item) => item.status === "imported").length;
  const duplicates = results.filter((item) => item.status === "duplicate").length;
  const errors = results.filter((item) => item.status === "error").length;
  return (
    <div className="panel mt-4 flex items-center gap-5 px-4 py-3">
      <div className="flex items-center gap-2 text-xs font-semibold text-emerald-700"><CheckCircle2 className="size-4" /> {imported} importados</div>
      <div className="flex items-center gap-2 text-xs font-semibold text-amber-700"><CopyCheck className="size-4" /> {duplicates} duplicados</div>
      <div className="flex items-center gap-2 text-xs font-semibold text-rose-700"><AlertCircle className="size-4" /> {errors} errores</div>
      <div className="min-w-0 flex-1 truncate text-[11px] text-slate-400">
        {results.find((item) => item.message)?.message ?? "Importación finalizada con trazabilidad local."}
      </div>
      <button onClick={close} aria-label="Cerrar resumen" className="text-slate-400"><X className="size-4" /></button>
    </div>
  );
}

function formatBytes(value?: number): string {
  if (value === undefined) return "—";
  if (value < 1024 * 1024) return `${Math.max(1, Math.round(value / 1024))} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}
