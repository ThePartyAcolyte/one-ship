/** Logs a rejected promise instead of discarding it. Supports `.catch(reportAsyncFailure)` and `(context, err)`. */
export function reportAsyncFailure(err: unknown): void;
export function reportAsyncFailure(context: string, err: unknown): void;
export function reportAsyncFailure(errOrContext: unknown, maybeErr?: unknown): void {
  if (maybeErr !== undefined) {
    console.error(`[async failure] ${String(errOrContext)}`, maybeErr);
    return;
  }
  console.error('[async failure]', errOrContext);
}
