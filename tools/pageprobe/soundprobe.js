"use strict";
const puppeteer = require("puppeteer");
(async () => {
  const [url, waitMs] = process.argv.slice(2);
  const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox", "--autoplay-policy=no-user-gesture-required"] });
  const page = await browser.newPage();
  const media = [];
  page.on("response", (r) => { if (/\.(wav|mp3|ogg|flac|m4a)(\?|$)/.test(r.url())) media.push({ url: r.url(), status: r.status(), type: r.headers()["content-type"], cache: r.headers()["cache-control"] || null }); });
  await page.evaluateOnNewDocument(() => {
    window.__audio = [];
    const Native = window.Audio;
    window.Audio = function (src) {
      const el = new Native(src);
      const rec = { src: String(src), events: [] };
      window.__audio.push(rec);
      for (const name of ["loadedmetadata", "playing", "ended", "error"]) el.addEventListener(name, () => rec.events.push(name + (name === "loadedmetadata" ? ":" + el.duration.toFixed(2) : "")));
      return el;
    };
  });
  await page.goto(url, { waitUntil: "load" });
  console.log("READY");
  await new Promise((r) => setTimeout(r, Number(waitMs || 8000)));
  const audio = await page.evaluate(() => window.__audio);
  console.log(JSON.stringify({ media, audio }, null, 1));
  await browser.close();
})().catch((e) => { console.error("failed:", e.message); process.exit(1); });
