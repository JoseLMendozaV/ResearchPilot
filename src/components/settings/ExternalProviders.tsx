import { useEffect, useState } from "react";
import { KeyRound, LoaderCircle, PlugZap, Trash2 } from "lucide-react";
import { backend, errorMessage } from "../../lib/backend";
import type { AppSettings, ExternalLog, ModelPrice, ProviderStatus } from "../../types";

const providers = [
  { id: "openai", name: "OpenAI", modelKey: "openaiModel", note: "La API de OpenAI se factura aparte de ChatGPT Plus/Pro." },
  { id: "anthropic", name: "Anthropic", modelKey: "anthropicModel", note: "La API de Anthropic se factura aparte de Claude Pro/Max." },
] as const;

interface Props {
  form: AppSettings;
  setForm: (settings: AppSettings) => void;
}

export function ExternalProviders({ form, setForm }: Props) {
  const [statuses, setStatuses] = useState<Record<string, ProviderStatus>>({});
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [models, setModels] = useState<Record<string, string[]>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [messages, setMessages] = useState<Record<string, string>>({});
  const [pricing, setPricing] = useState<ModelPrice[]>([]);
  const [newPrice, setNewPrice] = useState({ provider: "openai", model: "", input: "", output: "" });
  const blocked = form.localOnly || !form.allowExternalProcessing;

  useEffect(() => {
    backend.providerStatuses().then((items) => setStatuses(Object.fromEntries(items.map((item) => [item.provider, item])))).catch(() => undefined);
    backend.listModelPricing().then(setPricing).catch(() => undefined);
  }, []);

  async function run(provider: string, action: () => Promise<void>) {
    setBusy(provider);
    setMessages((current) => ({ ...current, [provider]: "" }));
    try {
      await action();
    } catch (cause) {
      setMessages((current) => ({ ...current, [provider]: errorMessage(cause) }));
    } finally {
      setBusy(null);
    }
  }

  const saveKey = (provider: string) => run(provider, async () => {
    const status = await backend.setApiKey(provider, keys[provider] ?? "");
    setStatuses((current) => ({ ...current, [provider]: status }));
    setKeys((current) => ({ ...current, [provider]: "" }));
    setMessages((current) => ({ ...current, [provider]: "Clave guardada en el Llavero de macOS." }));
  });
  const removeKey = (provider: string) => run(provider, async () => {
    const status = await backend.deleteApiKey(provider);
    setStatuses((current) => ({ ...current, [provider]: status }));
  });
  const test = (provider: string) => run(provider, async () => {
    const list = await backend.testProvider(provider);
    setModels((current) => ({ ...current, [provider]: list }));
    setMessages((current) => ({ ...current, [provider]: `Conexión correcta · ${list.length} modelo(s) disponibles.` }));
  });

  async function addPrice() {
    try {
      setPricing(await backend.setModelPricing(newPrice.provider, newPrice.model, Number(newPrice.input), Number(newPrice.output)));
      setNewPrice({ ...newPrice, model: "", input: "", output: "" });
    } catch (cause) {
      setMessages((current) => ({ ...current, pricing: errorMessage(cause) }));
    }
  }

  return (
    <div className="space-y-4">
      {blocked && (
        <p className="rounded-lg bg-slate-50 px-3 py-2 text-[11px] text-slate-500">
          Los proveedores externos están bloqueados: {form.localOnly ? "el modo solo local está activo" : "el procesamiento externo no está permitido"}. Puedes guardar claves, pero no se realiza ninguna conexión.
        </p>
      )}
      <div className="grid grid-cols-2 gap-4">
        {providers.map((provider) => {
          const status = statuses[provider.id];
          const available = models[provider.id] ?? [];
          const current = form[provider.modelKey];
          return (
            <div key={provider.id} className="rounded-xl border border-slate-100 p-4">
              <div className="flex items-center justify-between">
                <p className="text-xs font-semibold text-slate-700">{provider.name}</p>
                <span className={`text-[10px] font-semibold ${status?.hasKey ? "text-emerald-700" : "text-slate-400"}`}>{status?.hasKey ? `Clave en el Llavero ${status.keyHint ?? ""}` : "Sin clave"}</span>
              </div>
              <p className="mt-1 text-[10px] text-slate-400">{provider.note}</p>
              <div className="mt-3 flex gap-1.5">
                <input className="input font-mono" type="password" autoComplete="off" placeholder="Pega la clave de API" value={keys[provider.id] ?? ""} onChange={(event) => setKeys((current) => ({ ...current, [provider.id]: event.target.value }))} aria-label={`Clave de API de ${provider.name}`} />
                <button type="button" className="button-secondary shrink-0 px-2.5" disabled={!keys[provider.id] || busy === provider.id} onClick={() => void saveKey(provider.id)}><KeyRound className="size-3.5" /> Guardar</button>
              </div>
              <div className="mt-2 flex gap-1.5">
                <button type="button" className="button-secondary px-2.5 py-1.5 text-[11px]" disabled={!status?.hasKey || blocked || busy === provider.id} onClick={() => void test(provider.id)}>
                  {busy === provider.id ? <LoaderCircle className="size-3 animate-spin" /> : <PlugZap className="size-3" />} Probar conexión
                </button>
                {status?.hasKey && <button type="button" className="button-secondary px-2.5 py-1.5 text-[11px]" onClick={() => void removeKey(provider.id)}><Trash2 className="size-3" /> Eliminar clave</button>}
              </div>
              <label className="label mt-3">Modelo
                {available.length ? (
                  <select className="input mt-1" value={current} onChange={(event) => setForm({ ...form, [provider.modelKey]: event.target.value })}>
                    <option value="">Sin seleccionar</option>
                    {current && !available.includes(current) && <option value={current}>{current}</option>}
                    {available.map((model) => <option key={model} value={model}>{model}</option>)}
                  </select>
                ) : (
                  <input className="input mt-1 font-mono" value={current} placeholder="Prueba la conexión para listar modelos" onChange={(event) => setForm({ ...form, [provider.modelKey]: event.target.value })} />
                )}
              </label>
              {messages[provider.id] && <p className="mt-2 text-[11px] text-slate-500">{messages[provider.id]}</p>}
            </div>
          );
        })}
      </div>

      <div className="grid grid-cols-2 gap-4">
        <label className="label">Presupuesto mensual externo (USD, 0 = sin límite)
          <input className="input mt-1.5" type="number" min={0} step={0.5} value={form.externalMonthlyBudgetUsd} onChange={(event) => setForm({ ...form, externalMonthlyBudgetUsd: Number(event.target.value) })} />
        </label>
        <label className="label">Máximo de tokens de salida por solicitud
          <input className="input mt-1.5" type="number" min={256} max={64000} step={256} value={form.externalMaxOutputTokens} onChange={(event) => setForm({ ...form, externalMaxOutputTokens: Number(event.target.value) })} />
        </label>
      </div>

      <details className="rounded-xl border border-slate-100 p-3 text-[11px]">
        <summary className="cursor-pointer font-semibold text-slate-600">Precios por millón de tokens ({pricing.length})</summary>
        <p className="mt-2 text-slate-400">Los precios «de referencia» tienen fecha; corrígelos con los de tu cuenta. Sin precio no se puede estimar ni limitar el coste.</p>
        <table className="mt-2 w-full text-left">
          <thead className="text-slate-400"><tr><th className="py-1 font-medium">Modelo</th><th className="font-medium">Entrada</th><th className="font-medium">Salida</th><th className="font-medium">Origen</th></tr></thead>
          <tbody className="text-slate-600">{pricing.map((price) => <tr key={`${price.provider}-${price.model}`} className="border-t border-slate-100"><td className="py-1 font-mono">{price.model}</td><td>${price.inputUsdPerMtok}</td><td>${price.outputUsdPerMtok}</td><td>{price.source === "reference" ? `referencia ${price.updatedAt}` : "tuyo"}</td></tr>)}</tbody>
        </table>
        <div className="mt-2 grid grid-cols-[110px_1fr_80px_80px_auto] gap-1.5">
          <select className="input" value={newPrice.provider} onChange={(event) => setNewPrice({ ...newPrice, provider: event.target.value })}><option value="openai">OpenAI</option><option value="anthropic">Anthropic</option></select>
          <input className="input font-mono" placeholder="modelo" value={newPrice.model} onChange={(event) => setNewPrice({ ...newPrice, model: event.target.value })} />
          <input className="input" placeholder="entrada $" inputMode="decimal" value={newPrice.input} onChange={(event) => setNewPrice({ ...newPrice, input: event.target.value })} />
          <input className="input" placeholder="salida $" inputMode="decimal" value={newPrice.output} onChange={(event) => setNewPrice({ ...newPrice, output: event.target.value })} />
          <button type="button" className="button-secondary px-2.5" disabled={!newPrice.model || !newPrice.input || !newPrice.output} onClick={() => void addPrice()}>Guardar</button>
        </div>
        {messages.pricing && <p className="mt-1 text-rose-700">{messages.pricing}</p>}
      </details>
    </div>
  );
}

