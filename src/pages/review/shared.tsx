import { useCallback, useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { errorMessage } from "../../lib/backend";
import type { Decision, Stage } from "../../reviewTypes";

export const decisionLabels: Record<Decision, string> = {
  include: "Incluir",
  exclude: "Excluir",
  maybe: "Duda",
  awaiting_full_text: "Esperar texto completo",
};

export const decisionTones: Record<Decision, string> = {
  include: "bg-emerald-50 text-emerald-700",
  exclude: "bg-rose-50 text-rose-700",
  maybe: "bg-amber-50 text-amber-800",
  awaiting_full_text: "bg-sky-50 text-sky-700",
};

export const stageLabels: Record<Stage, string> = {
  identified: "Identificado",
  screening: "En cribado",
  eligible: "Elegible (texto completo)",
  included: "Incluido",
  excluded: "Excluido",
};

/** Load data with loading/error state and a reload function. */
export function useLoad<T>(load: () => Promise<T>, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const reload = useCallback(async () => {
    setLoading(true);
    try {
      setData(await load());
      setError(null);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }, deps);
  useEffect(() => {
    void reload();
  }, [reload]);
  return { data, setData, error, setError, loading, reload };
}

/** Run an action, reporting its error instead of throwing. */
export function useAction() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async <T,>(action: () => Promise<T>): Promise<T | undefined> => {
    setBusy(true);
    setError(null);
    try {
      return await action();
    } catch (cause) {
      setError(errorMessage(cause));
      return undefined;
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, setError, run };
}

export function Loading() {
  return <div className="grid h-40 place-items-center"><LoaderCircle className="size-5 animate-spin text-teal-600" /></div>;
}

export function ErrorNote({ message }: { message?: string | null }) {
  return message ? <p className="rounded-lg bg-rose-50 px-3 py-2 text-xs text-rose-700">{message}</p> : null;
}

export function Empty({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="rounded-xl border border-dashed border-slate-200 px-5 py-8 text-center">
      <p className="text-sm font-medium text-slate-600">{title}</p>
      <p className="mt-1 text-xs text-slate-400">{detail}</p>
    </div>
  );
}

export function Card({ title, actions, children }: { title: string; actions?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="panel p-5">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold text-slate-800">{title}</h3>
        {actions && <div className="flex items-center gap-2">{actions}</div>}
      </div>
      <div className="mt-4">{children}</div>
    </section>
  );
}
