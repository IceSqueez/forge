"use strict";
const puppeteer = require("puppeteer");
(async () => {
  const [url, out] = process.argv.slice(2);
  const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox"] });
  const page = await browser.newPage();
  await page.setViewport({ width: 1920, height: 1080 });
  await page.goto(url, { waitUntil: "load" });
  await new Promise((r) => setTimeout(r, 1500));
  const box = await page.evaluate(() => {
    const el = document.querySelector("#stage"); const r = el.getBoundingClientRect();
    const fs = (s) => { const n = document.querySelector(s); return n ? getComputedStyle(n).fontSize : null; };
    return { w: Math.round(r.width * 10) / 10, h: Math.round(r.height * 10) / 10, headline: fs(".headline"), subline: fs(".subline"),
             texts: [...document.querySelectorAll("[data-bind]")].map((n) => n.textContent) };
  });
  if (out) await page.screenshot({ path: out });
  console.log(JSON.stringify(box));
  await browser.close();
})().catch((e) => { console.error("failed:", e.message); process.exit(1); });
