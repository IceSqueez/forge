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
 * The connection has no subscription surface. The page says who it is, and from
 * then on it only receives. It opens ws://<this host>/ws/v1/ and, when config.json
 * carries a top-level credential, presents it as the first frame:
 *
 *   { "id": "1", "request": "auth", "overlayCredential": "<credential>" }
 *
 * forge derives the overlay identity from that credential, so nothing arriving
 * here is addressed by an id the page claimed. A config.json without a credential
 * sends no first frame. The connection reconnects on its own with a capped
 * backoff, so a browser source that was closed and reopened recovers unhelped.
 *
 * Two frame shapes arrive, both addressed to this overlay by the connection
 * itself:
 *
 *   { "frame": "content", "content": { "<key>": <value>, ... }, "durationMs": 5000 }
 *   { "frame": "reload" }
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
 * A reload frame is handled here rather than by the page. forge sends it after
 * rewriting an overlay's files, which is why hand-edited pages pick up regenerated
 * markup without the browser source being refreshed by hand.
 */

(function () {
  "use strict";

  var CONFIG_FILE = "./config.json";
  var SOCKET_PATH = "/ws/v1/";
  var AUTH_REQUEST_ID = "1";
  var CONTENT_FRAME = "content";
  var RELOAD_FRAME = "reload";
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
        fireReady();
        connect();
      })
      .catch(function (error) {
        attempt += 1;
        warn("could not load config.json (" + error.message + "), retrying");
        window.setTimeout(loadConfig, backoffMs());
      });
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

    document_.body.dataset.position = values.position || "";
    document_.body.dataset.animation = values.animation || "";
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
    socket.send(
      JSON.stringify({
        id: AUTH_REQUEST_ID,
        request: "auth",
        overlayCredential: credential,
      }),
    );
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
    if (frame.frame !== CONTENT_FRAME) {
      return;
    }

    var values =
      frame.content && typeof frame.content === "object" ? frame.content : {};
    var durationMs =
      typeof frame.durationMs === "number" && frame.durationMs > 0
        ? frame.durationMs
        : 0;

    contentCallbacks.forEach(function (callback) {
      invoke(callback, values, durationMs);
    });
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
    if (!(milliseconds > 0)) {
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
