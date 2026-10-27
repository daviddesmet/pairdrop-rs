<div align="center">
  <a href="https://github.com/daviddesmet/pairdrop-rs">
    <img src="public/images/android-chrome-512x512.png" alt="Logo"  width="150" height="150">
  </a>

  # pairdrop-rs

  <p>
    Local file sharing <strong>in your web browser</strong>.
    <br>
    Inspired by Apple's AirDrop.
    <br>
    A fork of <a href="https://github.com/schlagmichdoch/PairDrop">PairDrop</a>, which is a fork of
    <a href="https://github.com/RobinLinus/snapdrop">Snapdrop</a>,
    <br>
    with the Node.js signaling server rewritten in Rust.
    <br>
    <br>
    <a href="https://github.com/daviddesmet/pairdrop-rs/issues">Report a bug</a>
    <br />
    <a href="https://github.com/daviddesmet/pairdrop-rs/issues">Request feature</a>
  </p>
</div>
<br>

## Why this fork

[PairDrop][up] is excellent and effectively unmaintained: the last substantive commit upstream is
2025-02-25, and community pull requests sit unmerged. This fork keeps the web client exactly as
it is and replaces the Node signaling server with a Rust one, so the parts that needed fixing
could be fixed.

| | upstream | here |
| --- | --- | --- |
| Signaling server | Node + Express + `ws` | Rust + axum, one static binary |
| Container image | 200 MB (`linuxserver/pairdrop`) | **30 MB**, `scratch`, no shell or package manager |
| Service worker cache | named after the release version | named after a digest of `public/` |
| Asset responses | whatever Express sends | `Cache-Control: no-cache`, ETags still answer `304` |
| `cf-connecting-ip` | always trusted | only under `TRUST_CF_CONNECTING_IP` |
| Healthcheck | `wget` from the base image | the binary probes itself |

### Image size

Measured with `docker image inspect` on the same host, both serving the same client:

| Image | On disk | Contents |
| --- | ---: | --- |
| `lscr.io/linuxserver/pairdrop:latest` | 200 MB | Alpine, the s6 service supervisor, the Node runtime and its npm dependencies, then the app. Largest single layer: 99 MB. |
| `pairdrop-rs:latest` | **30 MB** | `public/` (14 MB) and a 2.4 MB statically linked binary, with no base image at all. |

The server is the smallest part of its own image: almost all of the 30 MB is the web client, of
which the bundled fonts and 32 translation files are the bulk. Dropping the Alpine base was worth
17 MB on its own, and became possible once `HEALTHCHECK` stopped needing `wget`. See
[docs/rust-server.md](docs/rust-server.md).

The client is unchanged PairDrop and the wire protocol is identical, so stock clients work
against this server and the original server works with this client. Upstream's `server/` was
removed once the port was verified against it; read it at `git show fb743eb:server/` if you need
to compare.

