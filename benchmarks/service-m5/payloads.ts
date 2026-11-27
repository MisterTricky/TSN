import { makeToken } from "./service";

export const token = makeToken();
export const validRequest = { requestId: "req-0001", item: { name: "widget", quantity: 3 } };

// Stable workload: each iteration selects the same valid/invalid cases in order.
export const workload = [
  { token, body: validRequest },
  { token, body: { requestId: "req-0002", item: { name: "gear", quantity: 2 } } },
  { token: `${token.slice(0, -1)}x`, body: validRequest },
  { token, body: { requestId: "BAD ID", item: { name: "widget", quantity: 3 } } },
  { token: "not.a.jwt.extra", body: validRequest },
] as const;
