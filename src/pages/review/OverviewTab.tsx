import { FormEvent, useState } from "react";
import { Plus, Save } from "lucide-react";
import { reviewBackend } from "../../lib/reviewBackend";
import type { ProjectSettingsInput, ProjectWorkspace } from "../../reviewTypes";
import type { LibraryArticle } from "../../types";
import { Card, ErrorNote, stageLabels, useAction } from "./shared";

export function OverviewTab({ workspace, onChange, articles }: { workspace: ProjectWorkspace; onChange: (next: ProjectWorkspace) => void; articles: LibraryArticle[] }) {
  const project = workspace.project;
  const [form, setForm] = useState<ProjectSettingsInput>({ id: project.id, name: project.name, description: project.description, reviewType: project.reviewType, status: project.status, screeningMode: project.screeningMode, qualityThreshold: project.qualityThreshold, qualityMissingPolicy: project.qualityMissingPolicy });
  const [reviewer, setReviewer] = useState({ name: "", role: "reviewer" });
  const [selected, setSelected] = useState<string[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const action = useAction();

  async function save(event: FormEvent) {
    event.preventDefault();
    const next = await action.run(() => reviewBackend.updateSettings(form));
    if (next) {
      onChange(next);
      setNotice("Configuración guardada");
    }
  }

  async function addReviewer() {
    const next = await action.run(() => reviewBackend.addReviewer(project.id, reviewer.name, reviewer.role));
    if (next) {
      onChange(next);
      setReviewer({ name: "", role: "reviewer" });
    }
  }

  async function addArticles() {
    const added = await action.run(() => reviewBackend.addToCorpus(project.id, selected));
    if (added !== undefined) {
      setNotice(`${added} artículo(s) añadidos al corpus`);
      setSelected([]);
      const next = await reviewBackend.workspace(project.id);
      onChange(next);
    }
  }

  return (
    <div className="grid grid-cols-[1.3fr_1fr] gap-6">
      <div className="space-y-6">
        <Card title="Estado del corpus">
          <div className="grid grid-cols-6 gap-2 text-center">
            {(["identified", "screening", "eligible", "included", "excluded"] as const).map((stage) => (
              <div key={stage} className="rounded-lg bg-slate-50 py-2.5"><p className="text-lg font-semibold text-slate-800">{workspace.counts[stage]}</p><p className="text-[10px] text-slate-400">{stageLabels[stage]}</p></div>
            ))}
            <div className="rounded-lg bg-slate-50 py-2.5"><p className="text-lg font-semibold text-slate-800">{workspace.counts.secondary}</p><p className="text-[10px] text-slate-400">Secundarios</p></div>
          </div>
          <p className="mt-3 text-[11px] text-slate-400">Las etapas se derivan solo de decisiones finales registradas; nada se marca como incluido sin una decisión humana.</p>
        </Card>
        <Card title="Configuración de la revisión">
          <form className="space-y-3" onSubmit={save}>
            <label className="label">Nombre<input className="input mt-1" value={form.name} onChange={(event) => setForm({ ...form, name: event.target.value })} /></label>
            <label className="label">Descripción<textarea className="input mt-1 min-h-16" value={form.description} onChange={(event) => setForm({ ...form, description: event.target.value })} /></label>
            <div className="grid grid-cols-3 gap-3">
              <label className="label">Tipo
                <select className="input mt-1" value={form.reviewType} onChange={(event) => setForm({ ...form, reviewType: event.target.value as ProjectSettingsInput["reviewType"] })}>
                  <option value="systematic_review">Revisión sistemática</option><option value="systematic_mapping">Mapeo sistemático</option><option value="scoping_review">Revisión de alcance</option>
                </select>
              </label>
              <label className="label">Estado
                <select className="input mt-1" value={form.status} onChange={(event) => setForm({ ...form, status: event.target.value as ProjectSettingsInput["status"] })}>
                  <option value="draft">Borrador</option><option value="active">En curso</option><option value="completed">Completada</option><option value="archived">Archivada</option>
                </select>
              </label>
              <label className="label">Revisión de estudios
                <select className="input mt-1" value={form.screeningMode} onChange={(event) => setForm({ ...form, screeningMode: event.target.value as ProjectSettingsInput["screeningMode"] })}>
                  <option value="single">Un revisor</option><option value="double">Dos revisores independientes</option>
                </select>
              </label>
              <label className="label">Umbral de calidad (%)
                <input className="input mt-1" type="number" min={0} max={100} value={form.qualityThreshold ?? ""} onChange={(event) => setForm({ ...form, qualityThreshold: event.target.value === "" ? undefined : Number(event.target.value) })} placeholder="Sin umbral" />
              </label>
              <label className="label col-span-2">Preguntas de calidad sin respuesta
                <select className="input mt-1" value={form.qualityMissingPolicy} onChange={(event) => setForm({ ...form, qualityMissingPolicy: event.target.value as ProjectSettingsInput["qualityMissingPolicy"] })}>
                  <option value="zero">Cuentan como 0 (el máximo no cambia)</option><option value="exclude_from_max">Se excluyen del máximo</option>
                </select>
              </label>
            </div>
            <div className="flex items-center justify-end gap-3">
              {notice && <span className="text-xs text-emerald-700">{notice}</span>}
              <button className="button-primary" disabled={action.busy}><Save className="size-4" /> Guardar</button>
            </div>
          </form>
        </Card>
      </div>
      <div className="space-y-6">
        <Card title="Revisores">
          <ul className="space-y-1.5 text-xs">
            {workspace.reviewers.map((item) => <li key={item.id} className="flex justify-between rounded-lg bg-slate-50 px-3 py-2"><span className="font-medium text-slate-700">{item.displayName}{item.isLocalProfile ? " (perfil local)" : ""}</span><span className="text-slate-400">{item.role === "owner" ? "Responsable" : item.role === "resolver" ? "Tercer revisor" : "Revisor"}</span></li>)}
          </ul>
          <div className="mt-3 grid grid-cols-[1fr_120px_auto] gap-2">
            <input className="input" placeholder="Nombre del revisor" value={reviewer.name} onChange={(event) => setReviewer({ ...reviewer, name: event.target.value })} />
            <select className="input" value={reviewer.role} onChange={(event) => setReviewer({ ...reviewer, role: event.target.value })}><option value="reviewer">Revisor</option><option value="resolver">Tercer revisor</option></select>
            <button className="button-secondary px-3" disabled={reviewer.name.trim().length < 2} onClick={() => void addReviewer()}><Plus className="size-3.5" /></button>
          </div>
          <p className="mt-2 text-[11px] leading-4 text-slate-400">La colaboración en tiempo real no está disponible: las decisiones de otro revisor se registran seleccionándolo en «Registrando como» (por ejemplo, al transcribir su hoja).</p>
        </Card>
        <Card title="Agregar desde la biblioteca" actions={<button className="button-secondary px-3 py-1.5 text-[11px]" disabled={!selected.length || action.busy} onClick={() => void addArticles()}>Añadir {selected.length || ""}</button>}>
          <div className="max-h-72 space-y-1 overflow-y-auto">
            {articles.map((article) => (
              <label key={article.id} className="flex items-start gap-2 rounded-md px-2 py-1.5 text-xs text-slate-600 hover:bg-slate-50">
                <input type="checkbox" className="mt-0.5 accent-teal-600" checked={selected.includes(article.id)} onChange={(event) => setSelected((current) => event.target.checked ? [...current, article.id] : current.filter((id) => id !== article.id))} />
                <span className="line-clamp-2">{article.title}{article.publicationYear ? ` (${article.publicationYear})` : ""}</span>
              </label>
            ))}
            {articles.length === 0 && <p className="text-xs text-slate-400">La biblioteca está vacía. También puedes importar resultados desde la pestaña Cadena.</p>}
          </div>
        </Card>
        <ErrorNote message={action.error} />
      </div>
    </div>
  );
}
