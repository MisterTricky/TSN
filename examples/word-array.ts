function main(): i32 {
  const words: Uint32Array = new Uint32Array(2);
  words[0] = 0x12345678;
  words[1] = words[0] + 1;
  if (words[1] === 0x12345679) return 0;
  return 1;
}
