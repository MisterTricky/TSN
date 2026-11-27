import { findByte, matchesAscii, parseDecimalU32 } from "./json.ts";
import { stringByteAt } from "tsn:strings";

// Small ASCII token helpers are ordinary tsn code. The numeric sentinels mean
// invalid-line (u32::MAX) and incomplete-line (u32::MAX - 1).
function lineEnd(input: Uint8Array, filled: u32, start: u32): u32 {
  let index: u32 = start;
  while (index < filled) {
    const byte: u8 = input[index];
    if (byte === 10) return 4294967295;
    if (byte === 13) {
      if (index + 1 >= filled) return 4294967294;
      if (input[index + 1] !== 10) return 4294967295;
      return index;
    }
    index++;
  }
  return 4294967294;
}

function isTokenByte(value: u8): bool {
  if (value >= 48 && value <= 57) return true;
  if (value >= 65 && value <= 90) return true;
  if (value >= 97 && value <= 122) return true;
  if (value === 33 || value === 35 || value === 36 || value === 37 || value === 38 || value === 39) return true;
  if (value === 42 || value === 43 || value === 45 || value === 46) return true;
  if (value === 94 || value === 95 || value === 96 || value === 124 || value === 126) return true;
  return false;
}

function isOptionalWhitespace(value: u8): bool {
  return value === 32 || value === 9;
}

function equalsIgnoreAsciiCase(input: Uint8Array, start: u32, length: u32, expected: string): bool {
  if (length !== expected.length) return false;
  let index: u32 = 0;
  while (index < length) {
    let value: u32 = input[start + index] as u32;
    if (value >= 65 && value <= 90) value = value + 32;
    if (value !== stringByteAt(expected, index)) return false;
    index++;
  }
  return true;
}

// Parse one bounded HTTP/1.0 or HTTP/1.1 request. `meta` receives method span,
// target span, body span, total bytes consumed, and keep-alive flag. Returns 1
// complete, 0 incomplete, or -1 invalid. Request Transfer-Encoding is rejected;
// response framing and higher-level routing remain source-library work.
export function parseRequest(input: Uint8Array, filled: u32, meta: Uint32Array): i32 {
  if (filled > input.length || meta.length < 8) return -1;
  const requestEnd: u32 = lineEnd(input, filled, 0);
  if (requestEnd === 4294967294) return 0;
  if (requestEnd === 4294967295 || requestEnd > 8192) return -1;

  const firstSpace: u32 = findByte(input, requestEnd, 0, 32);
  if (firstSpace === 4294967295 || firstSpace === 0) return -1;
  const secondSpace: u32 = findByte(input, requestEnd, firstSpace + 1, 32);
  if (secondSpace === 4294967295 || secondSpace === firstSpace + 1) return -1;
  const versionStart: u32 = secondSpace + 1;
  if (versionStart + 8 !== requestEnd) return -1;
  const http11: bool = matchesAscii(input, filled, versionStart, "HTTP/1.1");
  if (!http11 && !matchesAscii(input, filled, versionStart, "HTTP/1.0")) return -1;
  meta[0] = 0;
  meta[1] = firstSpace;
  meta[2] = firstSpace + 1;
  meta[3] = secondSpace - firstSpace - 1;

  let cursor: u32 = requestEnd + 2;
  const headerStart: u32 = cursor;
  let headerCount: u32 = 0;
  let hasContentLength: bool = false;
  let contentLength: u32 = 0;
  let connectionClose: bool = false;
  let connectionKeepAlive: bool = false;
  let hostSeen: bool = false;
  let headersDone: bool = false;
  while (!headersDone) {
    const headerEnd: u32 = lineEnd(input, filled, cursor);
    if (headerEnd === 4294967294) return 0;
    if (headerEnd === 4294967295) return -1;
    if (headerEnd - headerStart > 16384 || headerCount >= 100) return -1;
    if (headerEnd === cursor) {
      cursor = headerEnd + 2;
      headersDone = true;
    } else {
      const colon: u32 = findByte(input, headerEnd, cursor, 58);
      if (colon === 4294967295 || colon === cursor || colon >= headerEnd) return -1;
      let nameIndex: u32 = cursor;
      while (nameIndex < colon) {
        if (!isTokenByte(input[nameIndex])) return -1;
        nameIndex++;
      }
      let valueStart: u32 = colon + 1;
      while (valueStart < headerEnd && isOptionalWhitespace(input[valueStart])) valueStart++;
      let valueEnd: u32 = headerEnd;
      while (valueEnd > valueStart && isOptionalWhitespace(input[valueEnd - 1])) valueEnd = valueEnd - 1;
      let valueIndex: u32 = valueStart;
      while (valueIndex < valueEnd) {
        const valueByte: u8 = input[valueIndex];
        if (valueByte < 32 && valueByte !== 9) return -1;
        if (valueByte === 127) return -1;
        valueIndex++;
      }

      const nameLength: u32 = colon - cursor;
      const valueLength: u32 = valueEnd - valueStart;
      if (equalsIgnoreAsciiCase(input, cursor, nameLength, "content-length")) {
        if (hasContentLength) return -1;
        contentLength = parseDecimalU32(input, valueStart, valueEnd);
        if (contentLength === 4294967295 || contentLength > 1048576) return -1;
        hasContentLength = true;
      } else if (equalsIgnoreAsciiCase(input, cursor, nameLength, "transfer-encoding")) {
        return -1;
      } else if (equalsIgnoreAsciiCase(input, cursor, nameLength, "host")) {
        if (hostSeen || valueLength === 0) return -1;
        hostSeen = true;
      } else if (equalsIgnoreAsciiCase(input, cursor, nameLength, "connection")) {
        connectionClose = equalsIgnoreAsciiCase(input, valueStart, valueLength, "close");
        connectionKeepAlive = equalsIgnoreAsciiCase(input, valueStart, valueLength, "keep-alive");
      }
      headerCount++;
      cursor = headerEnd + 2;
    }
  }

  if (http11 && !hostSeen) return -1;
  if (contentLength > 4294967295 - cursor) return -1;
  const requestBytes: u32 = cursor + contentLength;
  if (filled < requestBytes) return 0;
  meta[4] = cursor;
  meta[5] = contentLength;
  meta[6] = requestBytes;
  let keepAlive: bool = http11;
  if (connectionClose) keepAlive = false;
  else if (connectionKeepAlive) keepAlive = true;
  meta[7] = 0;
  if (keepAlive) meta[7] = 1;
  return 1;
}

// Kept as a tiny deterministic route fixture for the local smoke server.
export function isEmptyGetRootRequest(input: Uint8Array, length: u32): bool {
  if (length !== 18) return false;
  if (input[0] !== 71 || input[1] !== 69 || input[2] !== 84 || input[3] !== 32) return false;
  if (input[4] !== 47 || input[5] !== 32) return false;
  if (input[6] !== 72 || input[7] !== 84 || input[8] !== 84 || input[9] !== 80) return false;
  if (input[10] !== 47 || input[11] !== 49 || input[12] !== 46 || input[13] !== 49) return false;
  if (input[14] !== 13 || input[15] !== 10 || input[16] !== 13 || input[17] !== 10) return false;
  return true;
}
