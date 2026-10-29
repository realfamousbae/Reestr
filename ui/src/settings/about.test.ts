import { describe, expect, it } from "vitest";

import { cipherText, createdText, formatSize } from "./about";

describe("about the file", () => {
  it("shows sizes and dates in Russian", () => {
    expect(formatSize(900)).toBe("900 Б");
    expect(formatSize(84_000)).toBe("82 КБ");
    expect(formatSize(1_300_000)).toBe("1,2 МБ");
    expect(createdText("2026-10-27T20:03:02Z")).toBe("27.10.2026");
    expect(createdText("")).toBe("—");
  });

  it("names the encryption", () => {
    const text = cipherText({
      version: "4.19.0 community",
      kdfIterations: "256000",
      kdfAlgorithm: "PBKDF2_HMAC_SHA512",
      hmacAlgorithm: "HMAC_SHA512",
      pageSize: "4096",
      provider: "openssl",
    });
    expect(text).toContain("AES-256");
    expect(text).toContain("PBKDF2-HMAC-SHA512, 256 000 повторений");
    expect(text).toContain("HMAC-SHA512");
  });
});
