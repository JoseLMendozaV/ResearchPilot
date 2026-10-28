import { createContext, useContext, type ReactNode } from "react";

const messages = {
  es: {
    "nav.home": "Inicio",
    "nav.library": "Biblioteca",
    "nav.reviews": "Revisiones",
    "nav.protocol": "Protocolo",
    "nav.evidence": "Evidencia",
    "nav.settings": "Configuración",
    "header.home.title": "Panel de investigación",
    "header.home.subtitle": "Resumen verificable de tu espacio de trabajo",
    "header.library.title": "Biblioteca",
    "header.library.subtitle": "Artículos científicos y documentos locales",
    "header.reviews.title": "Revisiones",
    "header.reviews.subtitle": "Protocolos y estudios secundarios",
    "header.protocol.title": "Flujo metodológico",
    "header.protocol.subtitle": "Trazabilidad desde el protocolo hasta la extracción",
    "header.evidence.title": "Evidencia",
    "header.evidence.subtitle": "Fragmentos, páginas y procedencia documental",
    "header.settings.title": "Configuración",
    "header.settings.subtitle": "Privacidad, identidad e inteligencia artificial",
    "shell.privacy": "Privacidad local",
    "shell.privacyDetail": "Ningún documento sale del equipo sin autorización explícita.",
    "shell.database": "SQLite local",
  },
  en: {
    "nav.home": "Home",
    "nav.library": "Library",
    "nav.reviews": "Reviews",
    "nav.protocol": "Protocol",
    "nav.evidence": "Evidence",
    "nav.settings": "Settings",
    "header.home.title": "Research dashboard",
    "header.home.subtitle": "A verifiable summary of your local workspace",
    "header.library.title": "Library",
    "header.library.subtitle": "Scientific papers and local documents",
    "header.reviews.title": "Reviews",
    "header.reviews.subtitle": "Protocols and secondary studies",
    "header.protocol.title": "Methodological workflow",
    "header.protocol.subtitle": "Traceability from protocol to extraction",
    "header.evidence.title": "Evidence",
    "header.evidence.subtitle": "Excerpts, pages, and document provenance",
    "header.settings.title": "Settings",
    "header.settings.subtitle": "Privacy, identity, and artificial intelligence",
    "shell.privacy": "Local privacy",
    "shell.privacyDetail": "No document leaves this device without explicit authorization.",
    "shell.database": "Local SQLite",
  },
} as const;

export type MessageKey = keyof typeof messages.es;
type Language = keyof typeof messages;

const I18nContext = createContext<(key: MessageKey) => string>((key) => messages.es[key]);

export function I18nProvider({ language, children }: { language: Language; children: ReactNode }) {
  const translate = (key: MessageKey) => messages[language][key];
  return <I18nContext.Provider value={translate}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  return useContext(I18nContext);
}
