const body = '{"ok":true}';

const server = Bun.serve({
  hostname: "127.0.0.1",
  port: 24875,
  fetch(request) {
    const url = new URL(request.url);
    if (request.method !== "GET" || url.pathname !== "/") {
      return new Response('{"ok":false}', {
        status: 404,
        headers: { "Content-Type": "application/json", Connection: "close" },
      });
    }
    return new Response(body, {
      status: 200,
      headers: { "Content-Type": "application/json", Connection: "keep-alive" },
    });
  },
});

process.on("SIGTERM", () => server.stop(false));
