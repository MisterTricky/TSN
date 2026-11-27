import { sha256 } from "../stdlib/crypto.ts";

function main(): i32 {
  const input: Uint8Array = new Uint8Array(72);
  let index: u32 = 0;
  while (index < 64) {
    input[index] = 54;
    index++;
  }
  input[64] = 72;
  input[65] = 105;
  input[66] = 32;
  input[67] = 84;
  input[68] = 104;
  input[69] = 101;
  input[70] = 114;
  input[71] = 101;
  const digest: Uint32Array = new Uint32Array(8);
  if (!sha256(input, digest)) return 1;
  if (digest[0] !== 0xf4ab6a85) return 2;
  if (digest[1] !== 0x44b09c01) return 3;
  if (digest[2] !== 0x76ab5896) return 4;
  if (digest[3] !== 0xa41670a3) return 5;
  if (digest[4] !== 0x472ef153) return 6;
  if (digest[5] !== 0x911d8873) return 7;
  if (digest[6] !== 0x4838f3c4) return 8;
  if (digest[7] !== 0xf73c942a) return 9;
  return 0;
}
