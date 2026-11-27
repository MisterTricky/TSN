import { acceptTcp, closeTcp, listenTcp, readTcp } from "tsn:net";
import { parseRequest } from "../stdlib/http.ts";
import { writeStringAll } from "../stdlib/net.ts";
import { matchesAscii } from "../stdlib/json.ts";
import { shutdownRequested, spawnThread } from "tsn:os";

function serveConnection(connection: i32): void {
  const keepAliveResponse: string = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: keep-alive\r\n\r\n{\"ok\":true}";
  const closeResponse: string = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";
  const request: Uint8Array = new Uint8Array(4096);
  const metadata: Uint32Array = new Uint32Array(8);
  let handled: u32 = 0;
  while (handled < 100) {
    let received: u32 = 0;
    let readFailed: bool = false;
    let parseStatus: i32 = 0;
    while (parseStatus === 0 && !readFailed) {
      const count: u32 = readTcp(connection, request, received);
      if (count === 0 || count === 4294967295) {
        readFailed = true;
      } else {
        received = received + count;
        parseStatus = parseRequest(request, received, metadata);
      }
    }
    if (readFailed) break;
    if (parseStatus !== 1 || !matchesAscii(request, received, metadata[0], "GET") || !matchesAscii(request, received, metadata[2], "/")) break;

    let written: bool = false;
    const closeAfterResponse: bool = metadata[7] === 0 || handled === 99 || shutdownRequested();
    if (closeAfterResponse) {
      written = writeStringAll(connection, closeResponse);
    } else {
      written = writeStringAll(connection, keepAliveResponse);
    }
    if (!written || closeAfterResponse) break;
    handled++;
    received = 0;
  }
  closeTcp(connection);
}

function main(): i32 {
  const listener: i32 = listenTcp(24875, true);
  if (listener < 0) return 1;

  while (!shutdownRequested()) {
    const connection: i32 = acceptTcp(listener);
    if (connection < 0) {
      if (shutdownRequested()) break;
      closeTcp(listener);
      return 2;
    }
    const status: i32 = spawnThread(serveConnection, connection);
    if (status !== 0) closeTcp(connection);
  }

  closeTcp(listener);
  return 0;
}
