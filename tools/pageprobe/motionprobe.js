"use strict";
const { start } = require("./serve.js");
const puppeteer = require("puppeteer");

const BOX = {
  design_width: 800,
  design_height: 600,
  margin_top: 10,
  margin_right: 10,
  margin_bottom: 10,
  margin_left: 10,
  text_size: 20,
  accent: "mauve",
  font: "Rubik",
};
const WINDOW_MS = 1000;
const TEXT_WINDOW_MS = 2000;
const EXIT_MS = 400;
const ENTRANCE_SHARE = 0.75;
const PREVIEW_PAUSE_MS = 1200;
const EARLY_SLACK_MS = 40;
const LATE_SLACK_MS = 200;
const SETTLE_MS = 250;
const IDLE_WATCH_MS = 300;
const MAX_CLONES = 48;
const WAIT_MS = 8000;

const RECT_EXITS = ["fade", "slide-up", "slide-down", "slide-left", "slide-right", "pop", "wipe"];
const RECT_ENTRANCES = RECT_EXITS;
const PARTICLE_ENTRANCES = ["sparks", "assemble", "glow-burst"];
const LIFTED_ENTRANCES = ["glow-burst"];
const ENTRANCE_MS = 500;
const ENTRANCE_WINDOW_MS = 4000;
const MID_ENTRANCE_MS = 150;
const ENTRANCE_SETTLED_MS = 1200;
const IN_FLIGHT_MS = 210;
const TRACE_BOX = { width: 600, height: 300 };
const DESTRUCTIVE_EXITS = ["dissolve", "smoke", "shatter", "dust", "burst"];
const TEXT_EFFECTS = ["typewriter", "fly-in", "spin", "wave", "bounce"];
const UNITS = ["letter", "word"];

const ALERT_CONTENT = { headline: "Thanks for the sub!", subline: "Three months" };
const UNICODE_HEADLINE = "Дякую 👩‍👩‍👧 café ﬁx";
const LONG_HEADLINE = "Supercalifragilistic ".repeat(7).trim();

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function instrument() {
  window.__probe = { marks: [], rafs: 0, errors: [] };
  const raf = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (callback) => {
    window.__probe.rafs += 1;
    return raf(callback);
  };
  window.addEventListener("error", (event) => window.__probe.errors.push(String(event.message)));
  document.addEventListener("DOMContentLoaded", () => {
    const stage = document.getElementById("stage");
    if (!stage) return;
    const note = () =>
      window.__probe.marks.push({ t: performance.now(), hidden: stage.classList.contains("hidden") });
    note();
    new MutationObserver(note).observe(stage, { attributes: true, attributeFilter: ["class"] });
  });
}

function alertConfig(extra) {
  return {
    ...BOX,
    duration: WINDOW_MS / 1000,
    entrance: "fade",
    entrance_ms: 300,
    text_effect: "none",
    text_unit: "letter",
    text_stagger_custom: false,
    exit: "fade",
    exit_ms: EXIT_MS,
    intensity: "medium",
    ...extra,
  };
}

async function open(browser, { kind = "alert", config, content, overrides, preview = false }) {
  const host = await start({ kind, config, content, overrides });
  const page = await browser.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error.message || error)));
  await page.setViewport({ width: config.design_width, height: config.design_height });
  await page.evaluateOnNewDocument(instrument);
  await page.goto(preview ? `${host.url}?preview=1` : host.url, { waitUntil: "load" });
  return {
    page,
    host,
    pageErrors,
    close: async () => {
      await page.close();
      await host.close();
    },
  };
}

function shownAt(page, nth = 1) {
  return page
    .waitForFunction(
      (n) => {
        const reveals = window.__probe.marks.filter((mark, i, all) => !mark.hidden && (i === 0 || all[i - 1].hidden));
        return reveals.length >= n ? reveals[n - 1].t : false;
      },
      { timeout: WAIT_MS, polling: 5 },
      nth,
    )
    .then((handle) => handle.jsonValue());
}

function hiddenAfter(page, since) {
  return page
    .waitForFunction(
      (t) => {
        const mark = window.__probe.marks.find((m) => m.hidden && m.t > t);
        return mark ? mark.t : false;
      },
      { timeout: WAIT_MS, polling: 5 },
      since,
    )
    .then((handle) => handle.jsonValue());
}

