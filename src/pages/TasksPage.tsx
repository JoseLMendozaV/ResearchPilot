import { useState } from "react";
import { AlertTriangle, ListChecks, LoaderCircle, Pause, Play, RotateCcw, ScanSearch, Square } from "lucide-react";
import { Link } from "react-router-dom";
import { backend, errorMessage } from "../lib/backend";
import { formatDateTime, isActiveJob, jobStatusLabels, jobStatusTones, jobTypeLabels } from "../lib/extraction";
import type { JobSummary } from "../types";

interface Props {
  jobs: JobSummary[];
  loading: boolean;
  error: string | null;
  pause: (jobId: string) => Promise<JobSummary>;
  resume: (jobId: string) => Promise<JobSummary>;
  cancel: (jobId: string) => Promise<JobSummary>;
  retry: (jobId: string) => Promise<JobSummary>;
  refresh: () => Promise<void>;
}

export function TasksPage({ jobs, loading, error, pause, resume, cancel, retry, refresh }: Props) {
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const active = jobs.filter((job) => isActiveJob(job.status));
  const finished = jobs.filter((job) => !isActiveJob(job.status));

  async function run(key: string, action: () => Promise<unknown>) {
    setBusy(key);
    setActionError(null);
    try {
      await action();
    } catch (cause) {
      setActionError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  async function extractPending() {
    await run("pending", async () => {
      const results = await backend.enqueuePendingExtractions();
      setNotice(results.length ? `${results.length} artículo(s) añadidos a la cola.` : "No hay artículos pendientes: todos tienen una extracción vigente.");
      await refresh();
    });
  }

  return (
    <div className="mx-auto max-w-[1280px] p-8">
      <div className="flex items-end justify-between gap-4">
        <div>
          <h2 className="section-title">Cola de procesamiento local</h2>
          <p className="section-subtitle">Las tareas persisten en SQLite: sobreviven a cierres y se reanudan al abrir la aplicación.</p>
        </div>
        <button className="button-primary" onClick={() => void extractPending()} disabled={busy === "pending"}>
          {busy === "pending" ? <LoaderCircle className="size-4 animate-spin" /> : <ScanSearch className="size-4" />}
          Extraer artículos pendientes
        </button>
      </div>
      {notice && <p className="mt-4 rounded-lg bg-teal-50 px-3 py-2 text-xs text-teal-800">{notice}</p>}
      {(error || actionError) && <p className="mt-4 rounded-lg bg-rose-50 px-3 py-2 text-xs text-rose-700">{actionError ?? error}</p>}

      <section className="panel mt-6 p-6">
        <h3 className="text-sm font-semibold text-slate-800">En curso <span className="font-normal text-slate-400">· {active.length}</span></h3>
        {loading ? (
          <div className="grid h-32 place-items-center"><LoaderCircle className="size-5 animate-spin text-teal-600" /></div>
        ) : active.length ? (
          <div className="mt-4 divide-y divide-slate-100">{active.map((job) => <JobRow key={job.id} job={job} busy={busy} run={run} pause={pause} resume={resume} cancel={cancel} retry={retry} />)}</div>
        ) : (
          <div className="mt-4 rounded-xl border border-dashed border-slate-200 px-5 py-8 text-center">
            <ListChecks className="mx-auto size-6 text-slate-300" />
            <p className="mt-2 text-sm font-medium text-slate-600">No hay tareas en curso</p>
            <p className="mt-1 text-xs text-slate-400">Al importar un PDF se encola su extracción científica automáticamente.</p>
          </div>
        )}
      </section>

      {finished.length > 0 && (
        <section className="panel mt-6 p-6">
          <h3 className="text-sm font-semibold text-slate-800">Historial reciente</h3>
          <div className="mt-4 divide-y divide-slate-100">{finished.map((job) => <JobRow key={job.id} job={job} busy={busy} run={run} pause={pause} resume={resume} cancel={cancel} retry={retry} />)}</div>
        </section>
      )}
    </div>
  );
}

function JobRow({ job, busy, run, pause, resume, cancel, retry }: { job: JobSummary; busy: string | null; run: (key: string, action: () => Promise<unknown>) => Promise<void> } & Pick<Props, "pause" | "resume" | "cancel" | "retry">) {
  const disabled = busy === job.id;
  const action = (handler: (jobId: string) => Promise<JobSummary>) => () => void run(job.id, () => handler(job.id));
  return (
    <article className="flex items-start gap-4 py-4 first:pt-0 last:pb-0" aria-label={`${jobTypeLabels[job.jobType] ?? job.jobType}: ${job.articleTitle ?? ""}`}>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className={`rounded-full px-2.5 py-1 text-[10px] font-semibold ${jobStatusTones[job.status]}`}>{jobStatusLabels[job.status]}</span>
          <span className="text-xs font-semibold text-slate-700">{jobTypeLabels[job.jobType] ?? job.jobType}</span>
          {job.attempts > 1 && <span className="text-[10px] text-slate-400">intento {job.attempts} de {job.maxAttempts}</span>}
        </div>
        {job.articleId && (
          <Link to={`/biblioteca/${job.articleId}`} className="mt-1 block truncate text-sm text-slate-600 hover:text-teal-700">{job.articleTitle ?? "Artículo"}</Link>
        )}
        {(job.status === "running" || job.status === "queued") && (
          <div className="mt-2.5 flex items-center gap-3">
            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-slate-100" role="progressbar" aria-valuenow={Math.round(job.progress * 100)} aria-valuemin={0} aria-valuemax={100}>
              <div className="h-full rounded-full bg-teal-500 transition-all" style={{ width: `${Math.round(job.progress * 100)}%` }} />
            </div>
            <span className="w-10 text-right text-[11px] tabular-nums text-slate-500">{Math.round(job.progress * 100)}%</span>
          </div>
        )}
        {job.message && <p className="mt-1.5 text-[11px] text-slate-400">{job.message}</p>}
        {job.errorMessage && <p className="mt-1.5 flex items-start gap-1.5 text-[11px] text-rose-700"><AlertTriangle className="mt-px size-3.5 shrink-0" />{job.errorMessage}</p>}
        <p className="mt-1 text-[10px] text-slate-300">Creada {formatDateTime(job.createdAt)}{job.finishedAt ? ` · terminó ${formatDateTime(job.finishedAt)}` : ""}</p>
      </div>
      <div className="flex shrink-0 gap-1.5">
        {(job.status === "running" || job.status === "queued") && <button className="button-secondary px-3 py-2" disabled={disabled} onClick={action(pause)}><Pause className="size-3.5" /> Pausar</button>}
        {job.status === "paused" && <button className="button-secondary px-3 py-2" disabled={disabled} onClick={action(resume)}><Play className="size-3.5" /> Reanudar</button>}
        {isActiveJob(job.status) && <button className="button-secondary px-3 py-2" disabled={disabled} onClick={action(cancel)}><Square className="size-3.5" /> Cancelar</button>}
        {(job.status === "failed" || job.status === "cancelled") && <button className="button-secondary px-3 py-2" disabled={disabled} onClick={action(retry)}><RotateCcw className="size-3.5" /> Reintentar</button>}
      </div>
    </article>
  );
}
