import { printU32, readStdin } from "tsn:io";

function main(): i32 {
  let hash: u32 = 2166136261;
  let bytes: u32 = 0;
  let records: u32 = 0;
  const prefix: Uint8Array = new Uint8Array(4);
  const payload: Uint8Array = new Uint8Array(996);

  while (records < 100000) {
    if (readStdin(prefix) !== 4) return 2;

    const b0: u32 = prefix[0];
    const b1: u32 = prefix[1];
    const b2: u32 = prefix[2];
    const b3: u32 = prefix[3];
    const length: u32 = b0 | (b1 << 8) | (b2 << 16) | (b3 << 24);
    if (length !== 996) return 3;

    if (readStdin(payload) !== 996) return 2;

    let i: u32 = 0;
    let valid: bool = true;
    while (i < 996) {
      const value: u8 = payload[i];
      const expected: u32 = (records * 17 + i * 31 + 90) & 255;
      if (value !== expected) valid = false;
      hash = Math.imul(hash ^ value, 16777619) >>> 0;
      i++;
    }
    if (!valid) return 4;

    bytes = bytes + 1000;
    records++;
  }

  const extra: Uint8Array = new Uint8Array(1);
  if (readStdin(extra) !== 0) return 5;

  printU32(records);
  printU32(bytes);
  printU32(hash);
  return 0;
}
