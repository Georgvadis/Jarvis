const CACHE = "jarvis-shell-v28";

self.addEventListener("install", (event) => {
  event.waitUntil(self.skipWaiting());
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

function offlinePage() {
  const html = `<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Jarvis — ожидание колонки</title>
</head>
<body>
<main>
  <h1>Колонка недоступна или перезапускается</h1>
  <p>Проверьте питание и подключение к домашнему Wi-Fi, затем обновите страницу.</p>
  <p><a href="/">Обновить</a></p>
</main>
</body>
</html>`;
  return new Response(html, {
    status: 503,
    headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" },
  });
}

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (url.pathname.startsWith("/api/") || url.pathname === "/healthz" || url.pathname === "/sw.js") {
    return;
  }

  if (
    url.pathname.startsWith("/_app/immutable/")
    || url.pathname.startsWith("/icon-")
    || url.pathname === "/manifest.webmanifest"
  ) {
    event.respondWith(
      caches.open(CACHE).then(async (cache) => {
        const hit = await cache.match(request);
        if (hit) return hit;
        const resp = await fetch(request);
        if (resp && resp.ok) void cache.put(request, resp.clone());
        return resp;
      }),
    );
    return;
  }

  if (request.mode === "navigate") {
    event.respondWith(fetch(request).catch(() => offlinePage()));
  }
});
