"use strict";
// showjoin.js - drives the shared page runtime's show/speech join in headless
// Chromium against a mock forge host and checks what the page actually did:
// which audio elements played, in what order, and when the alert appeared and
// disappeared. Exits 1 when any case fails. Usage: node showjoin.js [case ...]
const http = require("http");
const fs = require("fs");
const path = require("path");
const { WebSocketServer } = require("ws");
const puppeteer = require("puppeteer");

const ASSETS = path.resolve(__dirname, "../../crates/forge-overlay/assets");
const KIND = "alert";
const SOUND_FILE = "ding.wav";
const SOUND_MS = 400;
const DURATION_S = 2;
const PAGE_WAIT_MS = 12000;

function wav(ms) {
  const rate = 8000;
  const samples = Math.round((rate * ms) / 1000);
  const buf = Buffer.alloc(44 + samples * 2);
  buf.write("RIFF", 0);
  buf.writeUInt32LE(36 + samples * 2, 4);
  buf.write("WAVEfmt ", 8);
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20);
  buf.writeUInt16LE(1, 22);
  buf.writeUInt32LE(rate, 24);
  buf.writeUInt32LE(rate * 2, 28);
  buf.writeUInt16LE(2, 32);
  buf.writeUInt16LE(16, 34);
  buf.write("data", 36);
  buf.writeUInt32LE(samples * 2, 40);
  for (let i = 0; i < samples; i++) {
    buf.writeInt16LE(Math.round(2000 * Math.sin((2 * Math.PI * 440 * i) / rate)), 44 + i * 2);
  }
  return buf;
}

function host() {
  const files = new Map();
  for (const name of ["index.html", "overlay.css", "overlay.js"]) {
    files.set(`/${KIND}/${name}`, fs.readFileSync(path.join(ASSETS, KIND, name)));
  }
  files.set("/forge-shared/runtime-v1.js", fs.readFileSync(path.join(ASSETS, "shared", "runtime-v1.js")));
  files.set(
    `/${KIND}/config.json`,
    Buffer.from(
      JSON.stringify({
        documentVersion: 1,
        credential: "probe-credential",
        kindId: `overlay.${KIND}`,
        config: { duration: DURATION_S, sound: SOUND_FILE, accent: "mauve" },
      }),
    ),
  );
  files.set(`/${KIND}/${SOUND_FILE}`, wav(SOUND_MS));
  const reports = {};
  let socket = null;
  let authed = null;
  const authedP = new Promise((r) => (authed = r));

  const server = http.createServer((req, res) => {
    const url = req.url.split("?")[0];
    if (req.method === "POST" && url.startsWith("/report/")) {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        reports[url.slice("/report/".length)] = JSON.parse(body);
        res.writeHead(204).end();
      });
      return;
    }
    const clip = /^\/clip\/ok-(\d+)$/.exec(url);
    if (clip) {
      res.writeHead(200, { "content-type": "audio/wav" });
      res.end(wav(Number(clip[1])));
      return;
    }
    const body = files.get(url);
    if (!body) {
      res.writeHead(404).end();
      return;
    }
    const ext = path.extname(url);
    const type = { ".html": "text/html", ".css": "text/css", ".js": "text/javascript", ".json": "application/json", ".wav": "audio/wav" }[ext];
    res.writeHead(200, { "content-type": type });
    res.end(body);
  });
  const wss = new WebSocketServer({ server, path: "/ws/v1/" });
  wss.on("connection", (s) => {
    s.on("message", (raw) => {
      const frame = JSON.parse(String(raw));
      if (frame.request === "auth") {
        s.send(JSON.stringify({ id: frame.id, status: "ok" }));
        socket = s;
        authed();
      }
    });
  });
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () =>
      resolve({
        url: `http://127.0.0.1:${server.address().port}/${KIND}/index.html`,
        authed: authedP,
        reports,
        push: (content) => socket.send(JSON.stringify({ frame: "content", content })),
        close: () => new Promise((done) => { wss.close(); server.close(done); }),
      }),
    ),
  );
}

