import { sha256 } from "../stdlib/crypto.ts";

function main(): i32 {
  const input: Uint8Array = new Uint8Array(3);
  input[0] = 97;
  input[1] = 98;
  input[2] = 99;
  const digest: Uint32Array = new Uint32Array(8);
  if (!sha256(input, digest)) return 1;
  if (digest[0] !== 0xba7816bf) return 2;
  if (digest[1] !== 0x8f01cfea) return 3;
  if (digest[2] !== 0x414140de) return 4;
  if (digest[3] !== 0x5dae2223) return 5;
  if (digest[4] !== 0xb00361a3) return 6;
  if (digest[5] !== 0x96177a9c) return 7;
  if (digest[6] !== 0xb410ff61) return 8;
  if (digest[7] !== 0xf20015ad) return 9;
  return 0;
}
