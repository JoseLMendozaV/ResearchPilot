import { useCallback, useEffect, useState } from "react";
import { Database, Download, LoaderCircle, RefreshCw, TriangleAlert } from "lucide-react";
import { backend, errorMessage } from "../../lib/backend";
import { onJobChanged } from "../../lib/events";
import { jobStatusLabels } from "../../lib/extraction";
import type { AppSettings, IndexStatus, JobSummary, OllamaModel, OllamaStatus } from "../../types";

const fitLabels: Record<NonNullable<OllamaModel["memoryFit"]>, { label: string; tone: string }> = {
  comfortable: { label: "cabe con holgura", tone: "text-emerald-700" },
  tight: { label: "justo para esta RAM", tone: "text-amber-700" },
  too_large: { label: "demasiado grande", tone: "text-rose-700" },
};

interface Props {
  form: AppSettings;
  setForm: (settings: AppSettings) => void;
}

export function AiSettings({ form, setForm }: Props) {
  const [status, setStatus] = useState<OllamaStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const [index, setIndex] = useState<IndexStatus | null>(null);
  const [pullName, setPullName] = useState("");
  const [confirmPull, setConfirmPull] = useState(false);
  const [jobs, setJobs] = useState<Record<string, JobSummary>>({});
  const [error, setError] = useState<string | null>(null);

  const check = useCallback(async () => {
    setChecking(true);
    try {
      const [ollama, indexStatus] = await Promise.all([backend.ollamaStatus(), backend.indexStatus()]);
      setStatus(ollama);
      setIndex(indexStatus);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => {
    void check();
    return onJobChanged((job) => {
      if (job.jobType !== "model_pull" && job.jobType !== "rag_index") return;
      setJobs((current) => ({ ...current, [job.jobType]: job }));
      if (job.status === "completed") void check();
    });
  }, [check]);

  async function pull() {
    setConfirmPull(false);
    setError(null);
    try {
      const job = await backend.pullOllamaModel(pullName.trim());
      setJobs((current) => ({ ...current, model_pull: job }));
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  async function reindex() {
    setError(null);
    try {
      const queued = await backend.enqueueIndexing();
      setJobs((current) => ({ ...current, rag_index: { ...(current.rag_index ?? ({} as JobSummary)), status: "queued", message: `${queued.length} artículo(s) en cola`, progress: 0 } }));
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  const chatModels = status?.models.filter((model) => model.kind !== "embedding") ?? [];
  const embeddingModels = status?.models.filter((model) => model.kind !== "chat") ?? [];

  return (
    <div className="space-y-5">
      <div className="rounded-xl bg-slate-50 p-4">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0 text-[11px]">
            <p className="font-semibold text-slate-700">
              {checking ? "Comprobando Ollama…" : status?.reachable ? `Ollama ${status.version} activo · ${status.models.length} modelo(s)` : "Ollama no disponible"}
            </p>
            {status && <p className="mt-1 text-slate-500">{status.recommendation}</p>}
            {status?.message && <p className="mt-1 flex items-start gap-1.5 text-amber-800"><TriangleAlert className="mt-px size-3.5 shrink-0" />{status.message}</p>}
          </div>
          <button type="button" className="button-secondary shrink-0 px-3 py-2" onClick={() => void check()} disabled={checking}>
            {checking ? <LoaderCircle className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />} Comprobar
          </button>
        </div>
        {status && status.models.length > 0 && (
          <table className="mt-3 w-full text-left text-[11px]">
            <thead className="text-slate-400"><tr><th className="py-1 font-medium">Modelo</th><th className="font-medium">Tipo</th><th className="font-medium">Tamaño</th><th className="font-medium">Memoria</th></tr></thead>
            <tbody className="text-slate-600">
              {status.models.map((model) => (
                <tr key={model.name} className="border-t border-slate-200/70">
                  <td className="py-1.5 font-mono">{model.name}</td>
                  <td>{model.kind === "embedding" ? "Embeddings" : model.kind === "chat" ? "Lenguaje" : "Desconocido"}{model.parameterSize ? ` · ${model.parameterSize}` : ""}</td>
                  <td>{(model.sizeBytes / 1024 ** 3).toFixed(1)} GB</td>
                  <td className={model.memoryFit ? fitLabels[model.memoryFit].tone : ""}>{model.memoryFit ? fitLabels[model.memoryFit].label : "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <div className="grid grid-cols-3 gap-4">
        <ModelSelect label="Modelo para el asistente" value={form.ollamaModel} models={chatModels} onChange={(value) => setForm({ ...form, ollamaModel: value })} />
        <ModelSelect label="Modelo para la ficha (opcional)" value={form.ollamaExtractionModel} models={chatModels} emptyLabel="Igual que el asistente" onChange={(value) => setForm({ ...form, ollamaExtractionModel: value })} />
        <ModelSelect label="Modelo de embeddings" value={form.ollamaEmbeddingModel} models={embeddingModels} emptyLabel="Sin búsqueda semántica" onChange={(value) => setForm({ ...form, ollamaEmbeddingModel: value })} />
      </div>

      <div className="grid grid-cols-[1fr_auto] items-end gap-3">
        <label className="label">Descargar un modelo en Ollama
          <input className="input mt-1.5 font-mono" value={pullName} onChange={(event) => setPullName(event.target.value)} placeholder="nombre:etiqueta, según ollama.com/library" />
        </label>
        <button type="button" className="button-secondary" disabled={!pullName.trim() || !status?.reachable} onClick={() => setConfirmPull(true)}><Download className="size-3.5" /> Descargar</button>
      </div>
      {confirmPull && (
        <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-[11px] leading-4 text-amber-900">
          Ollama descargará <strong className="font-mono">{pullName.trim()}</strong> desde internet (puede ocupar varios GB). Ningún documento se envía; revisa la licencia del modelo antes de usarlo.
          <div className="mt-2 flex gap-2">
            <button type="button" className="button-primary px-3 py-1.5" onClick={() => void pull()}>Confirmar descarga</button>
            <button type="button" className="button-secondary px-3 py-1.5" onClick={() => setConfirmPull(false)}>Cancelar</button>
          </div>
        </div>
      )}
      {jobs.model_pull && <p className="text-[11px] text-slate-500">Descarga: <span className="font-semibold">{jobStatusLabels[jobs.model_pull.status]}</span>{jobs.model_pull.message ? ` · ${jobs.model_pull.message}` : ""}{jobs.model_pull.errorMessage ? ` · ${jobs.model_pull.errorMessage}` : ""}</p>}

      <div className="grid grid-cols-4 gap-4 border-t border-slate-100 pt-4">
        <label className="label">Búsqueda
          <select className="input mt-1.5" value={form.ragMode} onChange={(event) => setForm({ ...form, ragMode: event.target.value as AppSettings["ragMode"] })}>
            <option value="hybrid">Híbrida (léxica + semántica)</option>
            <option value="lexical">Solo léxica (FTS5)</option>
            <option value="semantic">Solo semántica</option>
          </select>
        </label>
        <label className="label">Fragmentos por respuesta
          <input className="input mt-1.5" type="number" min={1} max={30} value={form.ragTopK} onChange={(event) => setForm({ ...form, ragTopK: Number(event.target.value) })} />
        </label>
        <label className="label">Presupuesto de contexto (tokens)
          <input className="input mt-1.5" type="number" min={1000} max={64000} step={500} value={form.ragContextTokens} onChange={(event) => setForm({ ...form, ragContextTokens: Number(event.target.value) })} />
        </label>
        <label className="label">Tiempo máximo por consulta (s)
          <input className="input mt-1.5" type="number" min={10} max={3600} value={form.aiTimeoutSeconds} onChange={(event) => setForm({ ...form, aiTimeoutSeconds: Number(event.target.value) })} />
        </label>
      </div>

      <div className="flex items-center justify-between gap-4 rounded-xl bg-slate-50 p-4">
        <div className="text-[11px] text-slate-600">
          <p className="flex items-center gap-1.5 font-semibold text-slate-700"><Database className="size-3.5" /> Índice de búsqueda</p>
          {index ? (
            <p className="mt-1">{index.indexedArticles} de {index.articles} artículos · {index.chunks} fragmentos · {index.embeddedChunks} con embeddings{index.embeddingModel ? ` (${index.embeddingModel})` : ""}</p>
          ) : <p className="mt-1">—</p>}
          {jobs.rag_index && <p className="mt-1 text-slate-500">Indexación: {jobStatusLabels[jobs.rag_index.status]}{jobs.rag_index.message ? ` · ${jobs.rag_index.message}` : ""}</p>}
          <label className="mt-2 flex items-center gap-2"><input type="checkbox" className="accent-teal-600" checked={form.autoIndexAfterExtraction} onChange={(event) => setForm({ ...form, autoIndexAfterExtraction: event.target.checked })} /> Indexar automáticamente tras cada extracción</label>
        </div>
        <button type="button" className="button-secondary shrink-0" onClick={() => void reindex()}><RefreshCw className="size-3.5" /> Indexar biblioteca</button>
      </div>
      {error && <p className="text-[11px] text-rose-700">{error}</p>}
    </div>
  );
}

function ModelSelect({ label, value, models, emptyLabel = "Sin seleccionar", onChange }: { label: string; value: string; models: OllamaModel[]; emptyLabel?: string; onChange: (value: string) => void }) {
  const known = models.some((model) => model.name === value);
  return (
    <label className="label">{label}
      <select className="input mt-1.5" value={value} onChange={(event) => onChange(event.target.value)}>
        <option value="">{emptyLabel}</option>
        {value && !known && <option value={value}>{value} (no disponible)</option>}
        {models.map((model) => <option key={model.name} value={model.name}>{model.name}</option>)}
      </select>
    </label>
  );
}
