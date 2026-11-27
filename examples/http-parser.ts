import { parseRequest } from "../stdlib/http.ts";
import { utf8ToBytes } from "../stdlib/strings.ts";

function main(): i32 {
  const text: string = "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 3\r\n\r\nabc";
  const bytes: Uint8Array = new Uint8Array(text.length);
  if (utf8ToBytes(text, bytes) !== text.length) return 1;
  const meta: Uint32Array = new Uint32Array(8);
  if (parseRequest(bytes, bytes.length, meta) !== 1) return 2;
  if (meta[5] !== 3 || meta[6] !== bytes.length || meta[7] !== 1) return 3;

  const brokenText: string = "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 3\r\nContent-Length: 3\r\n\r\nabc";
  const broken: Uint8Array = new Uint8Array(brokenText.length);
  utf8ToBytes(brokenText, broken);
  if (parseRequest(broken, broken.length, meta) !== -1) return 4;
  return 0;
}
