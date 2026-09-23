"use strict";
const puppeteer = require("puppeteer");
(async () => {
  const [url, out] = process.argv.slice(2);
  const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox"] });
  const page = await browser.newPage();
  const media = [];
  page.on("response", (r) => { if (r.url().includes("/forge-media/")) media.push({ url: r.url().split("/").pop(), status: r.status(), type: r.headers()["content-type"] }); });
  await page.setViewport({ width: 1920, height: 1080 });
  await page.goto(url, { waitUntil: "load" });
  await new Promise((r) => setTimeout(r, 1500));
  const icon = await page.evaluate(() => {
    const el = document.querySelector(".icon"); if (!el) return null;
    const cs = getComputedStyle(el); const r = el.getBoundingClientRect();
    return { hidden: el.hidden, w: r.width, h: r.height, tinted: el.classList.contains("tinted"), background: cs.backgroundColor,
             mask: (cs.maskImage || cs.webkitMaskImage || "").slice(0, 60), img: !!el.querySelector("img"), svgInDom: !!document.querySelector("svg"),
             accent: getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() };
  });
  if (out) { const stage = await page.$("#stage"); await (stage || page).screenshot({ path: out }); }
  console.log(JSON.stringify({ media, icon }, null, 1));
  await browser.close();
})().catch((e) => { console.error("failed:", e.message); process.exit(1); });
