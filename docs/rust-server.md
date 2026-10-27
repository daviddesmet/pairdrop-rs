# The Rust server

This fork replaces PairDrop's Node signaling server with a Rust one. The web client in `public/`
is unchanged PairDrop, and the protocol is identical, so stock clients work against it.

Upstream's `server/` was removed in 2.0.0 once the port was verified frame-for-frame against
it. It is still in history: `git show fb743eb:server/ws-server.js`, or
`git checkout fb743eb -- server` to bring it back temporarily.

## Why

Upstream's last substantive commit is 2025-02-25. Community pull requests sit unmerged, and the
one bug that actually bites a self-hosted instance, [#384][384], is a client-side cache
problem no proxy configuration can fix. Owning the server makes it fixable.

[384]: https://github.com/schlagmichdoch/PairDrop/issues/384

## The service worker cache

`public/service-worker.js` is cache-first with no revalidation, and its `activate` handler only
deletes caches whose name differs from the current one. Upstream hardcodes that name as the
release version:

```js
const cacheVersion = 'v1.11.2';
```

So a cache populated from stale HTTP responses keeps serving them forever: the name still
matches, nothing is deleted, nothing is re-fetched. A stale `index.html` beside a current
`main.js` makes every deferred script load twice, raising `Uncaught SyntaxError: Identifier
'Events' has already been declared`. `network.js` aborts, the WebSocket never opens, and the
display name never arrives. A hard reload bypasses the service worker and looks fine; the next normal load is
broken again. Only clearing site data, by hand, on every device, gets out of it.

`src/sw.rs` serves the worker with the version replaced by a digest of everything else in
`public/`:

```
const cacheVersion = 'sha-5234c7fae57e99f6';
```

Change any asset and the name changes, so `activate` drops the old cache and refills it. The skew
cannot outlive a deploy, and the first visit after cutover clears whatever a device was holding.

Static responses also carry `Cache-Control: no-cache`, because `cache.addAll()` fetches through
the ordinary HTTP cache. Without it the service worker can bake a heuristically cached copy into
its own cache, which is how the cache gets poisoned in the first place. ETags still answer `304`,
so revalidation stays cheap.

## Healthcheck

The runtime image is `scratch`, so there is no `wget`, `curl` or shell for `HEALTHCHECK` to call.
The binary answers for itself:

```
pairdrop-rs --healthcheck
```

It opens one TCP connection to `127.0.0.1:$PORT`, sends `GET /config`, and exits `0` only on
`HTTP/1.1 200`.

**It must not half-close the socket.** An earlier version called `shutdown(Write)` after writing
the request, which is ordinarily polite; hyper reads that as an aborted connection and sends back
nothing at all, so the probe reported `unhealthy` against a perfectly healthy server. Write the
request, then read. No shutdown.

## Configuration

Upstream's variables all work the same way: `PORT`, `WS_FALLBACK`, `RATE_LIMIT`, `RTC_CONFIG`,
`SIGNALING_SERVER`, `IPV6_LOCALIZE`, `DEBUG_MODE`, and the `*_BUTTON_*` set. CLI flags
`--rate-limit`, `--include-ws-fallback` and `--localhost-only` work too.

Two additions:

| Variable | Default | Meaning |
| --- | --- | --- |
| `TRUST_CF_CONNECTING_IP` | `false` | Honour the `cf-connecting-ip` header when picking a client's IP. |
| `AUTO_CLOSE_RECEIVE_DIALOG` | `false` | Close the receive dialog once the download starts, instead of leaving it up with a "Download again" button. |

`AUTO_CLOSE_RECEIVE_DIALOG` is served to the client through `/config` and read in
`ReceiveFileDialog`. It deliberately does not apply when the browser offers the native share
sheet, since that is the user's own dialog and closing underneath it drops the file.

Upstream trusts that header unconditionally, so anyone can forge it and join an arbitrary IP room
([#510][510]). Turn it on only when Cloudflare is genuinely the only way in.

[510]: https://github.com/schlagmichdoch/PairDrop/issues/510

`X-Forwarded-For` is read the way upstream reads it, first entry wins, which is equally
forgeable by anything that can reach the port directly. Publish the container on loopback and let
the reverse proxy be the only route to it.

## Running it

```bash
cargo run                      # serves ./public on :3000
docker compose up -d --build   # build the scratch image locally, ~30 MB
```

Or pull the image built by CI for amd64 and arm64:

```bash
docker compose pull && docker compose up -d
```

To compare behaviour against upstream, restore its server from history and run it beside this
one:

```bash
git checkout fb743eb -- server package.json
npm install && PORT=3001 node server/index.js
```
