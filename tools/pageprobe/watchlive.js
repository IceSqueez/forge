"use strict";
const puppeteer = require("puppeteer");
(async () => {
  const [url, secs] = process.argv.slice(2);
  const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox"] });
  const page = await browser.newPage();
  await page.setViewport({ width: 1920, height: 1080 });
  let loads = 0; page.on("load", () => { loads += 1; });
  await page.goto(url, { waitUntil: "load" });
  let last = "";
  const t0 = Date.now();
  while (Date.now() - t0 < Number(secs) * 1000) {
    await new Promise((r) => setTimeout(r, 500));
    const now = await page.evaluate(() => { const r = document.querySelector("#stage").getBoundingClientRect(); const h = document.querySelector(".headline"); return `${Math.round(r.width)}x${Math.round(r.height)} headline=${getComputedStyle(h).fontSize}`; }).catch(() => "navigating");
    if (now !== last) { console.log(`+${((Date.now() - t0) / 1000).toFixed(1)}s loads=${loads} ${now}`); last = now; }
  }
  await browser.close();
})().catch((e) => { console.error("failed:", e.message); process.exit(1); });
