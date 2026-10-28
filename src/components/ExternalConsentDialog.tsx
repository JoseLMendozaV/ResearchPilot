import { CloudUpload, ShieldAlert, X } from "lucide-react";
import type { ExternalConsent, ExternalEstimate } from "../types";

const providerNames = { openai: "OpenAI", anthropic: "Anthropic" } as const;

function usd(value?: number) {
  return value === undefined ? "sin precio configurado" : `${value < 1 ? value.toFixed(4) : value.toFixed(2)} USD`;
}

/** Explicit, per-request authorization before content leaves the machine. */
export function ExternalConsentDialog({ estimate, purpose, onConfirm, onCancel }: { estimate: ExternalEstimate; purpose: string; onConfirm: (consent: ExternalConsent) => void; onCancel: () => void }) {
  const blocked = !estimate.withinBudget || (estimate.monthlyBudgetUsd > 0 && !estimate.pricingKnown) || estimate.requests === 0;
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-slate-900/40 p-6" role="dialog" aria-modal="true" aria-labelledby="consent-title">
      <div className="panel w-full max-w-lg p-6 shadow-xl">
        <div className="flex items-start justify-between gap-4">
          <h2 id="consent-title" className="flex items-center gap-2 text-sm font-semibold text-slate-800"><CloudUpload className="size-4 text-amber-600" /> Enviar a {providerNames[estimate.provider]}</h2>
          <button className="viewer-button size-7" aria-label="Cerrar" onClick={onCancel}><X className="size-4" /></button>
        </div>
        <p className="mt-2 text-xs leading-5 text-slate-500">{purpose} Se enviarán extractos de tus documentos a <strong>{providerNames[estimate.provider]}</strong> con el modelo <span className="font-mono">{estimate.model}</span>. La facturación de la API es independiente de cualquier suscripción web.</p>
        <dl className="mt-4 grid grid-cols-2 gap-x-4 gap-y-2 rounded-xl bg-slate-50 p-3 text-[11px]">
          <dt className="text-slate-400">Solicitudes</dt><dd className="text-slate-700">{estimate.requests} (hasta {estimate.maxOutputTokens.toLocaleString("es")} tokens de salida con reintentos)</dd>
          <dt className="text-slate-400">Extractos enviados</dt><dd className="text-slate-700">{estimate.excerpts} · ~{estimate.inputTokens.toLocaleString("es")} tokens de entrada</dd>
          <dt className="text-slate-400">Coste esperado</dt><dd className="text-slate-700">{usd(estimate.expectedCostUsd)}</dd>
          <dt className="text-slate-400">Coste máximo</dt><dd className="font-semibold text-slate-800">{usd(estimate.worstCaseCostUsd)}</dd>
          <dt className="text-slate-400">Gasto del mes</dt><dd className="text-slate-700">{estimate.monthSpentUsd.toFixed(2)} USD{estimate.monthlyBudgetUsd > 0 ? ` de ${estimate.monthlyBudgetUsd.toFixed(2)} USD` : " (sin presupuesto)"}</dd>
        </dl>
        {estimate.articles.length > 0 && (
          <div className="mt-3 text-[11px] text-slate-500">
            <p className="font-semibold text-slate-600">Artículos incluidos</p>
            <ul className="mt-1 max-h-28 list-disc space-y-0.5 overflow-y-auto pl-4">{estimate.articles.map((article) => <li key={article.id}>{article.title}</li>)}</ul>
          </div>
        )}
        {estimate.excludedConfidential > 0 && <p className="mt-2 text-[11px] text-slate-500">{estimate.excludedConfidential} extracto(s) de artículos confidenciales se excluyeron y no se enviarán.</p>}
        {!estimate.pricingKnown && <p className="mt-2 flex items-start gap-1.5 text-[11px] text-amber-800"><ShieldAlert className="mt-px size-3.5 shrink-0" /> El modelo no tiene precio configurado: el coste no se puede estimar ni limitar. Agrégalo en Configuración.</p>}
        {!estimate.withinBudget && <p className="mt-2 text-[11px] font-semibold text-rose-700">El coste máximo superaría el presupuesto mensual configurado.</p>}
        {estimate.requests === 0 && <p className="mt-2 text-[11px] text-slate-500">No hay nada que enviar.</p>}
        <div className="mt-5 flex justify-end gap-2">
          <button className="button-secondary" onClick={onCancel}>Cancelar</button>
          <button className="button-primary bg-amber-600 hover:bg-amber-500" disabled={blocked} onClick={() => onConfirm({ provider: estimate.provider, model: estimate.model, maxCostUsd: estimate.worstCaseCostUsd })}>
            Autorizar este envío
          </button>
        </div>
      </div>
    </div>
  );
}
