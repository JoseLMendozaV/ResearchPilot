import { FormEvent, useCallback, useEffect, useState } from "react";
import { Check, Database, Download, HardDrive, LoaderCircle, RefreshCw, ScanText, ShieldCheck, Sparkles, TriangleAlert } from "lucide-react";
import { backend, errorMessage } from "../lib/backend";
import { onJobChanged } from "../lib/events";
import { jobStatusLabels } from "../lib/extraction";
import type { AppSettings, ExtractorStatus, JobSummary } from "../types";

interface Props {
  settings: AppSettings;
  databasePath: string;
  saveSettings: (settings: AppSettings) => Promise<AppSettings>;
}

export function SettingsPage({ settings, databasePath, saveSettings }: Props) {
  const [form, setForm] = useState(settings);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => setForm(settings), [settings]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const normalized = form.localOnly ? { ...form, allowExternalProcessing: false } : form;
      await saveSettings(normalized);
      setForm(normalized);
      setMessage("Configuración guardada");
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form className="mx-auto max-w-[1000px] space-y-5 p-8" onSubmit={submit}>
      <SettingsSection icon={HardDrive} title="Perfil local" description="Identidad usada en el historial de decisiones.">
        <div className="grid grid-cols-3 gap-4">
          <label className="label">Nombre del investigador
            <input className="input mt-1.5" value={form.researcherName} onChange={(event) => setForm({ ...form, researcherName: event.target.value })} placeholder="Nombre opcional" />
          </label>
          <label className="label">Idioma predeterminado
            <select className="input mt-1.5" value={form.defaultLanguage} onChange={(event) => setForm({ ...form, defaultLanguage: event.target.value as "es" | "en" })}>
              <option value="es">Español</option><option value="en">English</option>
            </select>
          </label>
          <label className="label">Apariencia
            <select className="input mt-1.5" value={form.appearance} onChange={(event) => setForm({ ...form, appearance: event.target.value as AppSettings["appearance"] })}>
              <option value="system">Sistema</option><option value="light">Claro</option><option value="dark">Oscuro</option>
            </select>
          </label>
        </div>
      </SettingsSection>

      <SettingsSection icon={ShieldCheck} title="Privacidad" description="Control explícito del procesamiento fuera del equipo.">
        <Toggle
          checked={form.localOnly}
          onChange={(checked) => setForm({ ...form, localOnly: checked, allowExternalProcessing: checked ? false : form.allowExternalProcessing })}
          title="Modo solo local"
          detail="Impide autorizar proveedores externos mientras está activo."
        />
        <Toggle
          checked={form.allowExternalProcessing}
          disabled={form.localOnly}
          onChange={(checked) => setForm({ ...form, allowExternalProcessing: checked })}
          title="Permitir procesamiento externo"
          detail="Autorización global; cada flujo deberá volver a confirmar el envío."
        />
      </SettingsSection>

      <SettingsSection icon={Sparkles} title="Inteligencia artificial" description="Ollama es el proveedor local predeterminado.">
        <div className="grid grid-cols-3 gap-4">
          <label className="label">Proveedor
            <select className="input mt-1.5" value={form.aiProvider} onChange={(event) => setForm({ ...form, aiProvider: event.target.value as AppSettings["aiProvider"] })}>
              <option value="ollama">Ollama (local)</option><option value="openai">OpenAI</option><option value="anthropic">Anthropic</option>
            </select>
          </label>
          <label className="label">URL de Ollama
            <input className="input mt-1.5" value={form.ollamaBaseUrl} onChange={(event) => setForm({ ...form, ollamaBaseUrl: event.target.value })} />
          </label>
          <label className="label">Modelo local
            <input className="input mt-1.5" value={form.ollamaModel} onChange={(event) => setForm({ ...form, ollamaModel: event.target.value })} placeholder="Se descubrirá desde Ollama" />
          </label>
        </div>
        {form.aiProvider !== "ollama" && !form.allowExternalProcessing && (
          <p className="mt-3 rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800">El proveedor está seleccionado, pero los envíos externos siguen bloqueados.</p>
        )}
      </SettingsSection>

      <SettingsSection icon={ScanText} title="Extracción científica" description="Sidecar Python local: diseño de página, OCR opcional y GROBID opcional en este equipo.">
        <ExtractorDiagnostics />
        <div className="mt-5 border-t border-slate-100 pt-4">
          <Toggle checked={form.autoExtractOnImport} onChange={(checked) => setForm({ ...form, autoExtractOnImport: checked })} title="Extraer al importar" detail="Encola la ficha científica de cada PDF nuevo. Los resultados son propuestas que requieren validación." />
          <Toggle checked={form.ocrEnabled} onChange={(checked) => setForm({ ...form, ocrEnabled: checked })} title="OCR local para páginas escaneadas" detail="Solo se aplica a páginas sin texto. El texto reconocido puede contener errores y se marca para revisión." />
          <Toggle checked={form.grobidEnabled} onChange={(checked) => setForm({ ...form, grobidEnabled: checked })} title="Usar GROBID" detail="Servicio opcional que debe ejecutarse en este equipo (por ejemplo con Docker). La consolidación externa está desactivada." />
        </div>
        <div className="mt-4 grid grid-cols-3 gap-4">
          <label className="label">Idiomas de OCR
            <input className="input mt-1.5" value={form.ocrLanguages} onChange={(event) => setForm({ ...form, ocrLanguages: event.target.value })} placeholder="eng+spa" />
          </label>
          <label className="label">URL de GROBID
            <input className="input mt-1.5" value={form.grobidUrl} onChange={(event) => setForm({ ...form, grobidUrl: event.target.value })} />
          </label>
          <label className="label">Tiempo máximo por documento (s)
            <input className="input mt-1.5" type="number" min={30} max={7200} value={form.extractionTimeoutSeconds} onChange={(event) => setForm({ ...form, extractionTimeoutSeconds: Number(event.target.value) })} />
          </label>
        </div>
        <label className="label mt-4">Intérprete de Python (opcional)
          <input className="input mt-1.5 font-mono" value={form.extractorPythonPath} onChange={(event) => setForm({ ...form, extractorPythonPath: event.target.value })} placeholder="Vacío: entorno de la aplicación o Python del sistema" />
        </label>
      </SettingsSection>

      <SettingsSection icon={Database} title="Almacenamiento" description="Base persistente administrada por el núcleo Rust.">
        <div className="rounded-lg bg-slate-50 px-3 py-2.5 font-mono text-[11px] text-slate-500 break-all">{databasePath}</div>
      </SettingsSection>

      <div className="flex items-center justify-end gap-3">
        {message && <span className="flex items-center gap-1.5 text-xs font-medium text-emerald-700"><Check className="size-4" />{message}</span>}
        {error && <span className="text-xs font-medium text-rose-700">{error}</span>}
        <button className="button-primary" disabled={saving}>{saving && <LoaderCircle className="size-4 animate-spin" />} Guardar cambios</button>
      </div>
    </form>
  );
}

