import { FormEvent, useEffect, useState } from "react";
import { ArrowLeft, Check, Edit3, FileKey2, Folder, LoaderCircle, Save, X } from "lucide-react";
import { Link } from "react-router-dom";
import { PdfViewer } from "../components/PdfViewer";
import { errorMessage } from "../lib/backend";
import type { LibraryArticle, LibraryCollection, UpdateArticleMetadataInput } from "../types";

export function ArticleViewerPage({ article, collections, updateMetadata, setArticleCollection }: { article: LibraryArticle; collections: LibraryCollection[]; updateMetadata: (input: UpdateArticleMetadataInput) => Promise<LibraryArticle>; setArticleCollection: (articleId: string, collectionId: string, included: boolean) => Promise<void> }) {
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState(() => toForm(article));

  useEffect(() => setForm(toForm(article)), [article]);

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
    <div className="flex h-full min-h-0">
      <section className="min-w-0 flex-1 border-r border-slate-200">
        {article.documentPath ? (
          <PdfViewer path={article.documentPath} title={article.title} />
        ) : (
          <div className="grid h-full place-items-center text-sm text-slate-400">Este registro no tiene un PDF asociado.</div>
        )}
      </section>
      <aside className="w-[390px] shrink-0 overflow-y-auto bg-white p-6">
        <div className="flex items-center justify-between">
          <Link to="/biblioteca" className="flex items-center gap-1.5 text-xs font-semibold text-slate-500 hover:text-teal-700"><ArrowLeft className="size-4" /> Biblioteca</Link>
          {!editing && <button className="button-secondary px-3 py-2" onClick={() => setEditing(true)}><Edit3 className="size-3.5" /> Editar</button>}
        </div>

        {editing ? (
          <form className="mt-6 space-y-4" onSubmit={submit}>
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
          <div className="mt-6">
            {saved && <p className="mb-4 flex items-center gap-2 rounded-lg bg-emerald-50 px-3 py-2 text-xs font-medium text-emerald-700"><Check className="size-4" /> Metadatos guardados y auditados</p>}
            <h2 className="text-lg font-semibold leading-7 tracking-[-0.02em] text-slate-900">{article.title}</h2>
            <p className="mt-3 text-sm leading-6 text-slate-500">{article.authors.length ? article.authors.join("; ") : "Autor no identificado"}</p>
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
      </aside>
    </div>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return <label className="label">{label}{hint && <span className="ml-1 font-normal text-slate-400">· {hint}</span>}<div className="mt-1.5">{children}</div></label>;
}

function Metadata({ label, value }: { label: string; value?: string }) {
  return <div className="grid grid-cols-[100px_1fr] gap-3"><dt className="text-xs font-medium text-slate-400">{label}</dt><dd className="break-words text-xs text-slate-600">{value || "No reportado"}</dd></div>;
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
