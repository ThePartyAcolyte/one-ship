// Typed client of the daemon RPC API (spec 12 §7). The daemon owns every process and all state;
// the app only calls it. There is no fallback: an unreachable daemon is an error the caller shows.
import { invoke } from '@tauri-apps/api/core';
import type { DaemonCommand, DaemonCommands } from '../types/daemonApi';
import type { ApiErrorResponse } from '../types/generated/ApiErrorResponse';

/** A command the daemon answered with an `ApiErrorResponse`. `name` is the error code. */
export class DaemonCallError extends Error {
  constructor(
    readonly command: DaemonCommand,
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = code;
  }
}

/** The daemon could not be reached, or did not answer with JSON. */
export class DaemonUnreachableError extends Error {
  constructor(readonly command: DaemonCommand, detail: string) {
    super(`ONE-SHIP daemon unreachable while calling '${command}': ${detail}`);
    this.name = 'DaemonUnreachableError';
  }
}

let baseUrlPromise: Promise<string> | null = null;

/** Base URL of the daemon (`mcp.mcp_daemon_port`, read once from the app). */
export function daemonBaseUrl(): Promise<string> {
  if (!baseUrlPromise) {
    baseUrlPromise = invoke<number>('get_daemon_port_cmd')
      .then((port) => `http://127.0.0.1:${port}`)
      .catch((e: unknown) => {
        baseUrlPromise = null;
        throw e;
      });
  }
  return baseUrlPromise;
}

function isApiErrorResponse(body: unknown): body is ApiErrorResponse {
  if (typeof body !== 'object' || body === null || !('error' in body)) return false;
  const error = (body as { error: unknown }).error;
  return typeof error === 'object' && error !== null && 'code' in error && 'message' in error;
}

/** Calls a daemon command; rejects with `DaemonCallError` or `DaemonUnreachableError`. */
export async function daemonCall<K extends DaemonCommand>(
  command: K,
  args: DaemonCommands[K]['args'],
): Promise<DaemonCommands[K]['result']> {
  const url = `${await daemonBaseUrl()}/api/v1/call/${command}`;
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(args),
    });
  } catch (e: unknown) {
    throw new DaemonUnreachableError(command, String(e));
  }

  let body: unknown;
  try {
    body = await response.json();
  } catch (e: unknown) {
    throw new DaemonUnreachableError(command, `HTTP ${response.status} without a JSON body (${String(e)})`);
  }

  if (!response.ok) {
    if (!isApiErrorResponse(body)) {
      throw new DaemonUnreachableError(command, `HTTP ${response.status} with an unexpected body`);
    }
    throw new DaemonCallError(command, response.status, body.error.code, body.error.message);
  }
  return body as DaemonCommands[K]['result'];
}
