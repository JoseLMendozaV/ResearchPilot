import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

export type { Update };

/** Ask the release feed for a newer signed build. Sends no documents or user data. */
export async function checkForUpdate(): Promise<Update | null> {
  return check();
}

/** Download, verify the signature and install; reports progress as 0..1 when the size is known. */
export async function installUpdate(update: Update, onProgress: (fraction: number | null) => void): Promise<void> {
  let total = 0;
  let received = 0;
  await update.downloadAndInstall((event) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? 0;
      onProgress(total ? 0 : null);
    } else if (event.event === "Progress") {
      received += event.data.chunkLength;
      onProgress(total ? Math.min(received / total, 1) : null);
    } else {
      onProgress(1);
    }
  });
}

export const currentVersion = () => getVersion();
export const restartApp = () => relaunch();
