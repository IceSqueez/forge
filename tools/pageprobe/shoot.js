"use strict";
const { start } = require("./serve.js");
const puppeteer = require("puppeteer");
const cases = JSON.parse(process.argv[2]);
(async () => {
  const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox", "--autoplay-policy=no-user-gesture-required"] });
  for (const c of cases) {
    const host = await start({ kind: c.kind, config: c.config, content: c.content });
    const page = await browser.newPage();
    await page.setViewport({ width: 1920, height: 1080 });
    await page.goto(`${host.url}${c.query || "?preview=1"}`, { waitUntil: "load" });
    await new Promise((r) => setTimeout(r, 1500));
    const box = await page.evaluate(() => {
      const el = document.querySelector("#stage");
      const r = el.getBoundingClientRect();
      const cs = (sel) => { const n = document.querySelector(sel); return n ? getComputedStyle(n).fontSize : null; };
      return { stage: { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width * 10) / 10, h: Math.round(r.height * 10) / 10 },
               cls: el.className, headline: cs(".headline") || cs(".label") || cs(".message"), subline: cs(".subline") || cs(".figure"),
               padding: getComputedStyle(el).padding, texts: [...document.querySelectorAll("[data-bind]")].map((n) => n.textContent) };
    });
    await page.screenshot({ path: `${__dirname}/shots/${c.name}.png`, clip: c.clip });
    console.log(c.name, JSON.stringify(box));
    await page.close();
    await host.close();
  }
  await browser.close();
  process.exit(0);
})().catch((e) => { console.error("shoot failed:", e.message); process.exit(1); });
