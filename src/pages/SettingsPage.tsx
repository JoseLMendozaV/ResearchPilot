import { FormEvent, useEffect, useState } from "react";
import { Check, Database, HardDrive, LoaderCircle, ShieldCheck, Sparkles } from "lucide-react";
import { errorMessage } from "../lib/backend";
import type { AppSettings } from "../types";

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
