import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { HashRouter, Navigate, Route, Routes, useParams } from "react-router-dom";
import { AppShell } from "./components/AppShell";
import { ErrorView, LoadingView } from "./components/StateViews";
import { useJobs } from "./hooks/useJobs";
import { useResearchPilot } from "./hooks/useResearchPilot";
import { I18nProvider } from "./i18n/I18nProvider";
import { maintenance, type RestoreNotice } from "./lib/maintenance";
import { checkForUpdate } from "./lib/updates";
import { DashboardPage } from "./pages/DashboardPage";
import { EvidencePage } from "./pages/EvidencePage";
import { LibraryPage } from "./pages/LibraryPage";
import { ProjectWorkspacePage } from "./pages/ProjectWorkspacePage";
import { ProtocolPage } from "./pages/ProtocolPage";
import { ReviewsPage } from "./pages/ReviewsPage";
import { AssistantPage } from "./pages/AssistantPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TasksPage } from "./pages/TasksPage";
import "./styles.css";

const ArticleViewerPage = lazy(() =>
  import("./pages/ArticleViewerPage").then((module) => ({ default: module.ArticleViewerPage })),
);

export default function App() {
  const app = useResearchPilot();
  // Finished jobs change dashboard counters (evidence, values to review).
  const jobs = useJobs(() => void app.refreshQuietly());
  const appearance = app.data?.settings.appearance ?? "system";
  const [updateVersion, setUpdateVersion] = useState<string | null>(null);
  const checkedUpdates = useRef(false);
  const [restoreNotice, setRestoreNotice] = useState<RestoreNotice | null>(null);

  // A restore applied at start-up is reported once.
  useEffect(() => {
    maintenance.takeRestoreNotice().then(setRestoreNotice).catch(() => undefined);
  }, []);

  // Optional check at start-up; never in local-only mode and at most once per session.
  useEffect(() => {
    const settings = app.data?.settings;
    if (!settings || checkedUpdates.current || !settings.checkUpdatesOnStart || settings.localOnly) return;
    checkedUpdates.current = true;
    checkForUpdate().then((update) => setUpdateVersion(update?.version ?? null)).catch(() => undefined);
  }, [app.data?.settings]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const applyTheme = () => {
      const theme = appearance === "system" ? (media.matches ? "dark" : "light") : appearance;
      document.documentElement.dataset.theme = theme;
      document.documentElement.style.colorScheme = theme;
    };
    applyTheme();
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [appearance]);

  return (
    <I18nProvider language={app.data?.settings.defaultLanguage ?? "es"}>
      <HashRouter>
      <Routes>
        <Route element={<AppShell updateVersion={updateVersion} restoreNotice={restoreNotice} onDismissRestoreNotice={() => setRestoreNotice(null)} />}>
          {app.loading && !app.data ? (
            <Route path="*" element={<LoadingView />} />
          ) : app.error || !app.data ? (
            <Route path="*" element={<ErrorView message={app.error ?? "Estado no disponible"} retry={app.refresh} />} />
          ) : (
            <>
              <Route path="/" element={<DashboardPage data={app.data} />} />
              <Route path="/biblioteca" element={<LibraryPage articles={app.data.articles} collections={app.data.collections} importPdfFiles={app.importPdfFiles} importReferenceFiles={app.importReferenceFiles} searchLibrary={app.searchLibrary} archiveArticle={app.archiveArticle} createCollection={app.createCollection} />} />
              <Route path="/biblioteca/:articleId" element={<ArticleRoute articles={app.data.articles} collections={app.data.collections} updateMetadata={app.updateArticleMetadata} setArticleCollection={app.setArticleCollection} refresh={app.refreshQuietly} />} />
              <Route path="/revisiones" element={<ReviewsPage projects={app.data.projects} createProject={app.createProject} onImported={() => void app.refreshQuietly()} />} />
              <Route path="/revisiones/:projectId" element={<ProjectWorkspacePage articles={app.data.articles} />} />
              <Route path="/protocolo" element={<ProtocolPage />} />
              <Route path="/evidencia" element={<EvidencePage stats={app.data.stats} />} />
              <Route path="/tareas" element={<TasksPage {...jobs} />} />
              <Route path="/asistente" element={<AssistantPage articles={app.data.articles} collections={app.data.collections} settings={app.data.settings} />} />
              <Route path="/configuracion" element={<SettingsPage settings={app.data.settings} databasePath={app.data.databasePath} saveSettings={app.saveSettings} onLibraryChanged={() => void app.refreshQuietly()} />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </>
          )}
        </Route>
      </Routes>
      </HashRouter>
    </I18nProvider>
  );
}

function ArticleRoute({ articles, collections, updateMetadata, setArticleCollection, refresh }: { refresh: () => Promise<void>; articles: import("./types").LibraryArticle[]; collections: import("./types").LibraryCollection[]; updateMetadata: (input: import("./types").UpdateArticleMetadataInput) => Promise<import("./types").LibraryArticle>; setArticleCollection: (articleId: string, collectionId: string, included: boolean) => Promise<void> }) {
  const { articleId } = useParams();
  const article = articles.find((item) => item.id === articleId);
  return article ? <Suspense fallback={<LoadingView />}><ArticleViewerPage article={article} collections={collections} updateMetadata={updateMetadata} setArticleCollection={setArticleCollection} refresh={refresh} /></Suspense> : <Navigate to="/biblioteca" replace />;
}
