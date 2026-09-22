/*
 * forge overlay client runtime, version 1.
 *
 * A generated page loads this file, then its own overlay.js. Everything the page
 * needs arrives through window.forge:
 *
 *   forge.ready(callback)        callback(config) once config.json is loaded
 *   forge.content(callback)      callback(values, durationMs) per delivery
 *   forge.set(name, text)        writes text into every [data-bind="name"] node
 *   forge.show(selector)         reveals matching elements
 *   forge.show(selector, ms)     reveals them, hides them again after ms
 *   forge.sound(name)            plays a file from this overlay's folder
 *
 * config.json sits next to the page. Its config object is what the ready callback
 * receives, and the accent, font, position and animation entries are applied here
 * rather than by the page: accent and font become the --accent and --font custom
 * properties, position and animation become data-position and data-animation on
 * <body>. Stylesheets read those.
 *
 * The element_width, element_height and text_size entries are applied the same
 * way, as pixels of the 1920x1080 browser source. Width and height become the
 * --element-width and --element-height custom properties with their px unit
 * attached; a stylesheet spends them through var() with its own fallback, so an
 * entry config.json leaves out is an element that sizes itself. text_size becomes
 * the unitless --text-size, and each stylesheet divides it by the text size that
 * kind draws at to reach its own --text-scale, which every size it states is
 * multiplied by. A config.json without text_size leaves every page exactly as it
 * was drawn before any of these three entries existed.
 *
 * The connection has no subscription surface. The page says who it is, and from
 * then on it only receives. It opens ws://<this host>/ws/v1/ and, when config.json
 * carries a top-level credential, presents it as the first frame:
 *
 *   { "id": "1", "request": "auth", "overlayCredential": "<credential>",
 *     "previewConnection": false }
 *
 * forge derives the overlay identity from that credential, so nothing arriving
 * here is addressed by an id the page claimed. previewConnection is true only on
 * a page opened with the preview flag, and forge counts such a connection apart
 * from a browser source while still delivering to it. A config.json without a
 * credential sends no first frame. The connection reconnects on its own with a
 * capped backoff, so a browser source that was closed and reopened recovers
 * unhelped.
 *
 * Three frame shapes arrive, all addressed to this overlay by the connection
 * itself:
 *
 *   { "frame": "content", "content": { "<key>": <value>, ... }, "durationMs": 5000 }
 *   { "frame": "reload" }
 *   { "frame": "clear" }
 *
 * A content frame carries the content group of this overlay's type with every
 * value already final: forge expanded it where the variable context lives, so the
 * page renders text and never expands it. Keys are the content field names the
 * type declares. durationMs appears only when the delivery overrode the overlay's
 * own duration. The wire says nothing about how content composes with what is on
 * screen - whether it replaces, shows for a while, or appends is the page's own
 * business, and a content group is applied whole, so a key the frame leaves out is
 * left out of the display too.
 *
 * A page reached with ?preview=1 in its query previews itself. It reads
 * sample.json, the generated sample document sitting beside config.json, and
 * delivers that content to itself as soon as the page is ready; it paints a
 * checkerboard behind the page so transparent areas are visible; and it never
 * hides anything on a timer, so a transient overlay stays up to be looked at.
 * Nothing else changes: a preview page connects, identifies and receives exactly
 * like the page a browser source loads; it only says so in its auth frame, so
 * forge counts it apart from this overlay's browser sources. Without the flag the
 * page is transparent and shows nothing until content arrives.
 *
 * A reload frame is handled here rather than by the page. forge sends it after
 * rewriting an overlay's files, which is why hand-edited pages pick up regenerated
 * markup without the browser source being refreshed by hand.
 *
 * A clear frame arrives just before forge closes the connection because this
 * overlay was disabled. The page is hidden in place, not reloaded, so nothing
 * stale keeps showing while the socket sits closed. The connection's own
 * reconnect loop keeps retrying; once re-enabled, a validated reconnect gets its
 * last content replayed automatically, and that content frame is what un-hides
 * the page again.
 */