function until(page, at) {
  return page.waitForFunction((t) => performance.now() >= t, { timeout: WAIT_MS, polling: 5 }, at);
}

function sampleInMotion() {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const layers = Array.from(document.querySelectorAll("[data-forge-motion]"));
  const outside = layers
    .map((layer) => {
      const r = layer.getBoundingClientRect();
      const clipped = getComputedStyle(layer).overflow === "hidden";
      const over = Math.max(-r.left, -r.top, r.right - vw, r.bottom - vh);
      return !clipped ? "a motion layer is not clipped" : over > 0.5 ? `a motion layer reaches ${over.toFixed(1)}px past the box` : "";
    })
    .filter(Boolean);
  const scroller = document.scrollingElement;
  if (scroller.scrollWidth > vw || scroller.scrollHeight > vh) {
    outside.push(`the page scrolls (${scroller.scrollWidth}x${scroller.scrollHeight} over ${vw}x${vh})`);
  }
  const clones = layers.reduce((sum, layer) => sum + layer.querySelectorAll("#stage").length, 0);
  return {
    running: document.getAnimations().filter((a) => a.playState === "running").length,
    layers: layers.length,
    canvases: document.querySelectorAll("[data-forge-motion] canvas").length,
    clones,
    outside,
    rafs: window.__probe.rafs,
  };
}

function sampleIdle() {
  const stage = document.getElementById("stage");
  return {
    animations: document.getAnimations().length,
    layers: document.querySelectorAll("[data-forge-motion]").length,
    visibility: stage ? stage.style.visibility : "",
    rafs: window.__probe.rafs,
    errors: window.__probe.errors,
  };
}

async function assertIdle(page, label, failures) {
  await sleep(SETTLE_MS);
  const idle = await page.evaluate(sampleIdle);
  await sleep(IDLE_WATCH_MS);
  const later = await page.evaluate(sampleIdle);
  if (idle.animations !== 0) failures.push(`${label}: ${idle.animations} animation(s) still alive once idle`);
  if (idle.layers !== 0) failures.push(`${label}: ${idle.layers} motion layer(s) left behind`);
  if (idle.visibility !== "") failures.push(`${label}: stage visibility left as '${idle.visibility}'`);
  if (later.rafs !== idle.rafs) failures.push(`${label}: ${later.rafs - idle.rafs} frame callback(s) while idle`);
  if (idle.errors.length) failures.push(`${label}: page errors ${idle.errors.join("; ")}`);
}

async function exitCase(browser, exit, failures) {
  const label = `exit ${exit}`;
  const probe = await open(browser, { config: alertConfig({ exit }), content: ALERT_CONTENT });
  try {
    const shown = await shownAt(probe.page);
    const before = await probe.page.evaluate(() => window.__probe.rafs);
    await until(probe.page, shown + WINDOW_MS + EXIT_MS / 2);
    const mid = await probe.page.evaluate(sampleInMotion);
    const hidden = await hiddenAfter(probe.page, shown);
    const span = hidden - shown;
    const expected = exit === "none" ? WINDOW_MS : WINDOW_MS + EXIT_MS;

    if (span < expected - EARLY_SLACK_MS || span > expected + LATE_SLACK_MS) {
      failures.push(`${label}: hidden ${span.toFixed(0)}ms after the reveal, expected ${expected}ms`);
    }
    if (exit === "none") {
      if (mid.layers || mid.running) failures.push(`${label}: something still moved after the window`);
    } else if (exit === "burst") {
      if (mid.canvases !== 1) failures.push(`${label}: ${mid.canvases} particle canvas(es) mid-exit`);
      if (mid.rafs <= before) failures.push(`${label}: no particle frames were drawn`);
    } else if (DESTRUCTIVE_EXITS.includes(exit)) {
      if (mid.layers !== 1 || mid.clones < 1) failures.push(`${label}: played without fragments (${mid.layers} layers, ${mid.clones} clones)`);
      if (mid.clones > MAX_CLONES) failures.push(`${label}: ${mid.clones} clones past the cap of ${MAX_CLONES}`);
    } else if (mid.running < 1 || mid.layers) {
      failures.push(`${label}: no whole-rect exit animation ran (${mid.running} running, ${mid.layers} layers)`);
    }
    for (const breach of mid.outside) failures.push(`${label}: ${breach}`);
    await assertIdle(probe.page, label, failures);
    if (probe.pageErrors.length) failures.push(`${label}: ${probe.pageErrors.join("; ")}`);
  } finally {
    await probe.close();
  }
}

