import { useCallback, useEffect, useRef, useState } from "react";
import { backend, errorMessage } from "../lib/backend";
import { onJobChanged } from "../lib/events";
import { isActiveJob } from "../lib/extraction";
import type { JobSummary } from "../types";

/** Live job list: initial load, runner events, and polling while work is active. */
export function useJobs(onFinished?: (job: JobSummary) => void) {
  const [jobs, setJobs] = useState<JobSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const finished = useRef(onFinished);
  finished.current = onFinished;

  const refresh = useCallback(async () => {
    try {
      setJobs(await backend.listJobs());
      setError(null);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    return onJobChanged((job) => {
      setJobs((current) => {
        const index = current.findIndex((item) => item.id === job.id);
        if (index === -1) return [job, ...current];
        const next = [...current];
        next[index] = job;
        return next;
      });
      if (!isActiveJob(job.status)) finished.current?.(job);
    });
  }, [refresh]);

  const hasActive = jobs.some((job) => isActiveJob(job.status));
  useEffect(() => {
    if (!hasActive) return;
    const timer = window.setInterval(() => void refresh(), 4000);
    return () => window.clearInterval(timer);
  }, [hasActive, refresh]);

  const apply = async (action: Promise<JobSummary>) => {
    const job = await action;
    setJobs((current) => current.map((item) => (item.id === job.id ? job : item)));
    return job;
  };

  return {
    jobs,
    loading,
    error,
    refresh,
    pause: (jobId: string) => apply(backend.pauseJob(jobId)),
    resume: (jobId: string) => apply(backend.resumeJob(jobId)),
    cancel: (jobId: string) => apply(backend.cancelJob(jobId)),
    retry: (jobId: string) => apply(backend.retryJob(jobId)),
  };
}
