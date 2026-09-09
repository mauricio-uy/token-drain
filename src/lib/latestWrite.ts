/**
 * Serialize writes while coalescing queued values to the newest one.
 *
 * An older response is ignored whenever a newer edit is waiting, so an
 * out-of-order network reply cannot restore stale UI state.
 */
export function latestWrite<T>(
  write: (value: T) => Promise<T>,
  onStored: (value: T) => void,
  onError: (cause: unknown) => void,
): (value: T) => void {
  let pending: { value: T } | null = null;
  let running = false;

  async function drain() {
    running = true;
    while (pending) {
      const { value } = pending;
      pending = null;
      try {
        const stored = await write(value);
        // An older reply must not undo edits made while it was in flight.
        if (!pending) onStored(stored);
      } catch (cause) {
        if (!pending) onError(cause);
      }
    }
    running = false;
  }

  return (value) => {
    pending = { value };
    if (!running) void drain();
  };
}
