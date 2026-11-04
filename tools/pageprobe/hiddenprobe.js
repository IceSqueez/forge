"use strict";
// hiddenprobe.js - checks that overlay content delivered while nobody can see
// the page (a background tab, or OBS reporting the browser source not visible)
// lands on its final state, that a hide whose time ran out unseen happens as
// soon as the page is seen, and that a page whose animation timeline never
// advances still finishes its entrance. Exits 1 when any case fails.
// Usage: node hiddenprobe.js [case ...]
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
const CARD_CONTENT = { headline: "Welcome to the stream", subline: "Say hi in chat" };
const NEXT_CONTENT = { headline: "Back in five", subline: "Grab a drink" };
const ALERT_CONTENT = { headline: "Thanks for the sub!", subline: "Three months" };
const SOURCE_VISIBLE_EVENT = "obsSourceVisibleChanged";

const ENTRANCES = [
  { entrance: "fade", text_effect: "none" },
  { entrance: "sparks", text_effect: "typewriter" },
];
const LONG_ENTRANCE_MS = 3000;
const FROZEN_ENTRANCE_MS = 600;
const SHORT_DURATION_S = 1;
const STARVED_DURATION_S = 3;
const MID_DURATION_S = 4;
const SEEN_AT_MS = 1200;
const EXIT_MS = 400;
const EARLY_SLACK_MS = 40;
const HIDDEN_TIMER_SLACK_MS = 1500;
const WATCH_MS = 700;
const WAIT_MS = 8000;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function instrument(starvedDelayMs) {
  window.__probe = { marks: [], starved: 0 };
  if (starvedDelayMs > 0) {
    const schedule = window.setTimeout.bind(window);
    window.setTimeout = (callback, delay, ...rest) => {
      if (delay === starvedDelayMs) {
        window.__probe.starved += 1;
        return 0;
      }
      return schedule(callback, delay, ...rest);
    };
  }
  document.addEventListener("DOMContentLoaded", () => {
    const stage = document.getElementById("stage");
    const note = () =>
      window.__probe.marks.push({ t: performance.now(), hidden: stage.classList.contains("hidden") });
    note();
    new MutationObserver(note).observe(stage, { attributes: true, attributeFilter: ["class"] });
  });
}

function sample() {
  const stage = document.getElementById("stage");
  return {
    visibility: document.visibilityState,
    hidden: stage.classList.contains("hidden"),
    opacity: Number(getComputedStyle(stage).opacity),
    animations: document.getAnimations().length,
    layers: document.querySelectorAll("[data-forge-motion]").length,
    text: stage.textContent.replace(/\s+/g, " ").trim(),
  };
}

function atRest(state) {
  return !state.hidden && state.opacity === 1 && state.animations === 0 && state.layers === 0;
}

async function open(browser, { kind, config, content, background = false, frozen = false, starvedDelayMs = 0 }) {
  const host = await start({ kind, config: { ...BOX, ...config }, content });
  const page = await browser.newPage();
  await page.setViewport({ width: BOX.design_width, height: BOX.design_height });
  await page.evaluateOnNewDocument(instrument, starvedDelayMs);
  let front = null;
  if (background) {
    front = await browser.newPage();
    await front.bringToFront();
  } else {
    await page.bringToFront();
  }
  if (frozen) {
    const cdp = await page.target().createCDPSession();
    await cdp.send("Animation.enable");
    await cdp.send("Animation.setPlaybackRate", { playbackRate: 0 });
  }
  await page.goto(host.url, { waitUntil: "load" });
  return {
    page,
    host,
    close: async () => {
      await page.close();
      if (front) await front.close();
      await host.close();
    },
  };
}

function delivered(page, headline) {
  return page.waitForFunction(
    (text) => {
      const stage = document.getElementById("stage");
      return stage && !stage.classList.contains("hidden") && stage.textContent.includes(text) && performance.now();
    },
    { timeout: WAIT_MS, polling: "mutation" },
    headline,
  ).then((handle) => handle.jsonValue());
}

function revealedAt(page) {
  return page.evaluate(() => {
    const reveal = window.__probe.marks.find((mark, i, all) => !mark.hidden && i > 0 && all[i - 1].hidden);
    return reveal ? reveal.t : null;
  });
}

function hiddenAfter(page, since) {
  return page
    .waitForFunction(
      (t) => {
        const mark = window.__probe.marks.find((m) => m.hidden && m.t > t);
        return mark ? mark.t : false;
      },
      { timeout: WAIT_MS, polling: 50 },
      since,
    )
    .then((handle) => handle.jsonValue());
}

function shownAfter(page, since) {
  return page.evaluate((t) => window.__probe.marks.some((m) => !m.hidden && m.t > t), since);
}

function now(page) {
  return page.evaluate(() => performance.now());
}

function describe(entrance) {
  return `${entrance.entrance}+${entrance.text_effect}`;
}