function SettingsSection({ icon: Icon, title, description, children }: { icon: typeof HardDrive; title: string; description: string; children: React.ReactNode }) {
  return (
    <section className="panel p-6">
      <div className="flex gap-4">
        <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-teal-50 text-teal-700"><Icon className="size-4" /></div>
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-semibold text-slate-800">{title}</h2>
          <p className="mt-0.5 text-xs text-slate-400">{description}</p>
          <div className="mt-5">{children}</div>
        </div>
      </div>
    </section>
  );
}

function Toggle({ checked, onChange, title, detail, disabled = false }: { checked: boolean; onChange: (value: boolean) => void; title: string; detail: string; disabled?: boolean }) {
  return (
    <label className={`flex items-center justify-between border-b border-slate-100 py-3 first:pt-0 last:border-0 last:pb-0 ${disabled ? "opacity-45" : ""}`}>
      <span><span className="block text-xs font-semibold text-slate-700">{title}</span><span className="mt-0.5 block text-[11px] text-slate-400">{detail}</span></span>
      <button type="button" role="switch" aria-checked={checked} disabled={disabled} onClick={() => onChange(!checked)} className={`relative h-6 w-11 rounded-full transition ${checked ? "bg-teal-500" : "bg-slate-200"}`}>
        <span className={`absolute top-1 size-4 rounded-full bg-white shadow-sm transition ${checked ? "left-6" : "left-1"}`} />
      </button>
    </label>
  );
}

const pythonSources: Record<NonNullable<ExtractorStatus["pythonSource"]>, string> = {
  override: "ruta configurada",
  app_environment: "entorno de la aplicación",
  development: "entorno de desarrollo",
  system: "Python del sistema",
};