async function textCase(browser, effect, unit, headline, failures) {
  const label = `text ${effect} by ${unit} on ${JSON.stringify(headline.slice(0, 24))}`;
  const probe = await open(browser, {
    config: alertConfig({ duration: TEXT_WINDOW_MS / 1000, text_effect: effect, text_unit: unit, entrance_ms: 500 }),
    content: { headline, subline: "Three months" },
  });
  try {
    const shown = await shownAt(probe.page);
    const entering = await probe.page.evaluate(() => {
      const target = document.querySelector('[data-bind="headline"]');
      const units = Array.from(target.querySelectorAll("span")).filter((span) => span.style.display === "inline-block");
      const graphemes = Array.from(new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(target.textContent))
        .map((part) => part.segment)
        .filter((segment) => segment.trim());
      const words = target.textContent.split(/\s+/).filter(Boolean);
      return {
        latestEnd: Math.max(0, ...document.getAnimations().map((a) => a.effect.getComputedTiming().endTime)),
        animated: document.getAnimations().length,
        units: units.map((span) => span.textContent),
        graphemes,
        words,
      };
    });
    const budget = TEXT_WINDOW_MS * ENTRANCE_SHARE;
    if (entering.animated < entering.units.length) {
      failures.push(`${label}: only ${entering.animated} animation(s) for ${entering.units.length} text units`);
    }
    if (entering.latestEnd > budget + 1) {
      failures.push(`${label}: entrance runs ${entering.latestEnd.toFixed(0)}ms, past ${budget}ms of a ${TEXT_WINDOW_MS}ms window`);
    }
    const expectedUnits = unit === "letter" ? entering.graphemes : entering.words;
    if (JSON.stringify(entering.units) !== JSON.stringify(expectedUnits)) {
      failures.push(`${label}: split into ${JSON.stringify(entering.units)} instead of ${JSON.stringify(expectedUnits)}`);
    }
    await until(probe.page, shown + budget + SETTLE_MS);
    const settled = await probe.page.evaluate(() => {
      const target = document.querySelector('[data-bind="headline"]');
      return {
        children: target.children.length,
        text: target.textContent,
        animations: document.getAnimations().length,
        hidden: document.getElementById("stage").classList.contains("hidden"),
      };
    });
    if (settled.hidden) failures.push(`${label}: hidden before the window ended`);
    if (settled.children !== 0 || settled.text !== headline) {
      failures.push(`${label}: text not restored after the entrance (${settled.children} children, '${settled.text}')`);
    }
    if (settled.animations !== 0) failures.push(`${label}: ${settled.animations} animation(s) alive while holding`);
    if (probe.pageErrors.length) failures.push(`${label}: ${probe.pageErrors.join("; ")}`);
  } finally {
    await probe.close();
  }
}

async function cancelCase(browser, failures) {
  const label = "content arriving mid-exit";
  const probe = await open(browser, {
    config: alertConfig({ exit: "shatter", exit_ms: 1500 }),
    content: ALERT_CONTENT,
  });
  try {
    const shown = await shownAt(probe.page);
    await until(probe.page, shown + WINDOW_MS + 300);
    probe.host.push({ frame: "content", content: { headline: "Second", subline: "Again" } });
    const reshown = await probe.page
      .waitForFunction(
        () => document.querySelector('[data-bind="headline"]').textContent === "Second" && performance.now(),
        { timeout: WAIT_MS, polling: 5 },
      )
      .then((handle) => handle.jsonValue());
    const after = await probe.page.evaluate(() => {
      const stage = document.getElementById("stage");
      return {
        layers: document.querySelectorAll("[data-forge-motion]").length,
        hidden: stage.classList.contains("hidden"),
        visibility: stage.style.visibility,
        entering: stage.getAnimations().length,
      };
    });
    if (after.layers) failures.push(`${label}: the old exit kept playing beside the new show`);
    if (after.hidden || after.visibility === "hidden") failures.push(`${label}: the new show was not revealed`);
    if (after.entering !== 1) failures.push(`${label}: the new show did not enter (${after.entering} animations)`);
    const hidden = await hiddenAfter(probe.page, reshown);
    if (hidden - reshown < WINDOW_MS + 1500 - EARLY_SLACK_MS) {
      failures.push(`${label}: the cancelled exit hid the new show ${(hidden - reshown).toFixed(0)}ms after it entered`);
    }
  } finally {
    await probe.close();
  }
}

