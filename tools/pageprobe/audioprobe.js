"use strict";
// audioprobe.js <page_url> <action_do_url> <bearer_token> <wait_ms>
// Opens an audio overlay page, fires an action over HTTP once the page is live,
// and records every /audio/v1/ exchange plus the page's Audio element events.
const puppeteer = require("puppeteer");
(async () => {
  const [pageUrl, actionUrl, token, waitMs] = process.argv.slice(2);
  const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox", "--autoplay-policy=no-user-gesture-required"] });
  const page = await browser.newPage();
  const exchanges = [];
  page.on("request", (r) => { if (r.url().includes("/audio/v1/")) exchanges.push({ t: Date.now(), req: r.method() + " " + r.url(), body: r.postData() || null }); });
  page.on("response", (r) => { if (r.status() >= 400) exchanges.push({ t: Date.now(), failed: r.status() + " " + r.url() }); });
  page.on("response", (r) => { if (r.url().includes("/audio/v1/")) exchanges.push({ t: Date.now(), res: r.status() + " " + r.url(), type: r.headers()["content-type"] || null, length: r.headers()["content-length"] || null }); });
  page.on("console", (m) => exchanges.push({ t: Date.now(), console: m.text().slice(0, 200) }));
  await page.evaluateOnNewDocument(() => {
    window.__audio = [];
    const Native = window.Audio;
    window.Audio = function (src) {
      const el = new Native(src);
      const rec = { events: [] };
      window.__audio.push(rec);
      for (const name of ["loadedmetadata", "playing", "ended", "error", "pause"]) el.addEventListener(name, () => rec.events.push(name + (name === "loadedmetadata" ? ":" + el.duration.toFixed(2) : "")));
      return el;
    };
  });
  await page.goto(pageUrl, { waitUntil: "load" });
  await new Promise((r) => setTimeout(r, 2500));
  const fired = await fetch(actionUrl, { method: "POST", headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" }, body: JSON.stringify({ args: {} }) });
  const firedAt = Date.now();
  console.log("action http " + fired.status + " " + (await fired.text()).slice(0, 300));
  await new Promise((r) => setTimeout(r, Number(waitMs || 8000)));
  const audio = await page.evaluate(() => window.__audio);
  console.log(JSON.stringify({ firedAt, exchanges: exchanges.map((e) => ({ ...e, t: e.t - firedAt })), audio }, null, 1));
  await browser.close();
})().catch((e) => { console.error("failed:", e.message); process.exit(1); });
