import type { DownloadEvent, Update } from "@tauri-apps/plugin-updater";

export type UpdateStatus = {
  phase: "idle" | "checking" | "current" | "available" | "downloading" | "installing" | "error";
  version?: string;
  notes?: string;
  progress?: number;
  checkedAt?: number;
  message?: string;
};

type Package = Pick<Update, "version" | "body" | "download" | "install" | "close">;

/** One rail-owned operation across both windows; native code verifies signatures. */
export function createUpdateController(
  check: () => Promise<Package | null>,
  publish: (state: UpdateStatus) => void,
) {
  let state: UpdateStatus = { phase: "idle" };
  let automatic = false;
  let disposed = false;
  let busy = false;
  let generation = 0;
  let pending: Package | null = null;

  const emit = (next: UpdateStatus) => {
    state = next;
    if (!disposed) publish(state);
  };
  const close = async (item: Package | null) => {
    try { await item?.close(); } catch { /* The window may already be closing. */ }
  };
  const installPackage = async (item: Package, allowed: () => boolean) => {
    let total = 0;
    let downloaded = 0;
    emit({ ...state, phase: "downloading", progress: undefined });
    await item.download((event: DownloadEvent) => {
      if (!allowed()) return;
      if (event.event === "Started") total = event.data.contentLength ?? 0;
      if (event.event === "Progress") downloaded += event.data.chunkLength;
      emit({ ...state, progress: total > 0 ? Math.min(100, Math.floor(downloaded / total * 100)) : undefined });
    }, { timeout: 120_000 });
    // Download may finish after opt-out or unmount. Never install in that case.
    if (!allowed()) return;
    emit({ ...state, phase: "installing", progress: 100 });
    await item.install();
  };
  const runCheck = async (background = false) => {
    if (disposed || busy || (background && !automatic)) return;
    busy = true;
    const revision = generation;
    const allowed = () => !disposed && (!background || (automatic && revision === generation));
    let item: Package | null = null;
    try {
      await close(pending);
      pending = null;
      if (!allowed()) return;
      emit({ phase: "checking", checkedAt: state.checkedAt });
      item = await check();
      if (!allowed()) return;
      const checkedAt = Date.now();
      if (!item) emit({ phase: "current", checkedAt });
      else {
        emit({ phase: "available", version: item.version, notes: item.body, checkedAt });
        if (background) await installPackage(item, allowed);
        else { pending = item; item = null; }
      }
    } catch {
      if (allowed()) emit({ phase: "error", checkedAt: state.checkedAt,
        message: "Could not complete the update. Your installed version is unchanged. Check your connection and try again." });
    } finally {
      await close(item);
      busy = false;
      if (!disposed && !allowed()) emit({ phase: "idle", checkedAt: state.checkedAt });
    }
  };
  return {
    snapshot: () => state,
    check: runCheck,
    setAutomatic(enabled: boolean) {
      if (automatic === enabled || disposed) return;
      automatic = enabled;
      generation++;
      if (enabled) void runCheck(true);
    },
    async install() {
      if (disposed || busy || !pending) return;
      busy = true;
      const item = pending;
      pending = null;
      try { await installPackage(item, () => !disposed); }
      catch {
        if (!disposed) emit({ phase: "error", checkedAt: state.checkedAt,
          message: "The update could not be installed. Check for updates to try again." });
      } finally {
        await close(item);
        busy = false;
      }
    },
    dispose() {
      disposed = true;
      generation++;
      void close(pending);
      pending = null;
    },
  };
}