export function ExternalLogPanel() {
  const [log, setLog] = useState<ExternalLog | null>(null);
  useEffect(() => {
    backend.externalLog(50).then(setLog).catch(() => undefined);
  }, []);
  if (!log) return null;
  return (
    <div className="mt-4 rounded-xl bg-slate-50 p-3 text-[11px]">
      <p className="font-semibold text-slate-700">Registro de envíos externos: {log.totalRequests} solicitud(es) · {log.monthSpentUsd.toFixed(4)} USD este mes</p>
      {log.totalRequests === 0 ? (
        <p className="mt-1 text-slate-500">Ningún contenido ha salido de este equipo hacia proveedores de IA.</p>
      ) : (
        <ul className="mt-2 max-h-40 space-y-1 overflow-y-auto">
          {log.entries.map((entry, index) => (
            <li key={`${entry.createdAt}-${index}`} className="text-slate-500">
              {new Date(entry.createdAt).toLocaleString("es")} · {entry.provider} {entry.model} · {entry.operation}{entry.articleTitle ? ` · ${entry.articleTitle}` : ""} · {entry.status} · {(entry.promptTokens ?? 0) + (entry.completionTokens ?? 0)} tokens{entry.costUsd !== undefined && entry.costUsd !== null ? ` · ${entry.costUsd.toFixed(4)} USD` : ""}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
