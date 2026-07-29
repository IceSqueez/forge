/*
 * forge overlay client runtime, version 1.
 *
 * A generated page loads this file, then its own overlay.js. Everything the page
 * needs arrives through window.forge:
 *
 *   forge.ready(callback)          callback(config) once config.json is loaded
 *   forge.on(kind, callback)       callback(payload) for every event of that kind
 *   forge.set(name, text)          writes text into every [data-bind="name"] node
 *   forge.tpl(template, payload)   expands %token% against the payload, one pass
 *   forge.show(selector)           reveals matching elements
 *   forge.show(selector, ms)       reveals them, hides them again after ms
 *   forge.sound(name)              plays a file from this overlay's folder
 *
 * config.json sits next to the page and holds the overlay identity plus the
 * config object handed to the ready callback. The accent, font, position and
 * animation entries are applied here, not by the page: accent and font become
 * the --accent and --font custom properties, position and animation become
 * data-position and data-animation on <body>. Stylesheets read those.
 *
 * Events arrive over the local WebSocket at ws://<this host>/ws/v1/ as frames
 * shaped { timeStamp, event: { source, type }, data }, and forge.on matches
 * event.type. The connection reconnects on its own with a capped backoff, so a
 * browser source that was closed and reopened recovers without help.
 *
 * One frame is handled by the runtime rather than the page: type
 * "overlay.reload", with data { overlayId }. The page reloads itself when the
 * id matches its own or is absent. forge sends it after rewriting an overlay's
 * files, which is why hand-edited pages pick up regenerated markup without the
 * browser source being refreshed by hand.
 */

(function () {
  "use strict";

  var CONFIG_FILE = "./config.json";
  var SOCKET_PATH = "/ws/v1/";
  var RELOAD_EVENT = "overlay.reload";
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
  var handlers = new Map();
  var readyCallbacks = [];
  var hideTimers = new Map();

  var config = null;
  var overlayId = "";
  var readyFired = false;
  var reloading = false;

  var socket = null;
  var subscribed = new Set();
  var attempt = 0;
  var requestId = 0;

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
        overlayId = document_json.overlayId || "";
        applyAppearance(config);
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

  function invoke(callback, argument) {
    try {
      callback(argument);
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
      warn("page was not served by forge, so no events will arrive");
      fireReady();
      return;
    }

    socket = new WebSocket(socketUrl());

    socket.onopen = function () {
      attempt = 0;
      subscribed = new Set();
      subscribe(Array.from(handlers.keys()).concat(RELOAD_EVENT));
      fireReady();
    };

    socket.onmessage = function (message) {
      receive(message.data);
    };

    socket.onclose = function () {
      socket = null;
      fireReady();
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

  function subscribe(kinds) {
    var fresh = kinds.filter(function (kind) {
      return kind && !subscribed.has(kind);
    });
    if (fresh.length === 0 || !socket || socket.readyState !== WebSocket.OPEN) {
      return;
    }
    fresh.forEach(function (kind) {
      subscribed.add(kind);
    });
    requestId += 1;
    socket.send(
      JSON.stringify({
        id: String(requestId),
        request: "subscribe",
        events: fresh.map(function (kind) {
          return { type: kind };
        }),
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
      warn("server refused a request: " + describeError(frame.error));
      return;
    }
    if (!frame.event || typeof frame.event.type !== "string") {
      return;
    }
    if (frame.event.type === RELOAD_EVENT) {
      reloadIfAddressed(frame.data);
      return;
    }

    var listeners = handlers.get(frame.event.type);
    if (listeners) {
      listeners.forEach(function (callback) {
        invoke(callback, frame.data || {});
      });
    }
  }

  function describeError(error) {
    if (!error) {
      return "no detail given";
    }
    return (error.code || "error") + " " + (error.message || "");
  }

  function reloadIfAddressed(data) {
    var target = data && data.overlayId;
    if (target && target !== overlayId) {
      return;
    }
    if (reloading) {
      return;
    }
    reloading = true;
    window.setTimeout(function () {
      window.location.reload();
    }, RELOAD_DELAY_MS);
  }

  function on(kind, callback) {
    if (!kind || typeof callback !== "function") {
      return;
    }
    var listeners = handlers.get(kind);
    if (listeners) {
      listeners.push(callback);
    } else {
      handlers.set(kind, [callback]);
    }
    subscribe([kind]);
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

  /* Single pass, no recursion: a value that itself contains %tokens% is left alone,
     and a token with no matching field survives verbatim. */
  function tpl(template, payload) {
    if (typeof template !== "string") {
      return "";
    }
    var fields = payload && typeof payload === "object" ? payload : {};
    var out = "";
    var index = 0;

    while (index < template.length) {
      var character = template[index];
      index += 1;
      if (character !== "%") {
        out += character;
        continue;
      }

      var name = "";
      var closed = false;
      while (index < template.length) {
        var inner = template[index];
        index += 1;
        if (inner === "%") {
          closed = true;
          break;
        }
        name += inner;
      }

      if (!closed) {
        out += "%";
        continue;
      }

      var value = fieldText(fields, name.trim());
      out += value === undefined ? "%" + name + "%" : value;
    }

    return out;
  }

  function fieldText(fields, name) {
    if (!Object.prototype.hasOwnProperty.call(fields, name)) {
      return undefined;
    }
    var value = fields[name];
    if (value === null || value === undefined) {
      return undefined;
    }
    if (Array.isArray(value)) {
      return "[" + value.length + " items]";
    }
    if (typeof value === "object") {
      return "{" + Object.keys(value).length + " keys}";
    }
    return String(value);
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
    on: on,
    set: set,
    tpl: tpl,
    show: show,
    sound: sound,
  });

  loadConfig();
})();
