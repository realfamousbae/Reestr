import { describe, expect, it } from "vitest";

import { passwordProblem } from "./password";

describe("new password", () => {
  it("needs eight characters, the same twice, and a note on paper", () => {
    expect(passwordProblem("short", "short", true)).toMatch(/не короче 8/);
    expect(passwordProblem("длинный пароль", "длинный парол", true)).toBe("Пароли не совпадают.");
    expect(passwordProblem("длинный пароль", "длинный пароль", false)).toMatch(/записан/);
    expect(passwordProblem("длинный пароль", "длинный пароль", true)).toBeNull();
  });
});
