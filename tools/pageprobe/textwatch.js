"use strict";
const puppeteer = require("puppeteer");
(async () => {
  const [url, secs, out] = process.argv.slice(2);
  const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox"] });
  const page = await browser.newPage();
  await page.setViewport({ width: 1920, height: 1080 });
  await page.goto(url, { waitUntil: "load" });
  const t0 = Date.now(); let last = "";
  while (Date.now() - t0 < Number(secs) * 1000) {
    const now = await page.evaluate(() => [...document.querySelectorAll("[data-bind]")].map((n) => n.getAttribute("data-bind") + "=" + JSON.stringify(n.textContent)).join(" ") + " hidden=" + (document.querySelector("#stage")?.classList.contains("hidden")));
    if (now !== last) { console.log(`+${((Date.now() - t0) / 1000).toFixed(1)}s ${now}`); last = now; if (out && !now.includes("hidden=true")) await page.screenshot({ path: out }); }
    await new Promise((r) => setTimeout(r, 200));
  }
  await browser.close();
})().catch((e) => { console.error("failed:", e.message); process.exit(1); });
