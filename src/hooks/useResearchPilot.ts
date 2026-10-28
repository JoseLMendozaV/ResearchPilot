import { useCallback, useEffect, useState } from "react";
import { backend, errorMessage } from "../lib/backend";
import type {
  AppSettings,
  BootstrapData,
  CreateProjectInput,
  UpdateArticleMetadataInput,
} from "../types";

export function useResearchPilot() {
  const [data, setData] = useState<BootstrapData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setData(await backend.bootstrap());
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const createProject = async (input: CreateProjectInput) => {
    await backend.createProject(input);
    await refresh();
  };

  const saveSettings = async (settings: AppSettings) => {
    const saved = await backend.saveSettings(settings);
    setData((current) => (current ? { ...current, settings: saved } : current));
    return saved;
  };

  const importPdfFiles = async (paths: string[]) => {
    const result = await backend.importPdfFiles(paths);
    await refresh();
    return result;
  };

  const importReferenceFiles = async (paths: string[]) => {
    const result = await backend.importReferenceFiles(paths);
    await refresh();
    return result;
  };

  const updateArticleMetadata = async (input: UpdateArticleMetadataInput) => {
    const result = await backend.updateArticleMetadata(input);
    await refresh();
    return result;
  };

  const archiveArticle = async (articleId: string) => {
    await backend.archiveArticle(articleId);
    await refresh();
  };

  const createCollection = async (name: string) => {
    const result = await backend.createCollection(name);
    await refresh();
    return result;
  };

  const setArticleCollection = async (articleId: string, collectionId: string, included: boolean) => {
    await backend.setArticleCollection(articleId, collectionId, included);
    await refresh();
  };

  return {
    data,
    loading,
    error,
    refresh,
    createProject,
    saveSettings,
    importPdfFiles,
    importReferenceFiles,
    searchLibrary: backend.searchLibrary,
    updateArticleMetadata,
    archiveArticle,
    createCollection,
    setArticleCollection,
  };
}
