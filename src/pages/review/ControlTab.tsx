import { useState } from "react";
import { CheckCircle2, CircleDashed, Plus, Trash2 } from "lucide-react";
import { reviewBackend } from "../../lib/reviewBackend";
import type { LibraryArticle } from "../../types";
import { Card, Empty, ErrorNote, Loading, useAction, useLoad } from "./shared";

export function ControlTab({ projectId, articles }: { projectId: string; articles: LibraryArticle[] }) {
  const coverage = useLoad(() => reviewBackend.controlCoverage(projectId), [projectId]);
  const [form, setForm] = useState({ articleId: "", rationale: "" });
  const action = useAction();
  const data = coverage.data;

  async function add() {
    const next = await action.run(() => reviewBackend.addControl(projectId, form.articleId, form.rationale));
    if (next) {
      coverage.setData(next);
      setForm({ articleId: "", rationale: "" });
    }
  }

  async function remove(id: string) {
    const next = await action.run(() => reviewBackend.removeControl(projectId, id));
    if (next) coverage.setData(next);
  }

  if (!data) return coverage.loading ? <Loading /> : <ErrorNote message={coverage.error} />;
  return (
    <div className="space-y-6">
      <Card title={`Artículos de control · ${data.found} de ${data.total} recuperados por las cadenas`}>
        {data.total < 5 && <p className="mb-3 rounded-lg bg-amber-50 px-3 py-2 text-[11px] text-amber-800">Se recomienda identificar al menos cinco estudios clave que esperas recuperar.</p>}
        {data.articles.length === 0 ? <Empty title="Sin artículos de control" detail="Agrega estudios clave conocidos desde la biblioteca." /> : (
          <table className="w-full text-left text-xs">
            <thead className="text-[11px] text-slate-400"><tr><th className="py-2 font-medium">Id</th><th className="font-medium">Reference</th><th className="font-medium">Justificación</th><th className="font-medium">Recuperado por</th><th /></tr></thead>
            <tbody>
              {data.articles.map((article, index) => (
                <tr key={article.id} className="border-t border-slate-100 align-top">
                  <td className="py-2 pr-2 text-slate-400">{index + 1}</td>
                  <td className="pr-3"><p className="font-medium text-slate-700">{article.title}</p><p className="text-[11px] text-slate-400">{article.year ?? "s. f."}{article.doi ? ` · doi:${article.doi}` : ""}{article.inCorpus ? " · en el corpus" : ""}</p></td>
                  <td className="pr-3 text-slate-500">{article.rationale || "—"}</td>
                  <td className="pr-3">{article.foundBy.length ? <span className="flex items-center gap-1 text-emerald-700"><CheckCircle2 className="size-3.5" /> {article.foundBy.join(", ")}</span> : <span className="flex items-center gap-1 text-amber-700"><CircleDashed className="size-3.5" /> No recuperado</span>}</td>
                  <td><button className="viewer-button size-7" aria-label="Quitar artículo de control" onClick={() => void remove(article.id)}><Trash2 className="size-3.5" /></button></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <p className="mt-3 text-[11px] text-slate-400">Un artículo cuenta como recuperado solo si aparece en los resultados importados de alguna cadena del proyecto. La cobertura indica sensibilidad, no exhaustividad.</p>
      </Card>
      <Card title="Agregar artículo de control">
        <div className="grid grid-cols-[1.5fr_1fr_auto] gap-2">
          <select className="input" value={form.articleId} onChange={(event) => setForm({ ...form, articleId: event.target.value })} aria-label="Artículo">
            <option value="">Elige un artículo de la biblioteca…</option>
            {articles.map((article) => <option key={article.id} value={article.id}>{article.title}</option>)}
          </select>
          <input className="input" placeholder="Justificación / relevancia esperada" value={form.rationale} onChange={(event) => setForm({ ...form, rationale: event.target.value })} />
          <button className="button-primary" disabled={!form.articleId || action.busy} onClick={() => void add()}><Plus className="size-4" /> Agregar</button>
        </div>
        <div className="mt-2"><ErrorNote message={action.error} /></div>
      </Card>
    </div>
  );
}
