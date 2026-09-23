// Measures whether any descendant of #stage paints outside the stage border box.
// Same case list shape as shoot.js.
"use strict";
const { start } = require("./serve.js");
const puppeteer = require("puppeteer");
const cases = JSON.parse(process.argv[2]);

(async () => {
  const browser = await puppeteer.launch({
    headless: true,
    args: ["--no-sandbox", "--autoplay-policy=no-user-gesture-required"],
  });
  for (const c of cases) {
    const host = await start({ kind: c.kind, config: c.config, content: c.content });
    const page = await browser.newPage();
    await page.setViewport({ width: 1920, height: 1080 });
    await page.goto(`${host.url}${c.query || "?preview=1"}`, { waitUntil: "load" });
    await new Promise((r) => setTimeout(r, 1200));
    const out = await page.evaluate(() => {
      const round = (n) => Math.round(n * 10) / 10;
      const stage = document.querySelector("#stage");
      const box = stage.getBoundingClientRect();
      let worst = null;
      const consider = (label, rect) => {
        if (rect.width === 0 && rect.height === 0) return;
        const over = round(rect.right - box.right);
        if (!worst || over > worst.over) {
          worst = { at: label, over, w: round(rect.width) };
        }
      };
      for (const node of stage.querySelectorAll("*")) {
        consider(node.className || node.tagName, node.getBoundingClientRect());
      }
      const walker = document.createTreeWalker(stage, NodeFilter.SHOW_TEXT);
      for (let text = walker.nextNode(); text; text = walker.nextNode()) {
        if (!text.data.trim()) continue;
        const range = document.createRange();
        range.selectNodeContents(text);
        for (const rect of range.getClientRects()) {
          consider(`text:${text.parentElement.className}`, rect);
        }
      }
      return {
        stage: { w: round(box.width), h: round(box.height) },
        scrollW: round(stage.scrollWidth),
        worstChild: worst,
      };
    });
    console.log(c.name, JSON.stringify(out));
    await page.close();
    await host.close();
  }
  await browser.close();
  process.exit(0);
})().catch((e) => {
  console.error("overflow failed:", e.message);
  process.exit(1);
});