(function () {
  "use strict";

  var CONFIG_FILE = "./config.json";
  var SAMPLE_FILE = "./sample.json";
  var PREVIEW_PARAM = "preview";
  var PREVIEW_VALUE = "1";
  var PREVIEW_CONNECTION_FIELD = "previewConnection";
  var CHECKER_TILE_PX = 24;
  var CHECKER_BASE = "#15151c";
  var CHECKER_SQUARE = "#23232e";
  var SOCKET_PATH = "/ws/v1/";
  var AUTH_REQUEST_ID = "1";
  var CONTENT_FRAME = "content";
  var RELOAD_FRAME = "reload";
  var CLEAR_FRAME = "clear";
  var HIDDEN_CLASS = "hidden";
  var RECONNECT_BASE_MS = 500;
  var RECONNECT_CAP_MS = 15000;
  var RELOAD_DELAY_MS = 250;
  var LOG_PREFIX = "forge overlay:";

  var ACCENT_HEX = {
    mauve: "#cba6f7",
    sky: "#89dceb",
    green: "#a6e3a1",
    peach: "#fab387",
    yellow: "#f9e2af",
    red: "#f38ba8",
  };
  var FALLBACK_ACCENT = ACCENT_HEX.mauve;
  var FONT_NAME = /^[A-Za-z0-9 _-]+$/;

  var ELEMENT_WIDTH_PROPERTY = "--element-width";
  var ELEMENT_HEIGHT_PROPERTY = "--element-height";
  var TEXT_SIZE_PROPERTY = "--text-size";
  var PIXEL_UNIT = "px";
  var NO_UNIT = "";

  var document_ = window.document;
  var readyCallbacks = [];
  var contentCallbacks = [];
  var hideTimers = new Map();

  var config = null;
  var credential = "";
  var readyFired = false;
  var reloading = false;

  var socket = null;
  var attempt = 0;
  var previewing =
    new URLSearchParams(window.location.search).get(PREVIEW_PARAM) ===
    PREVIEW_VALUE;

  function warn(message) {
    if (window.console) {
      window.console.warn(LOG_PREFIX + " " + message);
    }
  }

  function loadConfig() {
    window
      .fetch(CONFIG_FILE, { cache: "no-store" })
      .then(function (response) {
        if (!response.ok) {
          throw new Error("config.json responded " + response.status);
        }
        return response.json();
      })
      .then(function (document_json) {
        config = document_json.config || {};
        credential = document_json.credential || "";
        applyAppearance(config);
        start();
      })
      .catch(function (error) {
        attempt += 1;
        warn("could not load config.json (" + error.message + "), retrying");
        window.setTimeout(loadConfig, backoffMs());
      });
  }

  function start() {
    if (!previewing) {
      fireReady();
      connect();
      return;
    }

    paintCheckerboard();
    loadSample(function (values) {
      fireReady();
      connect();
      deliver(values, 0);
    });
  }

  function loadSample(then) {
    window
      .fetch(SAMPLE_FILE, { cache: "no-store" })
      .then(function (response) {
        if (!response.ok) {
          throw new Error("sample.json responded " + response.status);
        }
        return response.json();
      })
      .then(function (document_json) {
        then(document_json.content || {});
      })
      .catch(function (error) {
        warn("could not load sample.json (" + error.message + ")");
        then({});
      });
  }

  function paintCheckerboard() {
    var tile = CHECKER_TILE_PX + "px";
    var offset = CHECKER_TILE_PX / 2 + "px";
    var square =
      "linear-gradient(45deg, " +
      CHECKER_SQUARE +
      " 25%, transparent 25%, transparent 75%, " +
      CHECKER_SQUARE +
      " 75%)";

    var backdrop = document_.documentElement.style;
    backdrop.setProperty("background-color", CHECKER_BASE);
    backdrop.setProperty("background-image", square + ", " + square);
    backdrop.setProperty("background-position", "0 0, " + offset + " " + offset);
    backdrop.setProperty("background-size", tile + " " + tile);
  }

  function applyAppearance(values) {
    var accent = ACCENT_HEX[values.accent] || FALLBACK_ACCENT;
    document_.documentElement.style.setProperty("--accent", accent);

    var font = values.font;
    if (typeof font === "string" && FONT_NAME.test(font)) {
      document_.documentElement.style.setProperty(
        "--font",
        '"' + font + '", sans-serif',
      );
    }

    applySize(ELEMENT_WIDTH_PROPERTY, values.element_width, PIXEL_UNIT);
    applySize(ELEMENT_HEIGHT_PROPERTY, values.element_height, PIXEL_UNIT);
    applySize(TEXT_SIZE_PROPERTY, values.text_size, NO_UNIT);

    document_.body.dataset.position = values.position || "";
    document_.body.dataset.animation = values.animation || "";
  }

  function applySize(property, value, unit) {
    var root = document_.documentElement.style;
    if (typeof value === "number" && isFinite(value) && value > 0) {
      root.setProperty(property, value + unit);
    } else {
      root.removeProperty(property);
    }
  }

  function fireReady() {
    if (readyFired) {
      return;
    }
    readyFired = true;
    var pending = readyCallbacks;
    readyCallbacks = [];
    pending.forEach(function (callback) {
      invoke(callback, config);
    });
  }

  function invoke(callback, first, second) {
    try {
      callback(first, second);
    } catch (error) {
      warn("overlay code threw: " + error);
    }
  }

  function backoffMs() {
    var ceiling = Math.min(
      RECONNECT_CAP_MS,
      RECONNECT_BASE_MS * Math.pow(2, attempt),
    );
    return Math.round(ceiling * (0.5 + Math.random() * 0.5));
  }

  function socketUrl() {
    var scheme = window.location.protocol === "https:" ? "wss:" : "ws:";
    return scheme + "//" + window.location.host + SOCKET_PATH;
  }

  function connect() {
    if (!window.location.host) {
      warn("page was not served by forge, so nothing can be delivered to it");
      return;
    }

    socket = new WebSocket(socketUrl());

    socket.onopen = function () {
      attempt = 0;
      identify();
    };

    socket.onmessage = function (message) {
      receive(message.data);
    };

    socket.onclose = function () {
      socket = null;
      if (!reloading) {
        window.setTimeout(connect, backoffMs());
        attempt += 1;
      }
    };

    socket.onerror = function () {
      if (socket) {
        socket.close();
      }
    };
  }

  function identify() {
    if (!credential || !socket || socket.readyState !== WebSocket.OPEN) {
      return;
    }
    var frame = {
      id: AUTH_REQUEST_ID,
      request: "auth",
      overlayCredential: credential,
    };
    frame[PREVIEW_CONNECTION_FIELD] = previewing;
    socket.send(JSON.stringify(frame));
  }

  function receive(raw) {
    var frame;
    try {
      frame = JSON.parse(raw);
    } catch (error) {
      warn("ignored an unreadable frame");
      return;
    }

    if (frame.status === "error") {
      warn("forge refused this connection: " + describeError(frame.error));
      return;
    }
    if (frame.frame === RELOAD_FRAME) {
      reload();
      return;
    }
    if (frame.frame === CLEAR_FRAME) {
      clear();
      return;
    }
    if (frame.frame !== CONTENT_FRAME) {
      return;
    }

    var values =
      frame.content && typeof frame.content === "object" ? frame.content : {};
    var durationMs =
      typeof frame.durationMs === "number" && frame.durationMs > 0
        ? frame.durationMs
        : 0;

    deliver(values, durationMs);
  }

  function deliver(values, durationMs) {
    unclear();
    contentCallbacks.forEach(function (callback) {
      invoke(callback, values, durationMs);
    });
  }

  function clear() {
    document_.body.style.visibility = "hidden";
  }

  function unclear() {
    document_.body.style.visibility = "";
  }

  function describeError(error) {
    if (!error) {
      return "no detail given";
    }
    return (error.code || "error") + " " + (error.message || "");
  }

  function reload() {
    if (reloading) {
      return;
    }
    reloading = true;
    window.setTimeout(function () {
      window.location.reload();
    }, RELOAD_DELAY_MS);
  }

  function content(callback) {
    if (typeof callback !== "function") {
      return;
    }
    contentCallbacks.push(callback);
  }

  function ready(callback) {
    if (typeof callback !== "function") {
      return;
    }
    if (readyFired) {
      invoke(callback, config);
    } else {
      readyCallbacks.push(callback);
    }
  }

  function set(name, text) {
    if (!name) {
      return;
    }
    var value = text === null || text === undefined ? "" : String(text);
    document_.querySelectorAll("[data-bind]").forEach(function (node) {
      if (node.getAttribute("data-bind") === name) {
        node.textContent = value;
      }
    });
  }

  function show(selector, milliseconds) {
    if (!selector) {
      return;
    }
    var nodes = document_.querySelectorAll(selector);
    nodes.forEach(function (node) {
      node.classList.remove(HIDDEN_CLASS);
    });

    var pending = hideTimers.get(selector);
    if (pending) {
      window.clearTimeout(pending);
      hideTimers.delete(selector);
    }
    if (previewing || !(milliseconds > 0)) {
      return;
    }

    hideTimers.set(
      selector,
      window.setTimeout(function () {
        hideTimers.delete(selector);
        nodes.forEach(function (node) {
          node.classList.add(HIDDEN_CLASS);
        });
      }, milliseconds),
    );
  }

  /* The page plays the sound, so it reaches the stream through the browser
     source's own audio rather than the local output device. */
  function sound(name) {
    if (!name) {
      return;
    }
    var audio = new Audio(new URL(name, document_.baseURI).href);
    var started = audio.play();
    if (started && started.catch) {
      started.catch(function (error) {
        warn("could not play " + name + " (" + error.message + ")");
      });
    }
  }

  window.forge = Object.freeze({
    ready: ready,
    content: content,
    set: set,
    show: show,
    sound: sound,
  });

  loadConfig();
})();
