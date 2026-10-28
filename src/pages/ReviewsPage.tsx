import { FormEvent, useState } from "react";
import { FolderKanban, LoaderCircle, Plus, X } from "lucide-react";
import { Link } from "react-router-dom";
import { errorMessage } from "../lib/backend";
import { formatDate, projectStatusLabels, reviewTypeLabels } from "../lib/reviews";
import type { CreateProjectInput, ReviewProject, ReviewType } from "../types";

interface Props {
  projects: ReviewProject[];
  createProject: (input: CreateProjectInput) => Promise<void>;
}

const emptyForm: CreateProjectInput = { name: "", description: "", reviewType: "systematic_review" };

export function ReviewsPage({ projects, createProject }: Props) {
  const [open, setOpen] = useState(false);
  const [form, setForm] = useState<CreateProjectInput>(emptyForm);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await createProject(form);
      setForm(emptyForm);
      setOpen(false);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="mx-auto max-w-[1280px] p-8">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-sm text-slate-500">{projects.length} {projects.length === 1 ? "proyecto" : "proyectos"} en este equipo</p>
        </div>
        <button className="button-primary" onClick={() => setOpen(true)}><Plus className="size-4" /> Nueva revisión</button>
      </div>

      {projects.length ? (
        <div className="mt-6 grid grid-cols-3 gap-4">
          {projects.map((project) => (
            <Link key={project.id} to={`/revisiones/${project.id}`} className="panel group block p-5 transition hover:-translate-y-0.5 hover:shadow-md">
              <div className="flex items-start justify-between">
                <div className="grid size-10 place-items-center rounded-xl bg-teal-50 text-teal-700"><FolderKanban className="size-5" /></div>
                <span className="status-badge">{projectStatusLabels[project.status]}</span>
              </div>
              <h2 className="mt-4 line-clamp-2 text-[15px] font-semibold leading-5 text-slate-800">{project.name}</h2>
              <p className="mt-2 line-clamp-2 min-h-10 text-xs leading-5 text-slate-400">{project.description || "Sin descripción"}</p>
              <div className="mt-5 border-t border-slate-100 pt-4">
                <p className="text-xs font-medium text-slate-500">{reviewTypeLabels[project.reviewType]}</p>
                <p className="mt-1 text-[11px] text-slate-400">Creado {formatDate(project.createdAt)}</p>
              </div>
            </Link>
          ))}
        </div>
      ) : (
        <div className="panel mt-6 grid min-h-[440px] place-items-center p-10 text-center">
          <div className="max-w-sm">
            <div className="mx-auto grid size-14 place-items-center rounded-2xl bg-teal-50 text-teal-700"><FolderKanban className="size-6" /></div>
            <h2 className="mt-4 text-lg font-semibold text-slate-800">Crea tu primera revisión</h2>
            <p className="mt-2 text-sm leading-6 text-slate-500">Define el tipo de estudio y ResearchPilot preparará un espacio trazable para el protocolo.</p>
            <button className="button-primary mx-auto mt-5" onClick={() => setOpen(true)}><Plus className="size-4" /> Nueva revisión</button>
          </div>
        </div>
      )}

      {open && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/35 p-6 backdrop-blur-sm" role="dialog" aria-modal="true" aria-labelledby="new-review-title">
          <form className="w-full max-w-lg rounded-2xl bg-white p-6 shadow-2xl" onSubmit={submit}>
            <div className="flex items-start justify-between">
              <div>
                <h2 id="new-review-title" className="text-lg font-semibold text-slate-900">Nueva revisión</h2>
                <p className="mt-1 text-xs text-slate-400">Los datos se guardarán en SQLite local.</p>
              </div>
              <button type="button" aria-label="Cerrar" className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-100" onClick={() => setOpen(false)}><X className="size-4" /></button>
            </div>
            <label className="label mt-6">Nombre del proyecto
              <input className="input mt-1.5" value={form.name} minLength={3} maxLength={140} autoFocus required onChange={(event) => setForm({ ...form, name: event.target.value })} placeholder="Ej. IA generativa en educación superior" />
            </label>
            <label className="label mt-4">Tipo de estudio
              <select className="input mt-1.5" value={form.reviewType} onChange={(event) => setForm({ ...form, reviewType: event.target.value as ReviewType })}>
                {Object.entries(reviewTypeLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
              </select>
            </label>
            <label className="label mt-4">Descripción
              <textarea className="input mt-1.5 min-h-24 resize-none" value={form.description} onChange={(event) => setForm({ ...form, description: event.target.value })} placeholder="Objetivo o alcance inicial…" />
            </label>
            {error && <p className="mt-4 rounded-lg bg-rose-50 px-3 py-2 text-xs text-rose-700">{error}</p>}
            <div className="mt-6 flex justify-end gap-3">
              <button type="button" className="button-secondary" onClick={() => setOpen(false)}>Cancelar</button>
              <button className="button-primary" disabled={saving}>{saving && <LoaderCircle className="size-4 animate-spin" />} Crear revisión</button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
}