async function clearCase(browser, failures) {
  const label = "clear mid-exit";
  const probe = await open(browser, { config: alertConfig({ exit: "dust", exit_ms: 1500 }), content: ALERT_CONTENT });
  try {
    const shown = await shownAt(probe.page);
    await until(probe.page, shown + WINDOW_MS + 300);
    probe.host.push({ frame: "clear" });
    await probe.page.waitForFunction(() => document.body.style.visibility === "hidden", { timeout: WAIT_MS, polling: 5 });
    const after = await probe.page.evaluate(() => ({
      layers: document.querySelectorAll("[data-forge-motion]").length,
      animations: document.getAnimations().length,
    }));
    if (after.layers || after.animations) {
      failures.push(`${label}: the exit kept playing after a clear (${after.layers} layers, ${after.animations} animations)`);
    }
  } finally {
    await probe.close();
  }
}

async function previewLoopCase(browser, failures) {
  const label = "preview replay";
  const probe = await open(browser, { config: alertConfig({ exit: "fade", exit_ms: 300 }), content: ALERT_CONTENT, preview: true });
  try {
    const first = await shownAt(probe.page, 1);
    const second = await shownAt(probe.page, 2);
    const expected = WINDOW_MS + 300 + PREVIEW_PAUSE_MS;
    if (second - first < expected - EARLY_SLACK_MS || second - first > expected + LATE_SLACK_MS) {
      failures.push(`${label}: replayed ${(second - first).toFixed(0)}ms after the first show, expected ${expected}ms`);
    }
  } finally {
    await probe.close();
  }
}

async function liveNoReplayCase(browser, failures) {
  const label = "live page after its exit";
  const probe = await open(browser, { config: alertConfig({ exit: "fade", exit_ms: 300 }), content: ALERT_CONTENT });
  try {
    const first = await shownAt(probe.page, 1);
    await until(probe.page, first + WINDOW_MS + 300 + PREVIEW_PAUSE_MS + LATE_SLACK_MS * 2);
    const reveals = await probe.page.evaluate(
      () => window.__probe.marks.filter((mark, i, all) => !mark.hidden && (i === 0 || all[i - 1].hidden)).length,
    );
    if (reveals !== 1) failures.push(`${label}: showed ${reveals} times for one delivery`);
  } finally {
    await probe.close();
  }
}

async function fallbackCases(browser, failures) {
  const markup = require("fs")
    .readFileSync(require("path").resolve(__dirname, "../../crates/forge-overlay/assets/alert/index.html"), "utf8")
    .replace(/ data-bind="[a-z]+"/g, "");
  const unbound = await open(browser, {
    config: alertConfig({ entrance: "none", text_effect: "wave" }),
    content: ALERT_CONTENT,
    overrides: { "index.html": markup },
  });
  try {
    await shownAt(unbound.page);
    const animation = await unbound.page.evaluate(() =>
      document.getElementById("stage").getAnimations().map((a) => a.effect.getKeyframes().map((k) => Number(k.opacity))),
    );
    if (JSON.stringify(animation) !== JSON.stringify([[0, 1]])) {
      failures.push(`text effect without bound text: the stage did not fall back to a fade (${JSON.stringify(animation)})`);
    }
    if (unbound.pageErrors.length) failures.push(`text effect without bound text: ${unbound.pageErrors.join("; ")}`);
  } finally {
    await unbound.close();
  }

  const stageless = await open(browser, {
    config: alertConfig({ exit: "shatter" }),
    content: ALERT_CONTENT,
    overrides: { "index.html": markup.replace('id="stage"', 'id="card"') },
  });
  try {
    await sleep(WINDOW_MS + EXIT_MS + SETTLE_MS);
    const errors = await stageless.page.evaluate(() => window.__probe.errors);
    if (errors.length || stageless.pageErrors.length) {
      failures.push(`markup without the reveal target threw: ${errors.concat(stageless.pageErrors).join("; ")}`);
    }
  } finally {
    await stageless.close();
  }

  const engine = await open(browser, { config: alertConfig({}), content: ALERT_CONTENT });
  try {
    await shownAt(engine.page);
    const result = await engine.page.evaluate(
      () =>
        new Promise((resolve) => {
          const element = document.createElement("div");
          document.body.appendChild(element);
          const plan = window.forgeMotion.plan({ exit: "shatter", exit_ms: 200 }, 0);
          const started = performance.now();
          window.forgeMotion.exit(element, plan, 7, () =>
            resolve({ frames, layers: document.querySelectorAll("[data-forge-motion]").length, after: performance.now() - started }),
          );
          const frames = element.getAnimations().map((a) => a.effect.getKeyframes().map((k) => Number(k.opacity)));
        }),
    );
    if (JSON.stringify(result.frames) !== JSON.stringify([[1, 0]]) || result.layers) {
      failures.push(`a shatter of an unmeasurable element did not fall back to a fade (${JSON.stringify(result)})`);
    }
    if (result.after > 200 + LATE_SLACK_MS) failures.push(`the fallback exit finished ${result.after.toFixed(0)}ms in, past its 200ms`);
  } finally {
    await engine.close();
  }
}

