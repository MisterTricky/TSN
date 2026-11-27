function main(): i32 {
  let count: u32 = 0;
  while (count < 100) {
    const bytes: Uint8Array = new Uint8Array(16);
    bytes[0] = 1;
    count++;
    if (count === 20) break;
  }
  return 0;
}
