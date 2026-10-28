import {
  BookOpen,
  FileSearch,
  FlaskConical,
  FolderKanban,
  LayoutDashboard,
  ListChecks,
  MessageSquareText,
  Settings,
  ShieldCheck,
} from "lucide-react";
import { NavLink, Outlet, useLocation } from "react-router-dom";
import { useI18n, type MessageKey } from "../i18n/I18nProvider";
import type { RestoreNotice } from "../lib/maintenance";
import { AppLogo } from "./AppLogo";

const navigation = [
  { to: "/", label: "nav.home" as MessageKey, icon: LayoutDashboard, end: true },
  { to: "/biblioteca", label: "nav.library" as MessageKey, icon: BookOpen },
  { to: "/revisiones", label: "nav.reviews" as MessageKey, icon: FolderKanban },
  { to: "/protocolo", label: "nav.protocol" as MessageKey, icon: FlaskConical },
  { to: "/evidencia", label: "nav.evidence" as MessageKey, icon: FileSearch },
  { to: "/asistente", label: "nav.assistant" as MessageKey, icon: MessageSquareText },
  { to: "/tareas", label: "nav.tasks" as MessageKey, icon: ListChecks },
];

const titles: Record<string, { title: MessageKey; subtitle: MessageKey }> = {
  "/": { title: "header.home.title", subtitle: "header.home.subtitle" },
  "/biblioteca": { title: "header.library.title", subtitle: "header.library.subtitle" },
  "/revisiones": { title: "header.reviews.title", subtitle: "header.reviews.subtitle" },
  "/protocolo": { title: "header.protocol.title", subtitle: "header.protocol.subtitle" },
  "/evidencia": { title: "header.evidence.title", subtitle: "header.evidence.subtitle" },
  "/tareas": { title: "header.tasks.title", subtitle: "header.tasks.subtitle" },
  "/asistente": { title: "header.assistant.title", subtitle: "header.assistant.subtitle" },
  "/configuracion": { title: "header.settings.title", subtitle: "header.settings.subtitle" },
};

export function AppShell({ updateVersion, restoreNotice, onDismissRestoreNotice }: { updateVersion?: string | null; restoreNotice?: RestoreNotice | null; onDismissRestoreNotice?: () => void }) {
  const location = useLocation();
  const heading = location.pathname.startsWith("/biblioteca/")
    ? { title: "header.library.title" as MessageKey, subtitle: "header.library.subtitle" as MessageKey }
    : location.pathname.startsWith("/revisiones/")
    ? { title: "header.reviews.title" as MessageKey, subtitle: "header.reviews.subtitle" as MessageKey }
    : titles[location.pathname] ?? titles["/"];
  const t = useI18n();

  return (
    <div className="app-shell flex h-screen min-h-[720px] bg-[#f5f7f8] text-slate-900">
      <aside className="app-sidebar flex w-[248px] shrink-0 flex-col border-r border-slate-200/80 bg-white px-4 py-5">
        <div className="px-2 pb-7"><AppLogo /></div>
        <nav className="space-y-1" aria-label="Navegación principal">
          {navigation.map(({ to, label, icon: Icon, end }) => (
            <NavLink
              key={to}
              to={to}
              end={end}
              className={({ isActive }) =>
                `group flex items-center gap-3 rounded-xl px-3 py-2.5 text-sm font-medium transition ${
                  isActive ? "bg-teal-50 text-teal-800" : "text-slate-500 hover:bg-slate-50 hover:text-slate-800"
                }`
              }
            >
              <Icon className="size-[18px]" strokeWidth={1.8} />
              {t(label)}
            </NavLink>
          ))}
        </nav>
        <div className="mt-auto space-y-3">
          <div className="rounded-xl border border-emerald-100 bg-emerald-50/70 p-3">
            <div className="flex items-center gap-2 text-xs font-semibold text-emerald-800">
              <ShieldCheck className="size-4" /> {t("shell.privacy")}
            </div>
            <p className="mt-1.5 text-[11px] leading-4 text-emerald-700/80">
              {t("shell.privacyDetail")}
            </p>
          </div>
          <NavLink
            to="/configuracion"
            className={({ isActive }) =>
              `flex items-center gap-3 rounded-xl px-3 py-2.5 text-sm font-medium transition ${
                isActive ? "bg-teal-50 text-teal-800" : "text-slate-500 hover:bg-slate-50"
              }`
            }
          >
            <Settings className="size-[18px]" strokeWidth={1.8} /> {t("nav.settings")}
          </NavLink>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="app-header flex h-[82px] shrink-0 items-center justify-between border-b border-slate-200/80 bg-white/90 px-8 backdrop-blur">
          <div>
            <h1 className="text-xl font-semibold tracking-[-0.025em] text-slate-900">{t(heading.title)}</h1>
            <p className="mt-0.5 text-xs text-slate-400">{t(heading.subtitle)}</p>
          </div>
          <div className="flex items-center gap-2">
          {updateVersion && (
            <NavLink to="/configuracion" className="rounded-full bg-teal-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-teal-500">
              Actualización {updateVersion} disponible
            </NavLink>
          )}
          <div className="flex items-center gap-2 rounded-full border border-slate-200 bg-slate-50 px-3 py-1.5 text-xs font-medium text-slate-500">
            <span className="size-2 rounded-full bg-emerald-500 shadow-[0_0_0_3px_rgba(34,197,94,0.12)]" />
            {t("shell.database")}
          </div>
          </div>
        </header>
        {restoreNotice && (
          <div role="status" className={`flex items-start justify-between gap-4 border-b px-8 py-2.5 text-xs ${restoreNotice.status === "restored" ? "border-teal-100 bg-teal-50 text-teal-900" : "border-rose-100 bg-rose-50 text-rose-900"}`}>
            <p>
              {restoreNotice.status === "restored"
                ? `Se restauró el respaldo del ${new Date(restoreNotice.outcome.backupCreatedAt).toLocaleString("es")}.${restoreNotice.outcome.preRestoreBackup ? ` El estado anterior quedó guardado en ${restoreNotice.outcome.preRestoreBackup}.` : ""}`
                : `No se pudo aplicar la restauración; tus datos anteriores se conservaron. ${restoreNotice.message}`}
            </p>
            <button type="button" className="font-semibold hover:underline" onClick={onDismissRestoreNotice}>Cerrar</button>
          </div>
        )}
        <main className="min-h-0 flex-1 overflow-y-auto"><Outlet /></main>
      </div>
    </div>
  );
}