async function determinismAndIntensityCase(browser, failures) {
  const probe = await open(browser, { config: alertConfig({ duration: 30 }), content: ALERT_CONTENT });
  try {
    await shownAt(probe.page);
    await sleep(600);
    const result = await probe.page.evaluate(() => {
      const stage = document.getElementById("stage");
      const grab = (seed, intensity) => {
        const plan = window.forgeMotion.plan({ exit: "shatter", exit_ms: 1000, intensity }, 0);
        window.forgeMotion.exit(stage, plan, seed, () => {});
        const pieces = Array.from(document.querySelectorAll("[data-forge-motion] > *"));
        const shape = pieces.map((piece) => JSON.stringify(piece.getAnimations()[0].effect.getKeyframes().map((k) => k.transform)));
        window.forgeMotion.cancel(stage);
        return shape;
      };
      return {
        first: grab(11, "medium"),
        again: grab(11, "medium"),
        other: grab(12, "medium"),
        low: grab(11, "low").length,
        medium: grab(11, "medium").length,
        high: grab(11, "high").length,
        left: document.querySelectorAll("[data-forge-motion]").length,
        visibility: stage.style.visibility,
      };
    });
    if (JSON.stringify(result.first) !== JSON.stringify(result.again)) failures.push("the same seed broke the content apart differently");
    if (JSON.stringify(result.first) === JSON.stringify(result.other)) failures.push("a different seed broke the content apart identically");
    if (!(result.low < result.medium && result.medium < result.high && result.high <= MAX_CLONES)) {
      failures.push(`intensity does not scale fragments within the cap: low ${result.low}, medium ${result.medium}, high ${result.high}`);
    }
    if (result.left || result.visibility !== "") failures.push("a cancelled exit left its layer or the hidden original behind");
  } finally {
    await probe.close();
  }
}

function entranceConfig(entrance) {
  return alertConfig({ duration: ENTRANCE_WINDOW_MS / 1000, entrance, entrance_ms: ENTRANCE_MS, exit: "none" });
}

function sampleRest() {
  const stage = document.getElementById("stage");
  const computed = getComputedStyle(stage);
  const scroller = document.scrollingElement;
  return {
    zIndex: stage.style.zIndex,
    position: stage.style.position,
    opacity: computed.opacity,
    transform: computed.transform,
    canvases: document.querySelectorAll("canvas").length,
    scrolls: scroller.scrollWidth > window.innerWidth || scroller.scrollHeight > window.innerHeight,
  };
}

async function assertAtRest(page, label, failures) {
  const rest = await page.evaluate(sampleRest);
  if (rest.zIndex !== "" || rest.position !== "") {
    failures.push(`${label}: the stage kept z-index '${rest.zIndex}' and position '${rest.position}' after the entrance`);
  }
  if (rest.opacity !== "1" || rest.transform !== "none") {
    failures.push(`${label}: the stage rests at opacity ${rest.opacity} and transform ${rest.transform}`);
  }
  if (rest.canvases) failures.push(`${label}: ${rest.canvases} particle canvas(es) left after the entrance`);
  if (rest.scrolls) failures.push(`${label}: the page scrolls after the entrance`);
}

