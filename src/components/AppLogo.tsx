export function AppLogo({ compact = false }: { compact?: boolean }) {
  return (
    <div className="flex items-center gap-3">
      <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-teal-500 text-white shadow-[0_6px_20px_rgba(20,184,166,0.3)]">
        <svg aria-hidden="true" viewBox="0 0 32 32" className="size-6" fill="none">
          <path d="M7 23.5 16 6l9 17.5" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
          <path d="M10.2 18h11.6M13 23.5h6" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
          <circle cx="16" cy="12" r="2.2" fill="currentColor" />
        </svg>
      </div>
      {!compact && (
        <div>
          <p className="text-[17px] font-semibold tracking-[-0.02em] text-slate-900">ResearchPilot</p>
          <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-slate-400">Evidence workspace</p>
        </div>
      )}
    </div>
  );
}
