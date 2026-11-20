/** Logs a rejected promise instead of discarding it. Use as `.catch(reportAsyncFailure)`. */
export function reportAsyncFailure(err: unknown): void {
  console.error('[async failure]', err);
}
