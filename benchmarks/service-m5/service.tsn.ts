import { handleFixtureRequest } from "../../stdlib/m5-service.ts";
import { utf8ToBytes } from "../../stdlib/strings.ts";
import { equalsAscii } from "../../stdlib/format.ts";
import { sliceUtf8 } from "tsn:strings";
import { printF64, printU32 } from "tsn:io";
import { monotonicMs } from "tsn:time";

function checksumOne(token: string, body: Uint8Array, bodyLength: u32, now: u32, key: Uint8Array, fields: Uint32Array, response: Uint8Array): u32 {
  const status: i32 = handleFixtureRequest(token, body, bodyLength, now, key, fields, response);
  return (status as u32) + fields[5];
}

function main(): i32 {
  const secret: string = "m5-fixture-secret-not-for-production";
  const key: Uint8Array = new Uint8Array(secret.length);
  if (utf8ToBytes(secret, key) !== secret.length) return 1;

  const validBodyText: string = "{\"requestId\":\"req-0001\",\"item\":{\"name\":\"widget\",\"quantity\":3}}";
  const secondBodyText: string = "{\"requestId\":\"req-0002\",\"item\":{\"name\":\"gear\",\"quantity\":2}}";
  const invalidBodyText: string = "{\"requestId\":\"BAD ID\",\"item\":{\"name\":\"widget\",\"quantity\":3}}";
  const validBody: Uint8Array = new Uint8Array(validBodyText.length);
  const secondBody: Uint8Array = new Uint8Array(secondBodyText.length);
  const invalidBody: Uint8Array = new Uint8Array(invalidBodyText.length);
  utf8ToBytes(validBodyText, validBody);
  utf8ToBytes(secondBodyText, secondBody);
  utf8ToBytes(invalidBodyText, invalidBody);

  const token: string = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyLTAwMDEiLCJleHAiOjIwMDAwMDAwMDAsImlzcyI6Im01LWZpeHR1cmUifQ.eMEuS9XlyIqNpTkUZvXzyEd0Qlw-1zKy8aCC1bsPjwk";
  const expired: string = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyLTAwMDEiLCJleHAiOjEsImlzcyI6Im01LWZpeHR1cmUifQ.M1pWxKmmLoc6w9ebGT3wlEgFPbVnou3g503UZs26VXo";
  const badSignature: string = sliceUtf8(token, 0, token.length - 1) + "x";
  const malformed: string = "not.a.jwt.extra";
  const now: u32 = 1900000000;
  const fields: Uint32Array = new Uint32Array(7);
  const response: Uint8Array = new Uint8Array(512);

  if (handleFixtureRequest(token, validBody, validBody.length, now, key, fields, response) !== 200) return 2;
  if (fields[5] !== 375) return 3;
  if (!equalsAscii(response, fields[6], "{\"ok\":true,\"message\":\"Accepted 3 × widget\",\"requestId\":\"req-0001\",\"total\":375}")) return 4;
  if (handleFixtureRequest(badSignature, validBody, validBody.length, now, key, fields, response) !== 401) return 5;
  if (handleFixtureRequest(malformed, validBody, validBody.length, now, key, fields, response) !== 401) return 6;
  if (handleFixtureRequest(expired, validBody, validBody.length, now, key, fields, response) !== 401) return 7;
  if (handleFixtureRequest(token, invalidBody, invalidBody.length, now, key, fields, response) !== 400) return 8;

  const started: f64 = monotonicMs();
  let checksum: u32 = 0;
  let iteration: u32 = 0;
  let caseIndex: u32 = 0;
  while (iteration < 100000) {
    if (caseIndex === 0) checksum = checksum + checksumOne(token, validBody, validBody.length, now, key, fields, response);
    else if (caseIndex === 1) checksum = checksum + checksumOne(token, secondBody, secondBody.length, now, key, fields, response);
    else if (caseIndex === 2) checksum = checksum + checksumOne(badSignature, validBody, validBody.length, now, key, fields, response);
    else if (caseIndex === 3) checksum = checksum + checksumOne(token, invalidBody, invalidBody.length, now, key, fields, response);
    else checksum = checksum + checksumOne(malformed, validBody, validBody.length, now, key, fields, response);
    iteration++;
    caseIndex++;
    if (caseIndex === 5) caseIndex = 0;
  }
  if (checksum !== 44540000) return 9;
  const elapsed_ms: f64 = monotonicMs() - started;
  printU32(checksum);
  printF64(elapsed_ms);
  return 0;
}
