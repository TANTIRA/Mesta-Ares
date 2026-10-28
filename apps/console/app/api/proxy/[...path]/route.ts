/**
 * BFF proxy: forwards every `/api/proxy/*` request to the api-server (or
 * gateway) with `Authorization: Bearer $ARES_CONSOLE_KEY` injected
 * server-side. The API key never reaches the browser; EventSource streams
 * (which cannot set headers) authenticate the same way.
 *
 * Runtime envs: `ARES_API_URL` (upstream, default http://localhost:8080),
 * `ARES_CONSOLE_KEY` (optional; omit when the upstream runs dev-mode auth).
 */

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

const UPSTREAM = () => process.env.ARES_API_URL ?? "http://localhost:8080";

const HOP_BY_HOP = new Set([
  "connection",
  "keep-alive",
  "proxy-authenticate",
  "proxy-authorization",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
  "host",
  "content-length",
]);

async function forward(
  req: Request,
  ctx: { params: Promise<{ path: string[] }> },
): Promise<Response> {
  const { path } = await ctx.params;
  const { search } = new URL(req.url);
  const url = `${UPSTREAM()}/${path.join("/")}${search}`;

  const headers = new Headers();
  req.headers.forEach((value, name) => {
    const n = name.toLowerCase();
    // Never forward browser-supplied credentials or session cookies; the
    // upstream sees only the injected service key.
    if (HOP_BY_HOP.has(n) || n === "authorization" || n === "x-api-key" || n === "cookie") {
      return;
    }
    headers.set(name, value);
  });
  const key = process.env.ARES_CONSOLE_KEY;
  if (key) {
    headers.set("authorization", `Bearer ${key}`);
  }

  let upstream: Response;
  try {
    upstream = await fetch(url, {
      method: req.method,
      headers,
      body:
        req.method === "GET" || req.method === "HEAD" ? undefined : req.body,
      // @ts-expect-error required by undici for streaming request bodies
      duplex: "half",
      redirect: "manual",
      cache: "no-store",
    });
  } catch (e) {
    return Response.json(
      { error: { code: "UPSTREAM_ERROR", message: `proxy upstream failed: ${e}` } },
      { status: 502 },
    );
  }

  const out = new Headers();
  upstream.headers.forEach((value, name) => {
    if (!HOP_BY_HOP.has(name.toLowerCase())) out.set(name, value);
  });
  // Streaming passthrough: SSE and chunked bodies flow unbuffered.
  return new Response(upstream.body, {
    status: upstream.status,
    statusText: upstream.statusText,
    headers: out,
  });
}

export {
  forward as GET,
  forward as POST,
  forward as PUT,
  forward as PATCH,
  forward as DELETE,
  forward as OPTIONS,
};
