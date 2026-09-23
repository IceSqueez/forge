# pageprobe

Headless Chromium probes for overlay pages served by a running forge. Each script
opens one page URL, drives it, and prints JSON you can assert on. Nothing here ships
with forge; the scripts exist so a page can be verified without OBS.

Setup once: `npm ci` (downloads Chromium for puppeteer on first run).

| Script | Usage | Records |
| :-- | :-- | :-- |
| `livepage.js` | `node livepage.js <page url>` | the `#stage` box and the font sizes the page resolved |
| `textwatch.js` | `node textwatch.js <page url> <seconds> <png>` | bound text and hidden state over time, plus a screenshot |
| `soundprobe.js` | `node soundprobe.js <page url> <wait ms>` | media responses and `Audio` element events |
| `audioprobe.js` | `node audioprobe.js <page url> <action :do url> <bearer token> <wait ms>` | an audio overlay's clip fetch, playback events and verdict report after firing an action over HTTP |
| `iconprobe.js` | `node iconprobe.js <page url> <png>` | the alert icon's geometry and tint, and whether any SVG was injected |
| `watchlive.js` | `node watchlive.js <page url> ...` | frames a live page receives |
| `serve.js` | required by `shoot.js` / `overflow.js` / `probe.js` | a mock page host serving the crate assets plus a config you pass |
| `shoot.js` | `node shoot.js '<json cases>'` | screenshots of one overlay kind under several configs |
| `overflow.js` | `node overflow.js '<json cases>'` | whether anything paints outside the stage |
| `probe.js` | `node probe.js ...` | jsdom read of what the runtime published and what the stylesheet declares |

Page URLs look like `http://127.0.0.1:<port>/overlays/<identity>/`. The server port and
bearer token come from the forge emulator's `seed` output or from Settings ->
WebSocket server.
