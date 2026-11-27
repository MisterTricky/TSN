import { verifyFixtureToken } from "../stdlib/jwt.ts";
import { utf8ToBytes } from "../stdlib/strings.ts";
import { sliceUtf8 } from "tsn:strings";

function main(): i32 {
  const secret: string = "m5-fixture-secret-not-for-production";
  const key: Uint8Array = new Uint8Array(secret.length);
  if (utf8ToBytes(secret, key) !== secret.length) return 1;

  const token: string = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyLTAwMDEiLCJleHAiOjIwMDAwMDAwMDAsImlzcyI6Im01LWZpeHR1cmUifQ.eMEuS9XlyIqNpTkUZvXzyEd0Qlw-1zKy8aCC1bsPjwk";
  if (!verifyFixtureToken(token, 1900000000, key)) return 2;
  if (verifyFixtureToken(token, 2100000000, key)) return 3;

  const changed: string = sliceUtf8(token, 0, token.length - 1) + "x";
  if (verifyFixtureToken(changed, 1900000000, key)) return 4;
  const expired: string = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyLTAwMDEiLCJleHAiOjEsImlzcyI6Im01LWZpeHR1cmUifQ.M1pWxKmmLoc6w9ebGT3wlEgFPbVnou3g503UZs26VXo";
  if (verifyFixtureToken(expired, 2, key)) return 5;
  if (verifyFixtureToken("a.b", 1900000000, key)) return 6;
  return 0;
}
