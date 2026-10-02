"use strict";
const { start } = require("./serve.js");
const puppeteer = require("puppeteer");

const SETTLE_MS = 1200;
const TOLERANCE_PX = 0.5;
const LONG_WORD = "Supercalifragilisticexpialidocious".repeat(6);

const margins = (top, right, bottom, left) => ({
  margin_top: top,
  margin_right: right,
  margin_bottom: bottom,
  margin_left: left,
});

const box = (width, height) => ({ design_width: width, design_height: height });

const CASES = [
  {
    name: "alert-default",
    kind: "alert",
    config: { ...box(800, 600), ...margins(10, 10, 10, 10), text_size: 20 },
    content: { headline: "Thanks for the sub!", subline: "Three months" },
  },
  {
    name: "alert-long-word-uneven-margins",
    kind: "alert",
    config: { ...box(800, 600), ...margins(30, 15, 15, 15), text_size: 40 },
    content: { headline: LONG_WORD, subline: LONG_WORD },
  },
  {
    name: "ticker-default",
    kind: "ticker",
    config: { ...box(1920, 96), ...margins(0, 0, 0, 0), text_size: 19 },
    content: { headline: LONG_WORD, subline: LONG_WORD },
  },
  {
    name: "goal-default",
    kind: "goal",
    config: { ...box(640, 160), ...margins(0, 0, 0, 0), text_size: 14 },
    content: { label: LONG_WORD, value: "42", target: "100" },
  },
  {
    name: "chat-default",
    kind: "chat",
    config: { ...box(400, 600), ...margins(0, 0, 0, 0), text_size: 13, position: "bottom" },
    content: { author: LONG_WORD, message: LONG_WORD, badges: "" },
  },
  {
    name: "frame-default-caption-bottom",
    kind: "frame",
    config: { ...box(1280, 720), ...margins(0, 0, 0, 0), text_size: 13, position: "bottom" },
    content: { headline: "Now playing", subline: "LIVE" },
  },
  {
    name: "frame-caption-top-large-text",
    kind: "frame",
    config: { ...box(1280, 720), ...margins(5, 5, 5, 5), text_size: 40, position: "top" },
    content: { headline: "Now playing", subline: "LIVE" },
  },
];

async function measure(page) {
  return page.evaluate(() => {
    const body = document.body;
    const style = getComputedStyle(body);
    const area = {
      left: parseFloat(style.paddingLeft),
      top: parseFloat(style.paddingTop),
      right: body.clientWidth - parseFloat(style.paddingRight),
      bottom: body.clientHeight - parseFloat(style.paddingBottom),
    };
    const painted = [];
    const clipOf = (node) => {
      let clip = { left: -Infinity, top: -Infinity, right: Infinity, bottom: Infinity };
      for (let up = node.parentElement; up && up !== body; up = up.parentElement) {
        if (getComputedStyle(up).overflow === "visible") continue;
        const r = up.getBoundingClientRect();
        clip = {
          left: Math.max(clip.left, r.left),
          top: Math.max(clip.top, r.top),
          right: Math.min(clip.right, r.right),
          bottom: Math.min(clip.bottom, r.bottom),
        };
      }
      return clip;
    };
    const keep = (label, rect, owner) => {
      const clip = clipOf(owner);
      const seen = {
        at: label,
        left: Math.max(rect.left, clip.left),
        top: Math.max(rect.top, clip.top),
        right: Math.min(rect.right, clip.right),
        bottom: Math.min(rect.bottom, clip.bottom),
      };
      if (seen.right - seen.left <= 0 || seen.bottom - seen.top <= 0) return;
      painted.push(seen);
    };
    for (const node of body.querySelectorAll("*")) {
      if (node.tagName === "SCRIPT") continue;
      if (getComputedStyle(node).visibility === "hidden") continue;
      keep(node.id || node.className || node.tagName, node.getBoundingClientRect(), node);
    }
    const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT);
    for (let text = walker.nextNode(); text; text = walker.nextNode()) {
      if (!text.data.trim()) continue;
      const range = document.createRange();
      range.selectNodeContents(text);
      for (const rect of range.getClientRects()) {
        keep(`text:${text.parentElement.className}`, rect, text);
      }
    }
    return {
      box: { width: window.innerWidth, height: window.innerHeight },
      area,
      painted,
    };
  });
}

function breaches(name, measured) {
  const out = [];
  const { area, painted } = measured;
  for (const rect of painted) {
    const over = Math.max(
      area.left - rect.left,
      area.top - rect.top,
      rect.right - area.right,
      rect.bottom - area.bottom,
    );
    if (over > TOLERANCE_PX) {
      out.push(`${name}: ${rect.at} leaves the content area by ${over.toFixed(1)}px`);
    }
  }
  return out;
}

(async () => {
  const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox"] });
  const failures = [];
  for (const c of CASES) {
    const host = await start({ kind: c.kind, config: c.config, content: c.content });
    const page = await browser.newPage();
    await page.setViewport({ width: c.config.design_width, height: c.config.design_height });
    await page.goto(`${host.url}?preview=1`, { waitUntil: "load" });
    await new Promise((r) => setTimeout(r, SETTLE_MS));
    const measured = await measure(page);
    const found = breaches(c.name, measured);
    console.log(c.name, found.length === 0 ? "inside" : `${found.length} breach(es)`);
    failures.push(...found);
    await page.close();
    await host.close();
  }
  await browser.close();
  for (const failure of failures) console.error(failure);
  process.exit(failures.length === 0 ? 0 : 1);
})().catch((e) => {
  console.error("boxprobe failed:", e.message);
  process.exit(1);
});
