import { verifyFixtureToken } from "./jwt.ts";
import { parseM5RequestBody } from "./json.ts";
import { appendBytes, appendString, appendU32Decimal } from "./format.ts";

// Source-level service core for the deterministic M5 fixture. Input JSON is a
// deliberately narrow canonical schema; the general JSON package is not yet
// implemented. `fields` and `response` are caller-owned reusable workspaces.
export function handleFixtureRequest(
  token: string,
  body: Uint8Array,
  bodyLength: u32,
  now: u32,
  key: Uint8Array,
  fields: Uint32Array,
  response: Uint8Array,
): i32 {
  if (!verifyFixtureToken(token, now, key)) {
    const length: u32 = appendString(response, 0, "{\"ok\":false,\"message\":\"Unauthorized\"}");
    if (length === 4294967295) return 500;
    fields[5] = 0;
    fields[6] = length;
    return 401;
  }
  if (!parseM5RequestBody(body, bodyLength, fields)) {
    const length: u32 = appendString(response, 0, "{\"ok\":false,\"message\":\"Invalid request\"}");
    if (length === 4294967295) return 500;
    fields[5] = 0;
    fields[6] = length;
    return 400;
  }

  const quantity: u32 = fields[4];
  const total: u32 = quantity * 125;
  let offset: u32 = appendString(response, 0, "{\"ok\":true,\"message\":\"Accepted ");
  if (offset === 4294967295) return 500;
  offset = appendU32Decimal(response, offset, quantity);
  if (offset === 4294967295) return 500;
  offset = appendString(response, offset, " × ");
  if (offset === 4294967295) return 500;
  offset = appendBytes(response, offset, body, fields[2], fields[3]);
  if (offset === 4294967295) return 500;
  offset = appendString(response, offset, "\",\"requestId\":\"");
  if (offset === 4294967295) return 500;
  offset = appendBytes(response, offset, body, fields[0], fields[1]);
  if (offset === 4294967295) return 500;
  offset = appendString(response, offset, "\",\"total\":");
  if (offset === 4294967295) return 500;
  offset = appendU32Decimal(response, offset, total);
  if (offset === 4294967295) return 500;
  offset = appendString(response, offset, "}");
  if (offset === 4294967295) return 500;
  fields[5] = total;
  fields[6] = offset;
  return 200;
}