function ExtractorDiagnostics() {
  const [status, setStatus] = useState<ExtractorStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [setupJob, setSetupJob] = useState<JobSummary | null>(null);

  const check = useCallback(async () => {
    setChecking(true);
    setError(null);
    try {
      setStatus(await backend.extractorStatus());
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => {
    void check();
    return onJobChanged((job) => {
      if (job.jobType !== "extractor_setup") return;
      setSetupJob(job);
      if (job.status === "completed") void check();
    });
  }, [check]);

  async function prepare() {
    setConfirming(false);
    setError(null);
    try {
      setSetupJob(await backend.prepareExtractorEnvironment());
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  const capabilities = status?.capabilities;
  const setupRunning = setupJob && (setupJob.status === "queued" || setupJob.status === "running");

  return (
    <div className="rounded-xl bg-slate-50 p-4">
      <div className="flex items-start justify-between gap-4">
        <dl className="grid flex-1 grid-cols-[130px_1fr] gap-x-3 gap-y-1.5 text-[11px]">
          <dt className="text-slate-400">Python</dt>
          <dd className="break-all text-slate-600">{status?.pythonPath ? `${capabilities?.python ?? ""} · ${pythonSources[status.pythonSource ?? "system"]}` : checking ? "Comprobando…" : "No encontrado"}</dd>
          <dt className="text-slate-400">Análisis de diseño</dt>
          <dd className="text-slate-600">{capabilities ? (capabilities.pdfplumber ? `pdfplumber ${capabilities.pdfplumber}` : "No instalado (modo texto básico, sin coordenadas)") : "—"}</dd>
          <dt className="text-slate-400">OCR</dt>
          <dd className="text-slate-600">{capabilities ? (capabilities.tesseract.available ? `${capabilities.tesseract.version ?? "Tesseract"} · idiomas: ${capabilities.tesseract.languages.join(", ") || "ninguno"}` : "Tesseract no instalado (brew install tesseract tesseract-lang)") : "—"}</dd>
          {capabilities?.grobid && (<><dt className="text-slate-400">GROBID</dt><dd className={capabilities.grobid.reachable ? "text-emerald-700" : "text-amber-700"}>{capabilities.grobid.reachable ? "Disponible" : "No responde"} · {capabilities.grobid.url}</dd></>)}
          {capabilities && (<><dt className="text-slate-400">Extractor</dt><dd className="text-slate-600">{capabilities.workerVersion} · reglas {capabilities.rulesetVersion}</dd></>)}
        </dl>
        <button type="button" className="button-secondary shrink-0 px-3 py-2" onClick={() => void check()} disabled={checking}>
          {checking ? <LoaderCircle className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />} Comprobar
        </button>
      </div>
      {status?.message && <p className="mt-3 flex items-start gap-1.5 text-[11px] text-amber-800"><TriangleAlert className="mt-px size-3.5 shrink-0" />{status.message}</p>}
      {error && <p className="mt-3 text-[11px] text-rose-700">{error}</p>}

      {setupJob && (
        <p className="mt-3 text-[11px] text-slate-500">
          Preparación del entorno: <span className="font-semibold">{jobStatusLabels[setupJob.status]}</span>
          {setupJob.message ? ` · ${setupJob.message}` : ""}
          {setupJob.errorMessage ? <span className="block text-rose-700">{setupJob.errorMessage}</span> : null}
        </p>
      )}
      {capabilities && !capabilities.pdfplumber && !setupRunning && (
        confirming ? (
          <div className="mt-3 rounded-lg border border-amber-200 bg-amber-50 p-3 text-[11px] leading-4 text-amber-900">
            Se creará un entorno virtual en la carpeta de datos de la aplicación y se descargará <strong>pdfplumber 0.11.10</strong> (MIT) y sus dependencias desde PyPI. Requiere internet una sola vez; ningún documento se envía.
            <div className="mt-2 flex gap-2">
              <button type="button" className="button-primary px-3 py-1.5" onClick={() => void prepare()}>Confirmar descarga</button>
              <button type="button" className="button-secondary px-3 py-1.5" onClick={() => setConfirming(false)}>Cancelar</button>
            </div>
          </div>
        ) : (
          <button type="button" className="button-secondary mt-3 px-3 py-2" onClick={() => setConfirming(true)}><Download className="size-3.5" /> Preparar entorno del extractor</button>
        )
      )}
    </div>
  );
}
