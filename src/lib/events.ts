import { listen } from "@tauri-apps/api/event";
import type { JobSummary } from "../types";

/** Subscribe to job updates emitted by the Rust runner. Returns an unsubscribe function. */
export function onJobChanged(callback: (job: JobSummary) => void): () => void {
  let unlisten: (() => void) | undefined;
  let disposed = false;
  listen<JobSummary>("jobs://changed", (event) => callback(event.payload))
    .then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    })
    .catch(() => {
      // Outside Tauri (tests, plain browser) there is no event bridge; polling covers it.
    });
  return () => {
    disposed = true;
    unlisten?.();
  };
}