async function restingShot(browser) {
  const probe = await open(browser, { config: entranceConfig("none"), content: ALERT_CONTENT });
  try {
    const shown = await shownAt(probe.page);
    await until(probe.page, shown + ENTRANCE_SETTLED_MS);
    return await probe.page.screenshot({ omitBackground: true });
  } finally {
    await probe.close();
  }
}

async function entranceCase(browser, entrance, resting, failures) {
  const label = `entrance ${entrance}`;
  const particles = PARTICLE_ENTRANCES.includes(entrance);
  const probe = await open(browser, { config: entranceConfig(entrance), content: ALERT_CONTENT });
  try {
    const shown = await shownAt(probe.page);
    const before = await probe.page.evaluate(() => window.__probe.rafs);
    await until(probe.page, shown + MID_ENTRANCE_MS);
    const mid = await probe.page.evaluate(sampleInMotion);
    const lift = await probe.page.evaluate(() => document.getElementById("stage").style.zIndex);
    if (entrance === "none") {
      if (mid.running || mid.layers) failures.push(`${label}: something moved (${mid.running} running, ${mid.layers} layers)`);
    } else if (particles) {
      if (mid.canvases !== 1) failures.push(`${label}: ${mid.canvases} particle canvas(es) mid-entrance`);
      if (mid.rafs <= before) failures.push(`${label}: no particle frames were drawn`);
      if (mid.running < 1) failures.push(`${label}: the content itself did not animate in`);
    } else if (mid.running < 1 || mid.layers) {
      failures.push(`${label}: no whole-rect entrance animation ran (${mid.running} running, ${mid.layers} layers)`);
    }
    if (LIFTED_ENTRANCES.includes(entrance) !== (lift !== "")) {
      failures.push(`${label}: the stage z-index mid-entrance was '${lift}'`);
    }
    for (const breach of mid.outside) failures.push(`${label}: ${breach}`);
    await until(probe.page, shown + ENTRANCE_SETTLED_MS);
    await assertIdle(probe.page, label, failures);
    await assertAtRest(probe.page, label, failures);
    const shot = await probe.page.screenshot({ omitBackground: true });
    if (!Buffer.from(shot).equals(Buffer.from(resting))) failures.push(`${label}: the settled page paints differently from a page that never moved`);
    if (probe.pageErrors.length) failures.push(`${label}: ${probe.pageErrors.join("; ")}`);
  } finally {
    await probe.close();
  }
}

async function entranceInterruptedCase(browser, entrance, failures) {
  const reshowLabel = `entrance ${entrance} under a newer show`;
  const reshow = await open(browser, { config: entranceConfig(entrance), content: ALERT_CONTENT });
  try {
    const shown = await shownAt(reshow.page);
    await until(reshow.page, shown + MID_ENTRANCE_MS);
    reshow.host.push({ frame: "content", content: { headline: "Second", subline: "Again" } });
    const reshown = await reshow.page
      .waitForFunction(
        () => document.querySelector('[data-bind="headline"]').textContent === "Second" && performance.now(),
        { timeout: WAIT_MS, polling: 5 },
      )
      .then((handle) => handle.jsonValue());
    const layers = await reshow.page.evaluate(() => document.querySelectorAll("[data-forge-motion]").length);
    if (layers > 1) failures.push(`${reshowLabel}: ${layers} motion layers, the old entrance kept playing`);
    await until(reshow.page, reshown + ENTRANCE_SETTLED_MS);
    await assertIdle(reshow.page, reshowLabel, failures);
    await assertAtRest(reshow.page, reshowLabel, failures);
    if (reshow.pageErrors.length) failures.push(`${reshowLabel}: ${reshow.pageErrors.join("; ")}`);
  } finally {
    await reshow.close();
  }

  const clearLabel = `entrance ${entrance} under a clear`;
  const cleared = await open(browser, { config: entranceConfig(entrance), content: ALERT_CONTENT });
  try {
    const shown = await shownAt(cleared.page);
    await until(cleared.page, shown + MID_ENTRANCE_MS);
    cleared.host.push({ frame: "clear" });
    await cleared.page.waitForFunction(() => document.body.style.visibility === "hidden", { timeout: WAIT_MS, polling: 5 });
    const after = await cleared.page.evaluate(() => ({
      layers: document.querySelectorAll("[data-forge-motion]").length,
      animations: document.getAnimations().length,
    }));
    if (after.layers || after.animations) {
      failures.push(`${clearLabel}: the entrance kept playing (${after.layers} layers, ${after.animations} animations)`);
    }
    await assertIdle(cleared.page, clearLabel, failures);
    await assertAtRest(cleared.page, clearLabel, failures);
    if (cleared.pageErrors.length) failures.push(`${clearLabel}: ${cleared.pageErrors.join("; ")}`);
  } finally {
    await cleared.close();
  }
}

