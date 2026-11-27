// SHA-256 implemented in tsn source using primitive fixed-width arithmetic and
// borrowed/owned typed arrays. This is a bootstrap implementation, not yet a
// production crypto package: it has no HMAC API, key handling, or audit.

function rotateRight(value: u32, count: u32): u32 {
  return (value >>> count) | (value << (32 - count));
}

function choose(x: u32, y: u32, z: u32): u32 {
  return (x & y) ^ (~x & z);
}

function majority(x: u32, y: u32, z: u32): u32 {
  return (x & y) ^ (x & z) ^ (y & z);
}

function bigSigma0(x: u32): u32 {
  return rotateRight(x, 2) ^ rotateRight(x, 13) ^ rotateRight(x, 22);
}

function bigSigma1(x: u32): u32 {
  return rotateRight(x, 6) ^ rotateRight(x, 11) ^ rotateRight(x, 25);
}

function smallSigma0(x: u32): u32 {
  return rotateRight(x, 7) ^ rotateRight(x, 18) ^ (x >>> 3);
}

function smallSigma1(x: u32): u32 {
  return rotateRight(x, 17) ^ rotateRight(x, 19) ^ (x >>> 10);
}

export function sha256(input: Uint8Array, digest: Uint32Array): bool {
  if (digest.length !== 8 || input.length > 16777216) return false;

  let paddedLength: u32 = 64;
  while (paddedLength < input.length + 9) paddedLength = paddedLength + 64;
  const padded: Uint8Array = new Uint8Array(paddedLength);
  let index: u32 = 0;
  while (index < input.length) {
    padded[index] = input[index];
    index++;
  }
  padded[input.length] = 128;

  const bitLength: u64 = (input.length as u64) * 8;
  padded[paddedLength - 1] = bitLength as u8;
  padded[paddedLength - 2] = (bitLength >>> 8) as u8;
  padded[paddedLength - 3] = (bitLength >>> 16) as u8;
  padded[paddedLength - 4] = (bitLength >>> 24) as u8;
  padded[paddedLength - 5] = (bitLength >>> 32) as u8;
  padded[paddedLength - 6] = (bitLength >>> 40) as u8;
  padded[paddedLength - 7] = (bitLength >>> 48) as u8;
  padded[paddedLength - 8] = (bitLength >>> 56) as u8;

  const state: Uint32Array = new Uint32Array(8);
  state[0] = 0x6a09e667;
  state[1] = 0xbb67ae85;
  state[2] = 0x3c6ef372;
  state[3] = 0xa54ff53a;
  state[4] = 0x510e527f;
  state[5] = 0x9b05688c;
  state[6] = 0x1f83d9ab;
  state[7] = 0x5be0cd19;

  const k: Uint32Array = new Uint32Array(64);
  k[0] = 0x428a2f98; k[1] = 0x71374491; k[2] = 0xb5c0fbcf; k[3] = 0xe9b5dba5;
  k[4] = 0x3956c25b; k[5] = 0x59f111f1; k[6] = 0x923f82a4; k[7] = 0xab1c5ed5;
  k[8] = 0xd807aa98; k[9] = 0x12835b01; k[10] = 0x243185be; k[11] = 0x550c7dc3;
  k[12] = 0x72be5d74; k[13] = 0x80deb1fe; k[14] = 0x9bdc06a7; k[15] = 0xc19bf174;
  k[16] = 0xe49b69c1; k[17] = 0xefbe4786; k[18] = 0x0fc19dc6; k[19] = 0x240ca1cc;
  k[20] = 0x2de92c6f; k[21] = 0x4a7484aa; k[22] = 0x5cb0a9dc; k[23] = 0x76f988da;
  k[24] = 0x983e5152; k[25] = 0xa831c66d; k[26] = 0xb00327c8; k[27] = 0xbf597fc7;
  k[28] = 0xc6e00bf3; k[29] = 0xd5a79147; k[30] = 0x06ca6351; k[31] = 0x14292967;
  k[32] = 0x27b70a85; k[33] = 0x2e1b2138; k[34] = 0x4d2c6dfc; k[35] = 0x53380d13;
  k[36] = 0x650a7354; k[37] = 0x766a0abb; k[38] = 0x81c2c92e; k[39] = 0x92722c85;
  k[40] = 0xa2bfe8a1; k[41] = 0xa81a664b; k[42] = 0xc24b8b70; k[43] = 0xc76c51a3;
  k[44] = 0xd192e819; k[45] = 0xd6990624; k[46] = 0xf40e3585; k[47] = 0x106aa070;
  k[48] = 0x19a4c116; k[49] = 0x1e376c08; k[50] = 0x2748774c; k[51] = 0x34b0bcb5;
  k[52] = 0x391c0cb3; k[53] = 0x4ed8aa4a; k[54] = 0x5b9cca4f; k[55] = 0x682e6ff3;
  k[56] = 0x748f82ee; k[57] = 0x78a5636f; k[58] = 0x84c87814; k[59] = 0x8cc70208;
  k[60] = 0x90befffa; k[61] = 0xa4506ceb; k[62] = 0xbef9a3f7; k[63] = 0xc67178f2;

  let block: u32 = 0;
  while (block * 64 < paddedLength) {
    const words: Uint32Array = new Uint32Array(64);
    let wordIndex: u32 = 0;
    while (wordIndex < 16) {
      const offset: u32 = block * 64 + wordIndex * 4;
      const b0: u32 = padded[offset] as u32;
      const b1: u32 = padded[offset + 1] as u32;
      const b2: u32 = padded[offset + 2] as u32;
      const b3: u32 = padded[offset + 3] as u32;
      words[wordIndex] = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;
      wordIndex++;
    }
    while (wordIndex < 64) {
      words[wordIndex] = smallSigma1(words[wordIndex - 2]) + words[wordIndex - 7] + smallSigma0(words[wordIndex - 15]) + words[wordIndex - 16];
      wordIndex++;
    }

    let a: u32 = state[0];
    let b: u32 = state[1];
    let c: u32 = state[2];
    let d: u32 = state[3];
    let e: u32 = state[4];
    let f: u32 = state[5];
    let g: u32 = state[6];
    let h: u32 = state[7];
    let round: u32 = 0;
    while (round < 64) {
      const t1: u32 = h + bigSigma1(e) + choose(e, f, g) + k[round] + words[round];
      const t2: u32 = bigSigma0(a) + majority(a, b, c);
      h = g;
      g = f;
      f = e;
      e = d + t1;
      d = c;
      c = b;
      b = a;
      a = t1 + t2;
      round++;
    }
    state[0] = state[0] + a;
    state[1] = state[1] + b;
    state[2] = state[2] + c;
    state[3] = state[3] + d;
    state[4] = state[4] + e;
    state[5] = state[5] + f;
    state[6] = state[6] + g;
    state[7] = state[7] + h;
    block++;
  }

  index = 0;
  while (index < 8) {
    digest[index] = state[index];
    index++;
  }
  return true;
}