function instrument() {
  window.__log = [];
  const log = (what) => window.__log.push({ t: Math.round(performance.now()), what });
  const kind = (el) => (String(el.src).startsWith("blob:") ? "clip" : "sound");
  const play = HTMLMediaElement.prototype.play;
  HTMLMediaElement.prototype.play = function () {
    log("play:" + kind(this));
    if (!this.__watched) {
      this.__watched = true;
      this.addEventListener("ended", () => log("ended:" + kind(this)));
    }
    return play.call(this);
  };
  document.addEventListener("DOMContentLoaded", () => {
    const stage = document.getElementById("stage");
    let last = "hidden";
    new MutationObserver(() => {
      const now = stage.classList.contains("hidden") ? "hidden" : "shown";
      if (now !== last) {
        last = now;
        log(now);
      }
    }).observe(stage, { attributes: true, attributeFilter: ["class"] });
    window.forge.content((values) => log("content:" + JSON.stringify(values)));
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function announce(id, clip, show) {
  const content = {
    clip_id: id,
    clip_path: `/clip/${clip}`,
    report_path: `/report/${id}`,
    clip_media_type: "audio/wav",
    clip_duration_ms: 1000,
  };
  if (show) content.show = show;
  return content;
}

class Run {
  constructor(page, h) {
    this.page = page;
    this.h = h;
  }
  async log() {
    return this.page.evaluate(() => window.__log);
  }
  async at(what) {
    const e = (await this.log()).find((x) => x.what === what);
    return e ? e.t : null;
  }
  async count(what) {
    return (await this.log()).filter((x) => x.what === what).length;
  }
  async contents() {
    return (await this.log()).filter((x) => x.what.startsWith("content:")).map((x) => JSON.parse(x.what.slice(8)));
  }
  async now() {
    return this.page.evaluate(() => Math.round(performance.now()));
  }
}

const CASES = {
  async base_sound_plays_on_every_delivery(r) {
    r.h.push({ headline: "first" });
    await sleep(1000);
    r.h.push({ headline: "second" });
    await sleep(1000);
    return [[await r.count("play:sound"), 2, "sound plays per delivery"]];
  },

  async a_spoken_show_stays_unseen_until_its_clip_arrives_then_sound_then_speech(r) {
    r.h.push({ headline: "donation", show: "T1" });
    await sleep(1500);
    const before = [await r.at("shown"), await r.count("play:sound"), (await r.contents()).length];
    r.h.push(announce("c1", "ok-1500", "T1"));
    await sleep(4000);
    const soundEnd = await r.at("ended:sound");
    const clipPlay = await r.at("play:clip");
    const contents = await r.contents();
    return [
      [before, [null, 0, 0], "nothing shown, sounded or handed to the look before the clip"],
      [(await r.at("shown")) !== null, true, "revealed once the clip arrived"],
      [soundEnd !== null && clipPlay !== null && clipPlay >= soundEnd, true, `speech starts after the sound ends (sound end ${soundEnd}, clip play ${clipPlay})`],
      [contents.length === 1 && !("show" in contents[0]), true, "the look got the content once, without the show token"],
      [r.h.reports.c1 && r.h.reports.c1.verdict, "played", "clip reported played"],
    ];
  },

  async a_hide_waits_for_speech_longer_than_the_duration(r) {
    r.h.push({ headline: "long", show: "T2" });
    r.h.push(announce("c2", "ok-4000", "T2"));
    await sleep(DURATION_S * 1000 + SOUND_MS + 4000 + 1500);
    const clipEnd = await r.at("ended:clip");
    const hidden = await r.at("hidden");
    return [[clipEnd !== null && hidden !== null && hidden >= clipEnd, true, `hidden ${hidden} not before speech end ${clipEnd}`]];
  },

  async a_short_speech_still_leaves_the_show_its_duration(r) {
    r.h.push({ headline: "short", show: "T3" });
    r.h.push(announce("c3", "ok-300", "T3"));
    await sleep(DURATION_S * 1000 + 2000);
    const shown = await r.at("shown");
    const hidden = await r.at("hidden");
    return [[shown !== null && hidden !== null && hidden - shown >= DURATION_S * 1000 - 100, true, `shown ${shown} hidden ${hidden}`]];
  },

  async a_refused_clip_reveals_the_show_silently(r) {
    r.h.push({ headline: "refused", show: "T4" });
    await sleep(300);
    r.h.push(announce("c4", "missing", "T4"));
    await sleep(1500);
    return [
      [(await r.at("shown")) !== null, true, "revealed after the clip was refused"],
      [await r.count("play:sound"), 1, "the base sound still plays"],
      [await r.count("play:clip"), 0, "no speech plays"],
      [r.h.reports.c4 && r.h.reports.c4.verdict, "refused", "clip reported refused"],
    ];
  },

  async a_reveal_frame_shows_only_its_own_show_and_never_reaches_the_look(r) {
    r.h.push({ headline: "withdrawn", show: "T5" });
    await sleep(300);
    r.h.push({ command: "reveal", show: "OTHER" });
    await sleep(700);
    const early = await r.at("shown");
    r.h.push({ command: "reveal", show: "T5" });
    await sleep(700);
    const contents = await r.contents();
    return [
      [early, null, "a reveal for another show left this one unseen"],
      [(await r.at("shown")) !== null, true, "revealed on its own reveal frame"],
      [await r.count("play:sound"), 1, "the silent reveal still plays the base sound"],
      [contents.length === 1 && contents[0].headline === "withdrawn" && !("command" in contents[0]), true, JSON.stringify(contents)],
    ];
  },

  async the_page_reveals_on_its_own_after_its_wait(r) {
    r.h.push({ headline: "lost", show: "T6" });
    await sleep(PAGE_WAIT_MS - 1000);
    const early = await r.at("shown");
    await sleep(2000);
    return [
      [early, null, "not revealed before the page's wait"],
      [(await r.at("shown")) !== null, true, "revealed once the page's wait ran out"],
    ];
  },

  async a_clip_matching_no_waiting_show_plays_at_once(r) {
    r.h.push(announce("c7", "ok-500", "T7"));
    r.h.push(announce("c8", "ok-500", ""));
    await sleep(1500);
    return [
      [await r.count("play:clip"), 2, "both orphan clips played"],
      [[await r.at("shown"), await r.count("play:sound")], [null, 0], "no show and no base sound"],
    ];
  },

  async a_show_arriving_after_its_clip_started_is_revealed_at_once(r) {
    r.h.push(announce("c9", "ok-3000", "T9"));
    await sleep(700);
    const sent = await r.now();
    r.h.push({ headline: "late", show: "T9" });
    await sleep(700);
    const shown = await r.at("shown");
    return [[shown !== null && shown - sent < 600, true, `sent ${sent} shown ${shown}`]];
  },
};

(async () => {
  const wanted = process.argv.slice(2);
  const names = Object.keys(CASES).filter((n) => !wanted.length || wanted.includes(n));
  let failed = 0;
  await Promise.all(
    names.map(async (name) => {
      const h = await host();
      const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox", "--autoplay-policy=no-user-gesture-required"] });
      try {
        const page = await browser.newPage();
        await page.evaluateOnNewDocument(instrument);
        await page.goto(h.url, { waitUntil: "load" });
        await h.authed;
        await sleep(200);
        const checks = await CASES[name](new Run(page, h));
        const bad = checks.filter(([got, want]) => JSON.stringify(got) !== JSON.stringify(want));
        if (bad.length) {
          failed += 1;
          console.log(`FAIL ${name}`);
          for (const [got, want, what] of bad) console.log(`  ${what}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
          console.log("  log " + JSON.stringify(await page.evaluate(() => window.__log)));
        } else {
          console.log(`ok   ${name}`);
        }
      } finally {
        await browser.close();
        await h.close();
      }
    }),
  );
  console.log(failed ? `${failed} failed` : `${names.length} passed`);
  process.exit(failed ? 1 : 0);
})().catch((e) => {
  console.error("failed:", e.message);
  process.exit(1);
});