async function entrancePlanCase(browser, failures) {
  const probe = await open(browser, { config: alertConfig({}), content: ALERT_CONTENT });
  try {
    await shownAt(probe.page);
    const planned = await probe.page.evaluate(
      (names) => names.map((name) => window.forgeMotion.plan({ entrance: name }, 0).entrance),
      [...RECT_ENTRANCES, ...PARTICLE_ENTRANCES, "sparkles", "none"],
    );
    const expected = [...RECT_ENTRANCES, ...PARTICLE_ENTRANCES, "fade", "none"];
    if (JSON.stringify(planned) !== JSON.stringify(expected)) {
      failures.push(`the plan keeps entrances ${JSON.stringify(planned)}, expected ${JSON.stringify(expected)}`);
    }
  } finally {
    await probe.close();
  }
}

async function particleEntranceTraceCase(browser, failures) {
  const probe = await open(browser, { config: alertConfig({ duration: 30 }), content: ALERT_CONTENT });
  try {
    await shownAt(probe.page);
    await sleep(600);
    const traces = await probe.page.evaluate(
      (entrances, box, entranceMs, inFlightMs) => {
        const context = CanvasRenderingContext2D.prototype;
        const drawn = [];
        const original = { fillRect: context.fillRect, lineTo: context.lineTo, stroke: context.stroke };
        context.fillRect = function (...args) {
          drawn.push(["square", ...args.map((value) => value.toFixed(2))]);
          return original.fillRect.apply(this, args);
        };
        context.lineTo = function (...args) {
          drawn.push(["streak", ...args.map((value) => value.toFixed(2))]);
          return original.lineTo.apply(this, args);
        };
        const realFrame = window.requestAnimationFrame;
        const frames = [];
        window.requestAnimationFrame = (callback) => {
          frames.push(callback);
          return 0;
        };
        const target = document.createElement("div");
        target.style.cssText = `position:absolute;left:0;top:0;width:${box.width}px;height:${box.height}px`;
        document.body.appendChild(target);
        const trace = (entrance, seed, intensity) => {
          frames.length = 0;
          const plan = window.forgeMotion.plan({ entrance, entrance_ms: entranceMs, intensity }, 0);
          window.forgeMotion.enter(target, plan, seed);
          const begin = frames.shift();
          if (!begin) return [];
          begin(1000);
          drawn.length = 0;
          const inFlight = frames.shift();
          if (inFlight) inFlight(1000 + inFlightMs);
          const shape = drawn.map((call) => call.join(" "));
          window.forgeMotion.cancel(target);
          return shape;
        };
        const result = {};
        try {
          for (const entrance of entrances) {
            result[entrance] = {
              first: trace(entrance, 11, "medium"),
              again: trace(entrance, 11, "medium"),
              other: trace(entrance, 12, "medium"),
              low: trace(entrance, 11, "low").length,
              high: trace(entrance, 11, "high").length,
            };
          }
        } finally {
          window.requestAnimationFrame = realFrame;
          Object.assign(context, original);
        }
        result.left = document.querySelectorAll("[data-forge-motion]").length;
        result.restored = target.style.zIndex === "" && target.style.position === "absolute";
        target.remove();
        return result;
      },
      PARTICLE_ENTRANCES,
      TRACE_BOX,
      ENTRANCE_MS,
      IN_FLIGHT_MS,
    );
    for (const entrance of PARTICLE_ENTRANCES) {
      const trace = traces[entrance];
      const medium = trace.first.length;
      if (!medium) failures.push(`entrance ${entrance}: no particle drawn in flight`);
      if (JSON.stringify(trace.first) !== JSON.stringify(trace.again)) failures.push(`entrance ${entrance}: the same seed scattered particles differently`);
      if (JSON.stringify(trace.first) === JSON.stringify(trace.other)) failures.push(`entrance ${entrance}: a different seed scattered particles identically`);
      if (!(trace.low < medium && medium < trace.high)) {
        failures.push(`entrance ${entrance}: intensity does not scale particles: low ${trace.low}, medium ${medium}, high ${trace.high}`);
      }
    }
    if (traces.left) failures.push(`a cancelled particle entrance left ${traces.left} layer(s) behind`);
    if (!traces.restored) failures.push("a cancelled particle entrance left its element lifted");
  } finally {
    await probe.close();
  }
}

