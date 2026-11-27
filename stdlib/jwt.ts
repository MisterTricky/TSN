import { decodeBase64Url } from "./base64.ts";
import { hmacSha256 } from "./crypto.ts";
import { findByte, matchesAscii, parseDecimalU32 } from "./json.ts";
import { indexOf, utf8ToBytes } from "./strings.ts";
import { sliceUtf8 } from "tsn:strings";

function validFixtureClaims(payload: Uint8Array, length: u32, now: u32): bool {
  if (!matchesAscii(payload, length, 0, "{\"sub\":\"")) return false;
  const subjectStart: u32 = 8;
  const subjectEnd: u32 = findByte(payload, length, subjectStart, 34);
  if (subjectEnd === 4294967295 || subjectEnd - subjectStart !== 9) return false;
  if (!matchesAscii(payload, length, subjectStart, "user-0001")) return false;
  if (!matchesAscii(payload, length, subjectEnd + 1, ",\"exp\":")) return false;

  const expirationStart: u32 = subjectEnd + 8;
  const expirationEnd: u32 = findByte(payload, length, expirationStart, 44);
  if (expirationEnd === 4294967295) return false;
  const expiration: u32 = parseDecimalU32(payload, expirationStart, expirationEnd);
  if (expiration === 4294967295 || expiration <= now) return false;

  const issuerSuffix: string = ",\"iss\":\"m5-fixture\"}";
  if (!matchesAscii(payload, length, expirationEnd, issuerSuffix)) return false;
  return expirationEnd + issuerSuffix.length === length;
}

// Verifies the bootstrap service's canonical HS256 token shape. Header/claim
// validation is source code; only HMAC-SHA256 is composed from the crypto module.
// This narrow verifier intentionally rejects JSON whitespace, escaped subjects,
// alternate claim orders, and headers other than the checked canonical JWT header.
export function verifyFixtureToken(token: string, now: u32, key: Uint8Array): bool {
  const firstDot: u32 = indexOf(token, ".");
  if (firstDot === 4294967295 || firstDot === 0) return false;
  const afterFirst: string = sliceUtf8(token, firstDot + 1, token.length);
  const secondRelative: u32 = indexOf(afterFirst, ".");
  if (secondRelative === 4294967295 || secondRelative === 0) return false;
  const secondDot: u32 = firstDot + 1 + secondRelative;
  const afterSecond: string = sliceUtf8(token, secondDot + 1, token.length);
  if (afterSecond.length === 0 || indexOf(afterSecond, ".") !== 4294967295) return false;

  const headerSegment: string = sliceUtf8(token, 0, firstDot);
  if (headerSegment !== "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9") return false;
  const payloadSegment: string = sliceUtf8(token, firstDot + 1, secondDot);
  const signatureSegment: string = afterSecond;
  if (signatureSegment.length !== 43) return false;

  const header: Uint8Array = new Uint8Array(headerSegment.length);
  const payload: Uint8Array = new Uint8Array(payloadSegment.length);
  const signature: Uint8Array = new Uint8Array(32);
  const headerLength: u32 = decodeBase64Url(headerSegment, header);
  const payloadLength: u32 = decodeBase64Url(payloadSegment, payload);
  const signatureLength: u32 = decodeBase64Url(signatureSegment, signature);
  if (headerLength !== 27 || payloadLength === 4294967295 || signatureLength !== 32) return false;
  if (!matchesAscii(header, headerLength, 0, "{\"alg\":\"HS256\",\"typ\":\"JWT\"}")) return false;
  if (!validFixtureClaims(payload, payloadLength, now)) return false;

  const signedText: string = sliceUtf8(token, 0, secondDot);
  const message: Uint8Array = new Uint8Array(signedText.length);
  if (utf8ToBytes(signedText, message) !== signedText.length) return false;
  const digest: Uint32Array = new Uint32Array(8);
  if (!hmacSha256(key, message, digest)) return false;

  let difference: u32 = 0;
  let index: u32 = 0;
  while (index < 32) {
    const word: u32 = digest[index >>> 2];
    const shift: u32 = 24 - ((index & 3) * 8);
    const expected: u8 = (word >>> shift) as u8;
    difference = difference | (expected ^ signature[index]);
    index++;
  }
  return difference === 0;
}
