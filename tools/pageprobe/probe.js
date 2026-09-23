// Loads one overlay page under jsdom against the mock host and prints what the
// runtime published and what the stylesheet declares. jsdom does not lay out and
// does not evaluate calc(), so the resolved column is this file's own arithmetic
// over the declared text - a real browser is what proves the pixels.
"use strict";

const fs = require("fs");
const path = require("path");
const { JSDOM } = require("jsdom");
const { start } = require("./serve");

const ASSETS = path.resolve(__dirname, "../../../crates/forge-overlay/assets");
const SETTLE_MS = 250;

const PUBLISHED = [
  "--accent",
  "--font",
  "--element-width",
  "--element-height",
  "--text-size",
];

const CONTENT = {
  alert: { headline: "Thanks for the sub!", subline: "7 months subscribed" },
  chat: { author: "pixel_pal", author_color: "#89dceb", badges: "", message: "first time here, hi!" },
  goal: { label: "Sub goal", value: "42", target: "100" },
  ticker: { headline: "Latest cheer: 500 bits", subline: '"take my bits"' },
  frame: { headline: "", subline: "LIVE" },
};

function stylesheetText(kind) {
  return fs.readFileSync(path.join(ASSETS, kind, "overlay.css"), "utf8");
}

function declarations(css) {
  const out = [];
  let selector = "";
  for (const raw of css.split("\n")) {
    const line = raw.trim();
    if (line.endsWith("{")) {
      selector = line.slice(0, -1).trim();
    } else if (line.includes(":") && line.endsWith(";")) {
      out.push({ selector, text: line });
    }
  }
  return out;
}

function baseTextSize(css) {
  const found = css.match(/--text-scale: calc\(var\(--text-size, ([0-9.]+)\) \/ ([0-9.]+)\);/);
  if (!found || found[1] !== found[2]) {
    throw new Error("the stylesheet states no single-base --text-scale rule");
  }
  return Number(found[1]);
}

function resolve(text, scale, published) {
  const scaled = text.replace(
    /calc\((-?[0-9.]+)px \* var\(--text-scale\)\)/g,
    (_, value) => `${Number(value) * scale}px`,
  );
  return scaled.replace(
    /var\((--element-(?:width|height)), ([^)]+)\)/g,
    (_, property, fallback) => published[property] || fallback,
  );
}

async function probe(kind, config) {
  const host = await start({ kind, config, content: CONTENT[kind] });
  const dom = await JSDOM.fromURL(host.url, {
    runScripts: "dangerously",
    resources: "usable",
    pretendToBeVisual: true,
    beforeParse(window) {
      window.fetch = (target, options) =>
        fetch(new URL(target, window.location.href).href, options);
    },
  });
  await new Promise((done) => setTimeout(done, SETTLE_MS));

  const root = dom.window.document.documentElement;
  const published = {};
  for (const property of PUBLISHED) {
    const value = root.style.getPropertyValue(property);
    published[property] = value === "" ? null : value;
  }

  const css = stylesheetText(kind);
  const base = baseTextSize(css);
  const declared = published["--text-size"];
  const scale = declared === null ? 1 : Number(declared) / base;

  const linked = dom.window.document.querySelector('link[rel="stylesheet"]');
  const stage = dom.window.document.getElementById("stage");
  const bound = [...dom.window.document.querySelectorAll("[data-bind]")].map(
    (node) => `${node.getAttribute("data-bind")}=${JSON.stringify(node.textContent)}`,
  );

  const derived = declarations(css)
    .filter(({ text }) => text.includes("var(--text-scale)") || text.includes("var(--element-"))
    .map(({ selector, text }) => ({
      selector,
      declared: text,
      resolved: resolve(text, scale, published),
    }));

  console.log(`\n=== ${kind}  config=${JSON.stringify(config)}`);
  console.log(`stylesheet linked: ${linked ? linked.href : "NONE"}`);
  console.log(`base text size: ${base}px   ratio: ${scale}`);
  console.log("published custom properties (inline on <html>):");
  for (const property of PUBLISHED) {
    console.log(`  ${property} = ${published[property] === null ? "(unset -> css fallback)" : published[property]}`);
  }
  console.log(`#stage class: ${JSON.stringify(stage ? stage.className : null)}`);
  console.log(`bound text: ${bound.join(", ") || "(none)"}`);
  console.log("derived declarations:");
  for (const row of derived) {
    console.log(`  ${row.selector.padEnd(34)} ${row.declared}`);
    console.log(`  ${"".padEnd(34)} -> ${row.resolved}`);
  }

  dom.window.close();
  await host.close();
}

async function main() {
  const base = {
    accent: "mauve",
    font: "Inter",
    position: "top",
    animation: "fade",
  };
  const textSize = { alert: 20, chat: 13, goal: 14, ticker: 19, frame: 13 };

  for (const kind of ["alert", "goal", "chat", "ticker", "frame"]) {
    await probe(kind, { ...base, text_size: textSize[kind] });
    await probe(kind, { ...base, text_size: textSize[kind] * 2 });
    await probe(kind, { ...base, text_size: textSize[kind], element_width: 700 });
  }
  await probe("alert", base);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