async function hiddenCardCase(browser, entrance, failures) {
  const label = `hidden card ${describe(entrance)}`;
  const probe = await open(browser, { kind: "card", config: entrance, content: CARD_CONTENT, background: true });
  try {
    await delivered(probe.page, CARD_CONTENT.headline);
    const unseen = await probe.page.evaluate(sample);
    if (unseen.visibility !== "hidden") {
      failures.push(`${label}: precondition - the page was not hidden (${unseen.visibility})`);
      return;
    }
    if (!atRest(unseen)) {
      failures.push(`${label}: while hidden the card is not at rest ${JSON.stringify(unseen)}`);
    }
    await probe.page.bringToFront();
    const seen = await probe.page.evaluate(sample);
    if (seen.visibility !== "visible" || !atRest(seen)) {
      failures.push(`${label}: right after becoming visible the card is not at rest ${JSON.stringify(seen)}`);
    }
  } finally {
    await probe.close();
  }
}

async function alertElapsedWhileHiddenCase(browser, failures) {
  const label = "alert elapsed while hidden";
  const config = { duration: SHORT_DURATION_S, entrance: "fade", exit: "fade", exit_ms: EXIT_MS };
  const probe = await open(browser, { kind: "alert", config, content: ALERT_CONTENT, background: true });
  try {
    await delivered(probe.page, ALERT_CONTENT.headline);
    const shown = await revealedAt(probe.page);
    await hiddenAfter(probe.page, shown);
    await probe.page.bringToFront();
    const seenAt = await now(probe.page);
    await sleep(WATCH_MS);
    const state = await probe.page.evaluate(sample);
    if (!state.hidden || (await shownAfter(probe.page, seenAt))) {
      failures.push(`${label}: the alert appeared after the page became visible ${JSON.stringify(state)}`);
    }
  } finally {
    await probe.close();
  }
}

async function alertOverdueHideCase(browser, skipsExit, failures) {
  const label = skipsExit ? "alert overdue hide without exit" : "alert overdue hide";
  const durationMs = STARVED_DURATION_S * 1000;
  const config = { duration: STARVED_DURATION_S, entrance: "fade", exit: "fade", exit_ms: EXIT_MS };
  const probe = await open(browser, {
    kind: "alert",
    config,
    content: ALERT_CONTENT,
    background: true,
    starvedDelayMs: durationMs,
  });
  try {
    await delivered(probe.page, ALERT_CONTENT.headline);
    const shown = await revealedAt(probe.page);
    await probe.page.waitForFunction((t) => performance.now() > t, { timeout: WAIT_MS, polling: 50 }, shown + durationMs);
    const overdue = await probe.page.evaluate(() => ({
      starved: window.__probe.starved,
      hidden: document.getElementById("stage").classList.contains("hidden"),
    }));
    if (overdue.starved === 0 || overdue.hidden) {
      failures.push(`${label}: precondition - the hide timer was not starved ${JSON.stringify(overdue)}`);
      return;
    }
    await probe.page.bringToFront();
    const seenAt = await now(probe.page);
    const hidden = await hiddenAfter(probe.page, seenAt).catch(() => null);
    if (hidden === null) {
      failures.push(`${label}: an alert past its duration stayed up after the page became visible`);
    }
    if (skipsExit && hidden !== null && hidden > seenAt + EARLY_SLACK_MS) {
      failures.push(`${label}: an alert past its duration played its exit for ${Math.round(hidden - seenAt)}ms once seen`);
    }
  } finally {
    await probe.close();
  }
}

async function alertMidWindowCase(browser, failures) {
  const label = "alert seen mid-window";
  const durationMs = MID_DURATION_S * 1000;
  const config = { duration: MID_DURATION_S, entrance: "fade", entrance_ms: 500, exit: "fade", exit_ms: EXIT_MS };
  const probe = await open(browser, { kind: "alert", config, content: ALERT_CONTENT, background: true });
  try {
    await delivered(probe.page, ALERT_CONTENT.headline);
    const shown = await revealedAt(probe.page);
    await probe.page.waitForFunction((t) => performance.now() > t, { timeout: WAIT_MS, polling: 50 }, shown + SEEN_AT_MS);
    await probe.page.bringToFront();
    const seen = await probe.page.evaluate(sample);
    if (!atRest(seen)) {
      failures.push(`${label}: the alert is not shown statically when seen ${JSON.stringify(seen)}`);
    }
    const hidden = await hiddenAfter(probe.page, shown);
    if (hidden < shown + durationMs - EARLY_SLACK_MS) {
      failures.push(`${label}: hid ${Math.round(shown + durationMs - hidden)}ms before its duration ended`);
    }
    if (hidden > shown + durationMs + EXIT_MS + HIDDEN_TIMER_SLACK_MS) {
      failures.push(`${label}: hid ${Math.round(hidden - shown - durationMs)}ms after its duration ended`);
    }
  } finally {
    await probe.close();
  }
}

