/**
 * Runs one job per key at a time. Asked again while one runs, it runs once more after it; all
 * who ask meanwhile wait for that run. `run` must not throw.
 */
export function oneAtATime(): (key: string, run: () => Promise<void>) => Promise<void> {
  const running = new Map<string, Promise<void>>();
  const again = new Map<string, Promise<void>>();
  const start = (key: string, run: () => Promise<void>): Promise<void> => {
    const current = running.get(key);
    if (current) {
      let next = again.get(key);
      if (!next) {
        next = current.then(() => {
          again.delete(key);
          return start(key, run);
        });
        again.set(key, next);
      }
      return next;
    }
    const job = run().finally(() => running.delete(key));
    running.set(key, job);
    return job;
  };
  return start;
}
