// A formula typed into a money cell, an Excel habit: "=100+250", "=1874,5*2", "=(1200-200)/3".
// It is computed exactly on fractions of whole numbers, never on floats (PLAN §15), and the
// result is rounded to kopecks half away from zero. Only the result is kept, like a value
// pasted from Excel.

import { type Parsed, fail, ok } from "./parsed";

/** n / d with d > 0. */
interface Fraction {
  n: bigint;
  d: bigint;
}

const LIMIT = 10n ** 15n; // kopecks, the same bound as a typed sum

function gcd(a: bigint, b: bigint): bigint {
  let [x, y] = [a < 0n ? -a : a, b < 0n ? -b : b];
  while (y !== 0n) [x, y] = [y, x % y];
  return x;
}

function frac(n: bigint, d: bigint): Fraction {
  const sign = d < 0n ? -1n : 1n;
  const g = gcd(n, d) || 1n;
  return { n: (sign * n) / g, d: (sign * d) / g };
}

class FormulaError extends Error {}

/** A recursive-descent reader of + − × ÷, unary minus and parentheses. */
class Reader {
  private pos = 0;

  constructor(private readonly text: string) {}

  read(): Fraction {
    const value = this.sum();
    if (this.pos < this.text.length) {
      throw new FormulaError(`лишнее «${this.text.slice(this.pos)}»`);
    }
    return value;
  }

  private peek(): string | undefined {
    return this.text[this.pos];
  }

  private sum(): Fraction {
    let value = this.product();
    for (;;) {
      const op = this.peek();
      if (op !== "+" && op !== "-") return value;
      this.pos += 1;
      const right = this.product();
      value =
        op === "+"
          ? frac(value.n * right.d + right.n * value.d, value.d * right.d)
          : frac(value.n * right.d - right.n * value.d, value.d * right.d);
    }
  }

  private product(): Fraction {
    let value = this.factor();
    for (;;) {
      const op = this.peek();
      if (op !== "*" && op !== "/") return value;
      this.pos += 1;
      const right = this.factor();
      if (op === "/") {
        if (right.n === 0n) throw new FormulaError("деление на ноль");
        value = frac(value.n * right.d, value.d * right.n);
      } else {
        value = frac(value.n * right.n, value.d * right.d);
      }
    }
  }

  private factor(): Fraction {
    const c = this.peek();
    if (c === "-" || c === "+") {
      this.pos += 1;
      const inner = this.factor();
      return c === "-" ? { n: -inner.n, d: inner.d } : inner;
    }
    if (c === "(") {
      this.pos += 1;
      const inner = this.sum();
      if (this.peek() !== ")") throw new FormulaError("не хватает «)»");
      this.pos += 1;
      return inner;
    }
    const match = /^(\d+)(?:[.,](\d+))?/.exec(this.text.slice(this.pos));
    if (match === null) {
      throw new FormulaError(c === undefined ? "формула оборвалась" : `непонятное «${c}»`);
    }
    this.pos += match[0].length;
    const whole = match[1] ?? "0";
    const part = match[2] ?? "";
    return frac(BigInt(whole + part), 10n ** BigInt(part.length));
  }
}

/** Kopecks of a formula ("=…"): its rubles × 100, rounded half away from zero. */
export function evaluateFormula(input: string): Parsed<number> {
  const body = input
    .trim()
    .slice(1)
    .replace(/\s/g, "")
    .replace(/[×хx]/g, "*")
    .replace(/[÷:]/g, "/")
    .replace(/−/g, "-");
  if (body === "") return fail("Формула пустая: после «=» напишите, что посчитать");
  let value: Fraction;
  try {
    value = new Reader(body).read();
  } catch (e) {
    if (e instanceof FormulaError)
      return fail(`Не удалось посчитать «${input.trim()}»: ${e.message}`);
    throw e;
  }
  const scaled = value.n * 100n;
  let kopecks = scaled / value.d;
  const rest = scaled % value.d;
  if ((rest < 0n ? -rest : rest) * 2n >= value.d) kopecks += value.n < 0n ? -1n : 1n;
  if (kopecks > LIMIT || kopecks < -LIMIT) return fail("Слишком большая сумма");
  return ok(Number(kopecks));
}
