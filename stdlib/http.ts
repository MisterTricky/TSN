// A tiny source-level HTTP parser entry point used by the M3 smoke server.
// This v0 bootstrap slice deliberately accepts only an empty GET / HTTP/1.1
// request. More general request-line, header, body, and keep-alive parsing is
// still to be implemented in tsn source.
export function isEmptyGetRootRequest(input: Uint8Array, length: u32): bool {
  if (length !== 18) return false;
  if (input[0] !== 71 || input[1] !== 69 || input[2] !== 84 || input[3] !== 32) return false;
  if (input[4] !== 47 || input[5] !== 32) return false;
  if (input[6] !== 72 || input[7] !== 84 || input[8] !== 84 || input[9] !== 80) return false;
  if (input[10] !== 47 || input[11] !== 49 || input[12] !== 46 || input[13] !== 49) return false;
  if (input[14] !== 13 || input[15] !== 10 || input[16] !== 13 || input[17] !== 10) return false;
  return true;
}