async function frozenTimelineCase(browser, entrance, failures) {
  const label = `frozen timeline ${describe(entrance)}`;
  const config = { ...entrance, entrance_ms: FROZEN_ENTRANCE_MS };
  const probe = await open(browser, { kind: "card", config, content: CARD_CONTENT, frozen: true });
  try {
    const shown = await delivered(probe.page, CARD_CONTENT.headline);
    const early = await probe.page.evaluate(sample);
    if (early.animations === 0 || early.opacity === 1) {
      failures.push(`${label}: precondition - the entrance was not frozen ${JSON.stringify(early)}`);
      return;
    }
    const settled = await probe.page
      .waitForFunction(
        () => {
          const stage = document.getElementById("stage");
          return (
            Number(getComputedStyle(stage).opacity) === 1 &&
            document.getAnimations().length === 0 &&
            document.querySelectorAll("[data-forge-motion]").length === 0 &&
            performance.now()
          );
        },
        { timeout: WAIT_MS, polling: 20 },
      )
      .then((handle) => handle.jsonValue())
      .catch(() => null);
    if (settled === null) {
      failures.push(`${label}: the entrance never finished on a frozen timeline ${JSON.stringify(await probe.page.evaluate(sample))}`);
    } else if (settled < shown + FROZEN_ENTRANCE_MS - EARLY_SLACK_MS) {
      failures.push(`${label}: the entrance was cut ${Math.round(shown + FROZEN_ENTRANCE_MS - settled)}ms before its planned end`);
    }
  } finally {
    await probe.close();
  }
}

function sourceVisible(page, visible) {
  return page.evaluate(
    (name, value) => window.dispatchEvent(new CustomEvent(name, { detail: { visible: value } })),
    SOURCE_VISIBLE_EVENT,
    visible,
  );
}

async function sourceHiddenCase(browser, failures) {
  const label = "obs source not visible";
  const config = { entrance: "fade", entrance_ms: LONG_ENTRANCE_MS };
  const probe = await open(browser, { kind: "card", config, content: CARD_CONTENT });
  try {
    await delivered(probe.page, CARD_CONTENT.headline);
    await sourceVisible(probe.page, false);
    const caughtUp = await probe.page.evaluate(sample);
    if (!atRest(caughtUp)) {
      failures.push(`${label}: the running entrance did not jump to its end ${JSON.stringify(caughtUp)}`);
    }
    probe.host.push({ frame: "content", content: NEXT_CONTENT });
    await delivered(probe.page, NEXT_CONTENT.headline);
    const next = await probe.page.evaluate(sample);
    if (!atRest(next)) {
      failures.push(`${label}: the next delivery did not land on its final state ${JSON.stringify(next)}`);
    }
  } finally {
    await probe.close();
  }
}

async function sourceVisibleAgainCase(browser, failures) {
  const label = "obs source visible again";
  const config = { entrance: "fade", entrance_ms: LONG_ENTRANCE_MS };
  const probe = await open(browser, { kind: "card", config, content: CARD_CONTENT });
  try {
    await delivered(probe.page, CARD_CONTENT.headline);
    await sourceVisible(probe.page, false);
    await sourceVisible(probe.page, true);
    probe.host.push({ frame: "content", content: NEXT_CONTENT });
    await delivered(probe.page, NEXT_CONTENT.headline);
    const next = await probe.page.evaluate(sample);
    if (next.animations === 0) {
      failures.push(`${label}: a delivery after the source is visible again did not animate ${JSON.stringify(next)}`);
    }
  } finally {
    await probe.close();
  }
}

async function visibleAnimatesCase(browser, failures) {
  const label = "visible page animates";
  const config = { entrance: "fade", entrance_ms: LONG_ENTRANCE_MS };
  const probe = await open(browser, { kind: "card", config, content: CARD_CONTENT });
  try {
    await delivered(probe.page, CARD_CONTENT.headline);
    const midway = await probe.page
      .waitForFunction(
        () => {
          const opacity = Number(getComputedStyle(document.getElementById("stage")).opacity);
          return opacity > 0 && opacity < 1;
        },
        { timeout: LONG_ENTRANCE_MS, polling: 10 },
      )
      .then(() => true)
      .catch(() => false);
    if (!midway) {
      failures.push(`${label}: a visible page never showed the entrance mid-way ${JSON.stringify(await probe.page.evaluate(sample))}`);
    }
  } finally {
    await probe.close();
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
    for (const entrance of ENTRANCES) {
      await run(`hidden-card-${describe(entrance)}`, () => hiddenCardCase(browser, entrance, failures));
    }
    await run("alert-elapsed-while-hidden", () => alertElapsedWhileHiddenCase(browser, failures));
    await run("alert-overdue-hide", () => alertOverdueHideCase(browser, false, failures));
    await run("alert-overdue-hide-skips-exit", () => alertOverdueHideCase(browser, true, failures));
    await run("alert-seen-mid-window", () => alertMidWindowCase(browser, failures));
    for (const entrance of ENTRANCES) {
      await run(`frozen-timeline-${describe(entrance)}`, () => frozenTimelineCase(browser, entrance, failures));
    }
    await run("source-hidden", () => sourceHiddenCase(browser, failures));
    await run("source-visible-again", () => sourceVisibleAgainCase(browser, failures));
    await run("visible-animates", () => visibleAnimatesCase(browser, failures));
  } finally {
    await browser.close();
  }
  for (const failure of failures) console.error(failure);
  process.exit(failures.length === 0 ? 0 : 1);
})().catch((error) => {
  console.error("hiddenprobe failed:", error.message);
  process.exit(1);
});