**The cache fix is the reason this exists.** PairDrop's service worker is cache-first with no
revalidation and only drops caches whose name differs from the current one, which upstream
hardcodes as the release version. Once a cache holds a stale `index.html`, every deferred script
loads twice (`Identifier 'Events' has already been declared`), `network.js` dies, the WebSocket
never opens and your display name never appears. A hard refresh bypasses the service worker and
looks fine; the next normal load is broken again. Nothing you can configure in a reverse proxy
touches it. The only escape is clearing site data by hand on every device ([#384][384]).

Naming the cache after a digest of `public/` means any asset change renames it, `activate`
deletes the old one, and everything is re-fetched. The problem cannot survive a deploy.

See [docs/rust-server.md](docs/rust-server.md) for the details, the full configuration, and the
hyper half-close gotcha behind the self-probing healthcheck.

[up]: https://github.com/schlagmichdoch/PairDrop
[384]: https://github.com/schlagmichdoch/PairDrop/issues/384

## Features
File sharing on your local network that works on all platforms.

- A multi-platform AirDrop-like solution that works.
  - Send images, documents or text via peer-to-peer connection to devices on the same local network.
- Internet transfers
  - Join temporary public rooms to transfer files easily over the Internet.
- Web-app 
  - Works on all devices with a modern web-browser.
 
Send a file from your phone to your laptop?
<br>Share photos in original quality with friends using Android and iOS?
<br>Share private files peer-to-peer between Linux systems?

<img src="docs/pairdrop_screenshot_mobile.gif" alt="Screenshot GIF showing PairDrop in use" style="width: 300px">

## Differences to the [Snapdrop](https://github.com/RobinLinus/snapdrop) it is based on
<details><summary>View all differences</summary>

### Paired Devices and Public Rooms — Internet Transfer
* Transfer files over the Internet between paired devices or by entering temporary public rooms.
* Connect to devices in complex network environments (public Wi-Fi, company network, iCloud Private Relay, VPN, etc.).
* Connect to devices on your mobile hotspot.
* Devices outside of your local network that are behind a NAT are auto-connected via the PairDrop TURN server.
* Devices from the local network, in the same public room, or previously paired are shown.

#### Persistent Device Pairing

Always connect to known devices

* Pair devices via a 6-digit code or a QR-Code.
* Paired devices always find each other via shared secrets independently of their local network. 
* Pairing is persistent. You find your devices even after reopening PairDrop.
* You can edit and unpair devices easily.

#### Temporary Public Rooms

Connect to others in complex network situations, or over the Internet.

* Enter a public room via a 5-letter code or a QR-code.
* Enter a public room to temporarily connect to devices outside your local network.
* All devices in the same public room see each other.
* Public rooms are temporary. Closing PairDrop  leaves all rooms.

### [Improved UI for Sending/Receiving Files](https://github.com/RobinLinus/snapdrop/issues/560)
* Files are transferred after a request is accepted. Files are auto-downloaded upon completing a transfer, if possible.
* Multiple files are downloaded as a ZIP file
* Download, share or save to gallery via the "Share" menu on Android and iOS.
* Multiple files are transferred at once with an overall progress indicator.

### Send Files or Text Directly From Share Menu, Context Menu or CLI
* [Send files directly from context menu on Ubuntu (using Nautilus)](docs/how-to.md#send-multiple-files-and-directories-directly-from-context-menu-on-ubuntu-using-nautilus)
* [Send files directly from the context menu on Windows](docs/how-to.md#send-files-directly-from-context-menu-on-windows)
* [Send directly from the "Share" menu on iOS](docs/how-to.md#send-directly-from-share-menu-on-ios)
* [Send directly from the "Share" menu on Android](docs/how-to.md#send-directly-from-share-menu-on-android)
* [Send directly via the command-line interface](docs/how-to.md#send-directly-via-command-line-interface)

### Other Changes
* Change your display name to easily differentiate your devices.
* [Paste files/text and choose the recipient afterwards ](https://github.com/RobinLinus/snapdrop/pull/534)
* [Prevent devices from sleeping on file transfer](https://github.com/RobinLinus/snapdrop/pull/413)
* Warn user before PairDrop is closed on file transfer
* Open PairDrop on multiple tabs simultaneously (Thanks [@willstott101](https://github.com/willstott101))
* [Video and audio preview](https://github.com/RobinLinus/snapdrop/pull/455) (Thanks [@victorwads](https://github.com/victorwads))
* Switch theme back to auto/system after dark or light mode is on
* Node-only implementation (Thanks [@Bellisario](https://github.com/Bellisario))
* Auto-restart on error (Thanks [@KaKi87](https://github.com/KaKi87))
* Lots of stability fixes (Thanks [@MWY001](https://github.com/MWY001) [@skiby7](https://github.com/skiby7) and [@willstott101](https://github.com/willstott101))
* To host PairDrop on your local network (e.g. on Raspberry Pi): [All peers connected with private IPs are discoverable by each other](https://github.com/RobinLinus/snapdrop/pull/558)
* When hosting PairDrop yourself, you can [set your own STUN/TURN servers](docs/host-your-own.md#specify-stunturn-servers)
* Translations.

</details>

## Translate PairDrop on [Hosted Weblate](https://hosted.weblate.org/engage/pairdrop/)
<a href="https://hosted.weblate.org/engage/pairdrop/">
<img src="https://hosted.weblate.org/widget/pairdrop/horizontal-blue.svg" alt="Translation status" style="width: 300px" />
</a>

Translations live in upstream's Weblate project, not here. Contributions there reach this fork
whenever `public/lang/` is synced from upstream.

## Built with the following awesome technologies:

Server:
* [Rust](https://www.rust-lang.org/) with [tokio](https://tokio.rs/) async runtime
* [axum](https://github.com/tokio-rs/axum) HTTP and WebSocket handling
* [tower-http](https://github.com/tower-rs/tower-http) static file serving
* [serde](https://serde.rs/) JSON, [sha2](https://github.com/RustCrypto/hashes) asset digests and peer id hashes

Client:
* Vanilla HTML5 / JS ES6 / CSS 3 frontend
* [WebRTC](http://webrtc.org/) / WebSockets
* [Progressive web app (PWA)](https://en.wikipedia.org/wiki/Progressive_web_app) unified functionality
* [IndexedDB API](https://developer.mozilla.org/en-US/docs/Web/API/IndexedDB_API) storage handling
* [zip.js](https://gildas-lormeau.github.io/zip.js/) library
* [cyrb53](https://github.com/bryc/code/blob/master/jshash/experimental/cyrb53.js) super-fast hash function, in both the client and the server
* [NoSleep](https://github.com/richtr/NoSleep.js) display sleep, add wake lock ([MIT](licenses/MIT-NoSleep))
* [heic2any](https://github.com/alexcorvi/heic2any) HEIC/HEIF to PNG/GIF/JPEG ([MIT](licenses/MIT-heic2any))
* [Weblate](https://weblate.org/) web-based localization tool

Gone with the Node server: Express, `ws`, `express-rate-limit`, `ua-parser-js` and
`unique-names-generator`, all reimplemented in `src/`. There are no npm dependencies left.

[FAQ](docs/faq.md)

[Host your own instance with Docker or from source](docs/host-your-own.md).

```bash
docker pull ghcr.io/daviddesmet/pairdrop-rs:latest
```

## Support upstream

This fork takes no donations. PairDrop is libre and always will be, and the work this is built on
is schlagmichdoch's. If you find it useful, support the original:

<a href="https://www.buymeacoffee.com/pairdrop" target="_blank">
<img src="https://cdn.buymeacoffee.com/buttons/v2/default-blue.png" alt="Buy me a coffee" style="height: 60px !important;width: 217px !important;" >
</a>

## Contributing
Feel free to [open an issue](https://github.com/daviddesmet/pairdrop-rs/issues/new/choose) or a
[pull request](https://github.com/daviddesmet/pairdrop-rs/pulls), following the
[Contributing Guidelines](CONTRIBUTING.md).

Issues with the client itself, rather than this fork's server, may be better raised
[upstream](https://github.com/schlagmichdoch/PairDrop/issues).
