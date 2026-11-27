import { hmacSha256 } from "../stdlib/crypto.ts";

function main(): i32 {
  const key: Uint8Array = new Uint8Array(20);
  let index: u32 = 0;
  while (index < key.length) {
    key[index] = 11;
    index++;
  }
  const message: Uint8Array = new Uint8Array(8);
  message[0] = 72;
  message[1] = 105;
  message[2] = 32;
  message[3] = 84;
  message[4] = 104;
  message[5] = 101;
  message[6] = 114;
  message[7] = 101;
  const digest: Uint32Array = new Uint32Array(8);
  if (!hmacSha256(key, message, digest)) return 1;
  if (digest[0] !== 0xb0344c61) return 2;
  if (digest[1] !== 0xd8db3853) return 3;
  if (digest[2] !== 0x5ca8afce) return 4;
  if (digest[3] !== 0xaf0bf12b) return 5;
  if (digest[4] !== 0x881dc200) return 6;
  if (digest[5] !== 0xc9833da7) return 7;
  if (digest[6] !== 0x26e9376c) return 8;
  if (digest[7] !== 0x2e32cff7) return 9;
  return 0;
}
