/** The result of parsing user input: a value, or a message to show. */
export type Parsed<T> = { ok: true; value: T } | { ok: false; message: string };

export function ok<T>(value: T): Parsed<T> {
  return { ok: true, value };
}

export function fail<T = never>(message: string): Parsed<T> {
  return { ok: false, message };
}