async function persistentLookCases(browser, failures) {
  const chat = await open(browser, {
    kind: "chat",
    config: { ...BOX, design_width: 400, text_size: 13, position: "bottom", entrance: "slide-up", entrance_ms: 300, intensity: "medium" },
    content: { author: "viewer", message: "hello there", badges: "" },
  });
  try {
    await chat.page.waitForFunction(() => document.querySelector("#rows > *"), { timeout: WAIT_MS, polling: 5 });
    const entering = await chat.page.evaluate(() => document.querySelector("#rows > *").getAnimations().length);
    if (entering !== 1) failures.push(`chat row: ${entering} entrance animation(s) on a new row`);
    await sleep(300);
    await assertIdle(chat.page, "chat row", failures);
  } finally {
    await chat.close();
  }

  const goal = await open(browser, {
    kind: "goal",
    config: { ...BOX, design_width: 640, design_height: 160, text_size: 14, label: "Sub goal", value: "42", target: "100", entrance: "fade", entrance_ms: 500, text_effect: "bounce", text_unit: "letter", intensity: "medium" },
    content: { label: "Sub goal", value: "43", target: "100" },
  });
  try {
    await shownAt(goal.page);
    await sleep(1000);
    await assertIdle(goal.page, "goal", failures);
    const label = await goal.page.evaluate(() => {
      const target = document.querySelector('[data-bind="label"]');
      return { children: target.children.length, text: target.textContent };
    });
    if (label.children !== 0 || label.text !== "Sub goal") failures.push(`goal: label not restored (${JSON.stringify(label)})`);
  } finally {
    await goal.close();
  }
}

(async () => {
  const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox"] });
  const failures = [];
  const only = process.argv.slice(2);
  const run = async (name, body) => {
    if (only.length && !only.includes(name)) return;
    const before = failures.length;
    await body();
    console.log(`${failures.length === before ? "ok  " : "FAIL"} ${name}`);
  };
  try {
    for (const exit of ["none", ...RECT_EXITS, ...DESTRUCTIVE_EXITS]) {
      await run(`exit-${exit}`, () => exitCase(browser, exit, failures));
    }
    for (const effect of TEXT_EFFECTS) {
      for (const unit of UNITS) {
        await run(`text-${effect}-${unit}`, () => textCase(browser, effect, unit, UNICODE_HEADLINE, failures));
      }
    }
    await run("text-compressed", () => textCase(browser, "bounce", "letter", LONG_HEADLINE, failures));
    const resting = only.length && !only.some((name) => name.startsWith("entrance-")) ? null : await restingShot(browser);
    for (const entrance of ["none", ...RECT_ENTRANCES, ...PARTICLE_ENTRANCES]) {
      await run(`entrance-${entrance}`, () => entranceCase(browser, entrance, resting, failures));
      if (entrance !== "none") {
        await run(`entrance-${entrance}-interrupted`, () => entranceInterruptedCase(browser, entrance, failures));
      }
    }
    await run("entrance-plan", () => entrancePlanCase(browser, failures));
    await run("entrance-particle-trace", () => particleEntranceTraceCase(browser, failures));
    await run("cancel", () => cancelCase(browser, failures));
    await run("clear", () => clearCase(browser, failures));
    await run("preview-loop", () => previewLoopCase(browser, failures));
    await run("live-no-replay", () => liveNoReplayCase(browser, failures));
    await run("fallbacks", () => fallbackCases(browser, failures));
    await run("determinism-intensity", () => determinismAndIntensityCase(browser, failures));
    await run("persistent-looks", () => persistentLookCases(browser, failures));
  } finally {
    await browser.close();
  }
  for (const failure of failures) console.error(failure);
  process.exit(failures.length === 0 ? 0 : 1);
})().catch((error) => {
  console.error("motionprobe failed:", error.message);
  process.exit(1);
});