export function hmacSha256(key: Uint8Array, message: Uint8Array, digest: Uint32Array): bool {
  if (digest.length !== 8 || key.length > 16777216 || message.length > 16777152) return false;

  const keyBlock: Uint8Array = new Uint8Array(64);
  if (key.length > 64) {
    const keyHash: Uint32Array = new Uint32Array(8);
    if (!sha256(key, keyHash)) return false;
    let wordIndex: u32 = 0;
    while (wordIndex < 8) {
      const word: u32 = keyHash[wordIndex];
      const offset: u32 = wordIndex * 4;
      keyBlock[offset] = (word >>> 24) as u8;
      keyBlock[offset + 1] = (word >>> 16) as u8;
      keyBlock[offset + 2] = (word >>> 8) as u8;
      keyBlock[offset + 3] = word as u8;
      wordIndex++;
    }
  } else {
    let keyIndex: u32 = 0;
    while (keyIndex < key.length) {
      keyBlock[keyIndex] = key[keyIndex];
      keyIndex++;
    }
  }

  const inner: Uint8Array = new Uint8Array(64 + message.length);
  let index: u32 = 0;
  while (index < 64) {
    inner[index] = keyBlock[index] ^ 54;
    index++;
  }
  index = 0;
  while (index < message.length) {
    inner[64 + index] = message[index];
    index++;
  }
  const innerHash: Uint32Array = new Uint32Array(8);
  if (!sha256(inner, innerHash)) return false;

  const outer: Uint8Array = new Uint8Array(96);
  index = 0;
  while (index < 64) {
    outer[index] = keyBlock[index] ^ 92;
    index++;
  }
  let word: u32 = 0;
  while (word < 8) {
    const value: u32 = innerHash[word];
    const offset: u32 = 64 + word * 4;
    outer[offset] = (value >>> 24) as u8;
    outer[offset + 1] = (value >>> 16) as u8;
    outer[offset + 2] = (value >>> 8) as u8;
    outer[offset + 3] = value as u8;
    word++;
  }
  return sha256(outer, digest);
}
