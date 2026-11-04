// Reads one overlay kind's page files the way forge composes them: the
// include_str! list in the kind's page_assets (crates/forge-overlay/src/kinds)
// names the markup, then the stylesheet, then every behavior script in order.
"use strict";

const fs = require("fs");
const path = require("path");

const CRATE = path.resolve(__dirname, "../../crates/forge-overlay");
const KINDS = path.join(CRATE, "src/kinds");
const INCLUDE = /include_str!\("([^"]+)"\)/g;

function includes(kind) {
  const source = fs.readFileSync(path.join(KINDS, `${kind}.rs`), "utf8");
  const start = source.indexOf("fn page_assets");
  if (start < 0) {
    throw new Error(`kind ${kind} declares no page_assets`);
  }
  const body = source.slice(start, source.indexOf("\n    }\n", start));
  return Array.from(body.matchAll(INCLUDE), (match) =>
    path.resolve(KINDS, match[1]),
  );
}

function kindAssets(kind) {
  const [markup, style, ...behavior] = includes(kind);
  if (!markup || !style || behavior.length === 0) {
    throw new Error(`kind ${kind} page_assets is not markup, style, behavior`);
  }
  return {
    "index.html": fs.readFileSync(markup),
    "overlay.css": fs.readFileSync(style),
    "overlay.js": Buffer.concat(behavior.map((file) => fs.readFileSync(file))),
  };
}

module.exports = { kindAssets, ASSETS: path.join(CRATE, "assets") };
