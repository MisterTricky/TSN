import { createHmac, timingSafeEqual } from "node:crypto";

// Synthetic fixture only. Secret and claims are deliberately public/test-only.
export const TEST_SECRET = "m5-fixture-secret-not-for-production";

type Claims = { sub: string; exp: number; iss: string };
type RequestBody = { requestId: string; item: { name: string; quantity: number } };
export type ApiResponse = { status: number; body: { ok: boolean; message: string; requestId?: string; total?: number } };

const encode = (value: unknown) => Buffer.from(JSON.stringify(value)).toString("base64url");
const sign = (input: string) => createHmac("sha256", TEST_SECRET).update(input).digest();

export function makeToken(claims: Claims = { sub: "user-0001", exp: 2_000_000_000, iss: "m5-fixture" }): string {
  const input = `${encode({ alg: "HS256", typ: "JWT" })}.${encode(claims)}`;
  return `${input}.${sign(input).toString("base64url")}`;
}

function decodeSegment(segment: string): unknown {
  if (!/^[A-Za-z0-9_-]+$/.test(segment)) throw new Error("bad base64url");
  const bytes = Buffer.from(segment, "base64url");
  if (bytes.toString("base64url") !== segment) throw new Error("non-canonical base64url");
  return JSON.parse(bytes.toString("utf8"));
}

function validClaims(value: unknown, now: number): value is Claims {
  if (value === null || typeof value !== "object") return false;
  const c = value as Record<string, unknown>;
  return typeof c.sub === "string" && c.sub.length > 0 && c.sub.length <= 64 &&
    Number.isSafeInteger(c.exp) && (c.exp as number) > now && c.iss === "m5-fixture";
}

function validBody(value: unknown): value is RequestBody {
  if (value === null || typeof value !== "object") return false;
  const b = value as Record<string, unknown>;
  if (typeof b.requestId !== "string" || !/^[a-z0-9-]{1,24}$/.test(b.requestId)) return false;
  if (b.item === null || typeof b.item !== "object") return false;
  const item = b.item as Record<string, unknown>;
  return typeof item.name === "string" && item.name.length >= 1 && item.name.length <= 40 &&
    Number.isSafeInteger(item.quantity) && (item.quantity as number) >= 1 && (item.quantity as number) <= 99;
}

export function handleRequest(token: string, body: unknown, now = 1_900_000_000): ApiResponse {
  try {
    const parts = token.split(".");
    if (parts.length !== 3) throw new Error("malformed token");
    const header = decodeSegment(parts[0]);
    if (header === null || typeof header !== "object" || (header as Record<string, unknown>).alg !== "HS256" ||
        ((header as Record<string, unknown>).typ !== undefined && (header as Record<string, unknown>).typ !== "JWT")) {
      throw new Error("unsupported header");
    }
    if (!/^[A-Za-z0-9_-]+$/.test(parts[2])) throw new Error("bad signature");
    const signature = Buffer.from(parts[2], "base64url");
    if (signature.toString("base64url") !== parts[2]) throw new Error("bad signature");
    const expected = sign(`${parts[0]}.${parts[1]}`);
    if (signature.length !== expected.length || !timingSafeEqual(signature, expected)) throw new Error("invalid signature");
    const claims = decodeSegment(parts[1]);
    if (!validClaims(claims, now)) throw new Error("invalid claims");
  } catch {
    return { status: 401, body: { ok: false, message: "Unauthorized" } };
  }

  if (!validBody(body)) return { status: 400, body: { ok: false, message: "Invalid request" } };
  const total = body.item.quantity * 125;
  return { status: 200, body: { ok: true, message: `Accepted ${body.item.quantity} × ${body.item.name}`, requestId: body.requestId, total } };
}
