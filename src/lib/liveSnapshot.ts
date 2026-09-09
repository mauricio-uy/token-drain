/**
 * Subscribe before reading an initial snapshot without letting that snapshot
 * overwrite a newer event received while the read was in flight.
 */
export function liveSnapshot<T>(
  subscribe: (receive: (value: T) => void) => Promise<() => void>,
  read: () => Promise<T>,
  receive: (value: T) => void,
  onError: (cause: unknown) => void = () => {},
): () => void {
  let cancelled = false;
  let revision = 0;
  let stop: (() => void) | undefined;

  void (async () => {
    try {
      stop = await subscribe((value) => {
        revision += 1;
        if (!cancelled) receive(value);
      });
      if (cancelled) {
        stop();
        return;
      }
      const initialRevision = revision;
      const snapshot = await read();
      if (!cancelled && revision === initialRevision) receive(snapshot);
    } catch (cause) {
      if (!cancelled) onError(cause);
    }
  })();

  return () => {
    cancelled = true;
    stop?.();
  };
}
