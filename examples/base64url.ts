import { decodeBase64Url } from "../stdlib/base64.ts";

function main(): i32 {
  const output: Uint8Array = new Uint8Array(2);
  const count: u32 = decodeBase64Url("SGk", output);
  if (count !== 2 || output[0] !== 72 || output[1] !== 105) return 1;

  const badOutput: Uint8Array = new Uint8Array(2);
  if (decodeBase64Url("SG!", badOutput) !== 4294967295) return 2;
  return 0;
}
