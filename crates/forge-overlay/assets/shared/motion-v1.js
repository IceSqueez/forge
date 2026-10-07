(function () {
  "use strict";

  var NONE = "none";
  var FALLBACK_STYLE = "fade";
  var POP = "pop";
  var WIPE = "wipe";
  var LAYER_ATTRIBUTE = "data-forge-motion";
  var BIND_ATTRIBUTE = "data-bind";
  var BIND_SELECTOR = "[" + BIND_ATTRIBUTE + "]";
  var LAYER_SELECTOR = "[" + LAYER_ATTRIBUTE + "]";
  var WORD_UNIT = "word";
  var LETTER_UNIT = "letter";
  var WHITESPACE_RUN = /(\s+)/;
  var ONLY_WHITESPACE = /^\s+$/;
  var PIXEL_UNIT = "px";

  var ENTRANCE_SHARE = 0.75;
  var TEXT_DELAY_SHARE = 0.4;
  var DEFAULT_ENTRANCE_MS = 500;
  var DEFAULT_EXIT_MS = 500;
  var MOTION_MS_MIN = 100;
  var MOTION_MS_MAX = 3000;
  var STAGGER_MS_MIN = 10;
  var STAGGER_MS_MAX = 300;
  var MIN_UNIT_MS = 40;
  var LETTER_LIMIT = 160;
  var WORD_LIMIT = 60;
  var FALLBACK_TEXT_PX = 16;
  var MIN_RECT_PX = 1;
  var MIN_DIRECTION = 0.01;
  var HALF = 0.5;
  var PERCENT = 100;
  var MS_PER_SECOND = 1000;

  var SETTLE_EASING = "cubic-bezier(0.22, 1, 0.36, 1)";
  var OVERSHOOT_EASING = "cubic-bezier(0.34, 1.56, 0.64, 1)";
  var ACCELERATE_EASING = "cubic-bezier(0.55, 0, 1, 0.45)";
  var LINEAR_EASING = "linear";

  var SLIDE_DISTANCE_PX = 24;
  var POP_SHRINK = 0.15;
  var WIPE_HIDDEN_RIGHT = "inset(0 100% 0 0)";
  var WIPE_HIDDEN_LEFT = "inset(0 0 0 100%)";
  var WIPE_SHOWN = "inset(0 0 0 0)";
  var SLIDES = {
    "slide-up": { x: 0, y: 1 },
    "slide-down": { x: 0, y: -1 },
    "slide-left": { x: 1, y: 0 },
    "slide-right": { x: -1, y: 0 },
  };
  var OPPOSITE = {
    "slide-up": "slide-down",
    "slide-down": "slide-up",
    "slide-left": "slide-right",
    "slide-right": "slide-left",
  };
  var RECT_STYLES = [FALLBACK_STYLE, POP, WIPE].concat(Object.keys(SLIDES));

  var INTENSITIES = {
    low: { scale: 0.6, fragments: 12, particles: 36, bands: 4 },
    medium: { scale: 1, fragments: 24, particles: 72, bands: 6 },
    high: { scale: 1.5, fragments: 40, particles: 120, bands: 8 },
  };
  var DEFAULT_INTENSITY = "medium";

  var FRAGMENT_AREA_PX = 600;
  var PARTICLE_AREA_PX = 300;
  var MIN_FRAGMENTS = 4;
  var MAX_CLONES = 48;
  var FRAGMENT_DELAY_SHARE = 0.35;
  var SHARDS_PER_CELL = 2;
  var SHATTER_DISTANCE_PX = 90;
  var SHATTER_FALL_PX = 120;
  var SHATTER_SPIN_DEG = 70;
  var SHATTER_BREAK_OFFSET = 0.3;
  var SHATTER_BREAK_SHARE = 0.15;
  var DISSOLVE_SHRINK = 0.6;
  var DISSOLVE_DELAY_SHARE = 0.55;
  var DUST_FRAGMENT_FACTOR = 2;
  var DUST_SWEEP_SHARE = 0.45;
  var DUST_JITTER_SHARE = 0.15;
  var DUST_DRIFT_PX = 60;
  var DUST_RISE_PX = 40;
  var SMOKE_RISE_PX = 50;
  var SMOKE_DRIFT_PX = 24;
  var SMOKE_SWELL = 0.12;
  var SMOKE_BLUR_PX = 8;
  var BURST_SWELL = 0.08;
  var BURST_SWELL_SHARE = 0.35;
  var BURST_SPEED_PX = 260;
  var BURST_LIFT_PX = 80;
  var GRAVITY_PX = 520;
  var PARTICLE_MIN_PX = 2;
  var PARTICLE_SPREAD_PX = 3;
  var PARTICLE_LIFE_SHARE = 0.5;
  var PIXEL_RATIO_CAP = 2;
  var SLOW_FRAME_MS = 45;
  var FRAME_SAMPLE = 8;
  var BUDGET_FLOOR = 0.25;
  var BUDGET_STEP = 0.5;
  var ACCENT_PROPERTY = "--accent";
  var FALLBACK_PARTICLE_COLOR = "#cba6f7";
  var HOT_PARTICLE_COLOR = "#fff8e7";
  var LIFTED_Z = "1";
  var AUTO_Z = "auto";
  var STATIC_POSITION = "static";
  var TURN_RADIANS = Math.PI * 2;
  var EASE_OUT_POWER = 3;
  var STREAK_SECONDS = 0.035;
  var PARTICLE_MIN_LIFE_MS = 600;

  var SPARK_SPEED_PX = 300;
  var SPARK_JITTER = 0.6;
  var SPARK_LIFT_PX = 140;
  var SPARK_DELAY_SHARE = 0.4;
  var SPARK_HOT_SHARE = 0.35;
  var SPARK_WIDTH_PX = 1.5;

  var ASSEMBLE_PARTICLE_FACTOR = 2;
  var ASSEMBLE_SCATTER_PX = 140;
  var ASSEMBLE_REVEAL_OFFSET = 0.55;
  var ASSEMBLE_TRAVEL_SHARE = 0.7;
  var ASSEMBLE_SWELL = 0.04;

  var GLOW_SHRINK = 0.25;
  var GLOW_SPREAD_PX = 60;
  var GLOW_BLUR_PX = 60;
  var GLOW_CORE_SHARE = 0.8;
  var GLOW_PASSES = 3;
  var GLOW_STREAK_WIDTH_PX = 2.5;
  var GLOW_LIFE_SHARE = 1.2;
  var GLOW_RING_SHARE = 0.6;
  var GLOW_RING_SPEED_PX = 420;
  var GLOW_RING_DRAG = 2.5;

  var FLY_SPREAD = 2.5;
  var FLY_RISE = 1.5;
  var FLY_TILT_DEG = 30;
  var SPIN_TURN_DEG = 270;
  var SPIN_START_SCALE = 0.2;
  var WAVE_HEIGHT = 0.8;
  var WAVE_CREST = 0.3;
  var WAVE_CREST_AT = 0.6;
  var BOUNCE_DROP = 1.2;
  var BOUNCE_STEPS = [
    { offset: 0, lift: 1, opacity: 0, easing: ACCELERATE_EASING },
    { offset: 0.45, lift: 0, opacity: 1, easing: SETTLE_EASING },
    { offset: 0.65, lift: 0.25, opacity: 1, easing: ACCELERATE_EASING },
    { offset: 0.82, lift: 0, opacity: 1, easing: SETTLE_EASING },
    { offset: 0.92, lift: 0.08, opacity: 1, easing: ACCELERATE_EASING },
    { offset: 1, lift: 0, opacity: 1, easing: LINEAR_EASING },
  ];

  var TEXT_EFFECTS = {
    typewriter: {
      staggerMs: 50,
      unitMs: MIN_UNIT_MS,
      easing: LINEAR_EASING,
      frames: typewriterFrames,
    },
    "fly-in": {
      staggerMs: 40,
      unitMs: 450,
      easing: SETTLE_EASING,
      frames: flyFrames,
    },
    spin: {
      staggerMs: 60,
      unitMs: 500,
      easing: SETTLE_EASING,
      frames: spinFrames,
    },
    wave: {
      staggerMs: 50,
      unitMs: 600,
      easing: SETTLE_EASING,
      frames: waveFrames,
    },
    bounce: {
      staggerMs: 80,
      unitMs: 700,
      easing: LINEAR_EASING,
      frames: bounceFrames,
    },
  };

  var PARTICLE_ENTRANCES = {
    sparks: sparks,
    assemble: assemble,
    "glow-burst": glowBurst,
  };

  var DESTRUCTIVE_EXITS = {
    dissolve: dissolve,
    smoke: smoke,
    shatter: shatter,
    dust: dust,
    burst: burst,
  };

  var FNV_OFFSET = 2166136261;
  var FNV_PRIME = 16777619;
  var LCG_MULTIPLIER = 1664525;
  var LCG_INCREMENT = 1013904223;
  var UINT32_RANGE = 4294967296;

  var document_ = window.document;
  var active = new Map();
  var budget = 1;

  function owned(table, key) {
    return (
      typeof key === "string" && Object.prototype.hasOwnProperty.call(table, key)
    );
  }

  function bounded(value, min, max, fallback) {
    if (typeof value !== "number" || !isFinite(value)) {
      return fallback;
    }
    return Math.min(max, Math.max(min, Math.round(value)));
  }

  function style(value, known) {
    if (typeof value !== "string" || !value || value === NONE) {
      return NONE;
    }
    return known(value) ? value : FALLBACK_STYLE;
  }

  function isRectStyle(value) {
    return RECT_STYLES.indexOf(value) >= 0;
  }

  function isEntranceStyle(value) {
    return isRectStyle(value) || owned(PARTICLE_ENTRANCES, value);
  }

  function isExitStyle(value) {
    return isRectStyle(value) || owned(DESTRUCTIVE_EXITS, value);
  }

  function plan(config, windowMs) {
    var values = config || {};
    var textEffect = owned(TEXT_EFFECTS, values.text_effect)
      ? values.text_effect
      : NONE;
    var exitStyle = style(values.exit, isExitStyle);
    var paceMs =
      textEffect === NONE ? 0 : TEXT_EFFECTS[textEffect].staggerMs;
    return {
      entrance: style(values.entrance, isEntranceStyle),
      entranceMs: bounded(
        values.entrance_ms,
        MOTION_MS_MIN,
        MOTION_MS_MAX,
        DEFAULT_ENTRANCE_MS,
      ),
      textEffect: textEffect,
      textUnit: values.text_unit === WORD_UNIT ? WORD_UNIT : LETTER_UNIT,
      staggerMs:
        values.text_stagger_custom === true
          ? bounded(
              values.text_stagger_ms,
              STAGGER_MS_MIN,
              STAGGER_MS_MAX,
              paceMs,
            )
          : paceMs,
      exit: exitStyle,
      exitMs:
        exitStyle === NONE
          ? 0
          : bounded(values.exit_ms, MOTION_MS_MIN, MOTION_MS_MAX, DEFAULT_EXIT_MS),
      budgetMs: windowMs > 0 ? windowMs * ENTRANCE_SHARE : Infinity,
      intensity: owned(INTENSITIES, values.intensity)
        ? INTENSITIES[values.intensity]
        : INTENSITIES[DEFAULT_INTENSITY],
    };
  }

  function seed(values) {
    var source = values || {};
    var text = Object.keys(source)
      .sort()
      .map(function (key) {
        return key + "=" + JSON.stringify(source[key]);
      })
      .join("\n");
    var hash = FNV_OFFSET;
    for (var index = 0; index < text.length; index += 1) {
      hash ^= text.charCodeAt(index);
      hash = Math.imul(hash, FNV_PRIME) >>> 0;
    }
    return hash >>> 0;
  }

  function random(seedValue) {
    var state = seedValue >>> 0;
    return function () {
      state = (Math.imul(state, LCG_MULTIPLIER) + LCG_INCREMENT) >>> 0;
      return state / UINT32_RANGE;
    };
  }

  function signed(rng) {
    return (rng() - HALF) / HALF;
  }

  function translate(x, y) {
    return "translate(" + x + PIXEL_UNIT + ", " + y + PIXEL_UNIT + ")";
  }

  function rotate(degrees) {
    return "rotate(" + degrees + "deg)";
  }

  function scaled(factor) {
    return "scale(" + factor + ")";
  }

  function handleFor(element) {
    var handle = {
      element: element,
      animations: [],
      pending: [],
      restores: [],
      layer: null,
      frame: 0,
      timer: 0,
      done: null,
      settled: false,
    };
    active.set(element, handle);
    return handle;
  }

  function settle(handle, completed) {
    if (handle.settled) {
      return;
    }
    handle.settled = true;
    if (active.get(handle.element) === handle) {
      active.delete(handle.element);
    }
    window.clearTimeout(handle.timer);
    if (handle.frame) {
      window.cancelAnimationFrame(handle.frame);
    }
    if (completed && handle.done) {
      handle.done();
    }
    handle.animations.forEach(function (animation) {
      animation.cancel();
    });
    if (handle.layer && handle.layer.parentNode) {
      handle.layer.parentNode.removeChild(handle.layer);
    }
    handle.restores.forEach(function (restore) {
      restore();
    });
  }

  function cancel(element) {
    var handle = active.get(element);
    if (handle) {
      settle(handle, false);
    }
  }

  function cancelAll() {
    Array.from(active.values()).forEach(function (handle) {
      settle(handle, false);
    });
  }

  function owns(node) {
    return Boolean(node && node.closest && node.closest(LAYER_SELECTOR));
  }

  function entranceFrames(name, scale) {
    var slide = SLIDES[name];
    if (slide) {
      var distance = SLIDE_DISTANCE_PX * scale;
      return [
        { opacity: 0, transform: translate(slide.x * distance, slide.y * distance) },
        { opacity: 1, transform: "none" },
      ];
    }
    if (name === POP) {
      return [
        { opacity: 0, transform: scaled(1 - POP_SHRINK * scale) },
        { opacity: 1, transform: "none" },
      ];
    }
    if (name === WIPE) {
      return [{ clipPath: WIPE_HIDDEN_RIGHT }, { clipPath: WIPE_SHOWN }];
    }
    return [{ opacity: 0 }, { opacity: 1 }];
  }

  function exitFrames(name, scale) {
    if (name === WIPE) {
      return [{ clipPath: WIPE_SHOWN }, { clipPath: WIPE_HIDDEN_LEFT }];
    }
    return entranceFrames(OPPOSITE[name] || name, scale).reverse();
  }

  function enter(element, motionPlan, seedValue) {
    cancel(element);
    if (!element || !element.animate || !motionPlan) {
      return;
    }
    var handle = handleFor(element);
    var entranceMs = Math.min(motionPlan.entranceMs, motionPlan.budgetMs);
    var entrance = motionPlan.entrance;
    var splits =
      motionPlan.textEffect === NONE
        ? []
        : splitTargets(element, motionPlan.textUnit);
    if (motionPlan.textEffect !== NONE && !splits.length && entrance === NONE) {
      entrance = FALLBACK_STYLE;
    }
    if (owned(PARTICLE_ENTRANCES, entrance)) {
      PARTICLE_ENTRANCES[entrance](handle, motionPlan, entranceMs, random(seedValue));
    } else if (entrance !== NONE) {
      arrive(
        handle,
        entranceFrames(entrance, motionPlan.intensity.scale),
        entranceMs,
        entrance === POP ? OVERSHOOT_EASING : SETTLE_EASING,
      );
    }
    if (splits.length) {
      animateText(
        handle,
        splits,
        motionPlan,
        entrance === NONE ? 0 : entranceMs * TEXT_DELAY_SHARE,
        random(seedValue),
      );
    }
    settleWhenFinished(handle);
  }

  function arrive(handle, frames, durationMs, easing) {
    handle.animations.push(
      handle.element.animate(frames, {
        duration: durationMs,
        easing: easing,
        fill: "backwards",
      }),
    );
  }

  function settleWhenFinished(handle) {
    if (!handle.animations.length && !handle.pending.length) {
      settle(handle, true);
      return;
    }
    Promise.all(
      handle.animations
        .map(function (animation) {
          return animation.finished;
        })
        .concat(handle.pending),
    ).then(
      function () {
        settle(handle, true);
      },
      function () {},
    );
  }

  function textTargets(element) {
    var found = Array.from(element.querySelectorAll(BIND_SELECTOR));
    if (element.hasAttribute(BIND_ATTRIBUTE)) {
      found.unshift(element);
    }
    return found.filter(function (target) {
      return target.children.length === 0 && target.textContent.trim();
    });
  }

  function splitTargets(element, unit) {
    var splits = [];
    textTargets(element).forEach(function (target) {
      var split = splitText(target, unit);
      if (split) {
        splits.push(split);
      }
    });
    return splits;
  }

  function graphemes(text) {
    if (window.Intl && window.Intl.Segmenter) {
      var segmenter = new window.Intl.Segmenter(undefined, {
        granularity: "grapheme",
      });
      return Array.from(segmenter.segment(text), function (part) {
        return part.segment;
      });
    }
    return Array.from(text);
  }

  function splitText(target, unit) {
    var original = target.textContent;
    var pieces = original.split(WHITESPACE_RUN).filter(Boolean);
    var words = pieces.filter(function (piece) {
      return !ONLY_WHITESPACE.test(piece);
    });
    var byLetter =
      unit === LETTER_UNIT && graphemes(original).length <= LETTER_LIMIT;
    if (!byLetter && words.length > WORD_LIMIT) {
      return null;
    }
    var size =
      parseFloat(window.getComputedStyle(target).fontSize) || FALLBACK_TEXT_PX;
    var holder = document_.createElement("span");
    var units = [];
    pieces.forEach(function (piece) {
      if (ONLY_WHITESPACE.test(piece)) {
        holder.appendChild(document_.createTextNode(piece));
        return;
      }
      if (!byLetter) {
        holder.appendChild(unitSpan(piece, size, units));
        return;
      }
      var word = document_.createElement("span");
      word.style.whiteSpace = "nowrap";
      graphemes(piece).forEach(function (letter) {
        word.appendChild(unitSpan(letter, size, units));
      });
      holder.appendChild(word);
    });
    target.textContent = "";
    target.appendChild(holder);
    return {
      units: units,
      restore: function () {
        if (holder.parentNode === target) {
          target.textContent = original;
        }
      },
    };
  }

  function unitSpan(text, size, units) {
    var span = document_.createElement("span");
    span.textContent = text;
    span.style.display = "inline-block";
    units.push({ node: span, size: size });
    return span;
  }

  function animateText(handle, splits, motionPlan, delayMs, rng) {
    var effect = TEXT_EFFECTS[motionPlan.textEffect];
    var units = [];
    splits.forEach(function (split) {
      handle.restores.push(split.restore);
      units = units.concat(split.units);
    });
    var room = motionPlan.budgetMs - delayMs;
    var unitMs = Math.max(MIN_UNIT_MS, Math.min(effect.unitMs, room));
    var stagger = motionPlan.staggerMs;
    if (units.length > 1) {
      stagger = Math.max(
        0,
        Math.min(stagger, (room - unitMs) / (units.length - 1)),
      );
    }
    var scale = motionPlan.intensity.scale;
    units.forEach(function (unit, index) {
      handle.animations.push(
        unit.node.animate(effect.frames(rng, scale, unit.size), {
          duration: unitMs,
          delay: delayMs + index * stagger,
          easing: effect.easing,
          fill: "backwards",
        }),
      );
    });
  }

  function typewriterFrames() {
    return [{ opacity: 0 }, { opacity: 1 }];
  }

  function flyFrames(rng, scale, size) {
    var x = signed(rng) * FLY_SPREAD * size * scale;
    var y = -(HALF + rng()) * FLY_RISE * size * scale;
    var tilt = signed(rng) * FLY_TILT_DEG * scale;
    return [
      { opacity: 0, transform: translate(x, y) + " " + rotate(tilt) },
      { opacity: 1, transform: "none" },
    ];
  }

  function spinFrames(rng, scale) {
    var turn = (rng() < HALF ? -SPIN_TURN_DEG : SPIN_TURN_DEG) * scale;
    return [
      {
        opacity: 0,
        transform: rotate(turn) + " " + scaled(SPIN_START_SCALE),
      },
      { opacity: 1, transform: "none" },
    ];
  }

  function waveFrames(rng, scale, size) {
    var height = WAVE_HEIGHT * size * scale;
    return [
      { opacity: 0, transform: translate(0, height) },
      {
        opacity: 1,
        transform: translate(0, -height * WAVE_CREST),
        offset: WAVE_CREST_AT,
      },
      { opacity: 1, transform: "none" },
    ];
  }

  function bounceFrames(rng, scale, size) {
    var drop = BOUNCE_DROP * size * scale;
    return BOUNCE_STEPS.map(function (step) {
      return {
        offset: step.offset,
        opacity: step.opacity,
        easing: step.easing,
        transform: step.lift ? translate(0, -step.lift * drop) : "none",
      };
    });
  }

  function exit(element, motionPlan, seedValue, done) {
    var finish = typeof done === "function" ? done : function () {};
    cancel(element);
    if (
      !element ||
      !element.animate ||
      !motionPlan ||
      motionPlan.exit === NONE ||
      !(motionPlan.exitMs > 0)
    ) {
      finish();
      return;
    }
    var handle = handleFor(element);
    handle.done = finish;
    var destructive = DESTRUCTIVE_EXITS[motionPlan.exit];
    var played =
      owned(DESTRUCTIVE_EXITS, motionPlan.exit) &&
      destructive(handle, motionPlan, random(seedValue));
    if (!played) {
      var name = isRectStyle(motionPlan.exit) ? motionPlan.exit : FALLBACK_STYLE;
      handle.animations.push(
        element.animate(exitFrames(name, motionPlan.intensity.scale), {
          duration: motionPlan.exitMs,
          easing: ACCELERATE_EASING,
          fill: "forwards",
        }),
      );
    }
    handle.timer = window.setTimeout(function () {
      settle(handle, true);
    }, motionPlan.exitMs);
  }

  function measurable(rect) {
    return rect.width >= MIN_RECT_PX && rect.height >= MIN_RECT_PX;
  }

  function openLayer(handle) {
    var body = document_.body;
    if (!body) {
      return null;
    }
    var box = body.getBoundingClientRect();
    var layer = document_.createElement("div");
    layer.setAttribute(LAYER_ATTRIBUTE, "");
    var css = layer.style;
    css.position = "absolute";
    css.left = box.left + window.scrollX + PIXEL_UNIT;
    css.top = box.top + window.scrollY + PIXEL_UNIT;
    css.width = box.width + PIXEL_UNIT;
    css.height = box.height + PIXEL_UNIT;
    css.margin = "0";
    css.padding = "0";
    css.overflow = "hidden";
    css.pointerEvents = "none";
    body.appendChild(layer);
    handle.layer = layer;
    return { element: layer, box: box };
  }

  function concealOriginal(handle) {
    var element = handle.element;
    var previous = element.style.visibility;
    element.style.visibility = "hidden";
    handle.restores.push(function () {
      element.style.visibility = previous;
    });
  }

  function withFragments(handle, build) {
    var rect = handle.element.getBoundingClientRect();
    if (!measurable(rect)) {
      return false;
    }
    var layer = openLayer(handle);
    if (!layer) {
      return false;
    }
    build(layer, rect);
    concealOriginal(handle);
    return true;
  }

  function fragment(handle, parent, layer, rect, clip) {
    var clone = handle.element.cloneNode(true);
    clone.removeAttribute(BIND_ATTRIBUTE);
    clone.querySelectorAll(BIND_SELECTOR).forEach(function (node) {
      node.removeAttribute(BIND_ATTRIBUTE);
    });
    var css = clone.style;
    css.position = "absolute";
    css.left = rect.left - layer.box.left + PIXEL_UNIT;
    css.top = rect.top - layer.box.top + PIXEL_UNIT;
    css.width = rect.width + PIXEL_UNIT;
    css.height = rect.height + PIXEL_UNIT;
    css.margin = "0";
    css.boxSizing = "border-box";
    css.maxWidth = "none";
    css.maxHeight = "none";
    css.transform = "none";
    css.opacity = "1";
    css.visibility = "visible";
    css.clipPath = clip;
    parent.appendChild(clone);
    return clone;
  }

  function play(handle, node, frames, durationMs, delayMs, easing) {
    handle.animations.push(
      node.animate(frames, {
        duration: Math.max(MIN_UNIT_MS, durationMs),
        delay: delayMs,
        easing: easing,
        fill: "both",
      }),
    );
  }

  function pieceCount(cap, rect, areaPerPiece) {
    var byArea = (rect.width * rect.height) / areaPerPiece;
    return Math.max(MIN_FRAGMENTS, Math.round(Math.min(cap, byArea) * budget));
  }

  function cloneCount(cap, rect, areaPerPiece) {
    return Math.min(MAX_CLONES, pieceCount(cap, rect, areaPerPiece));
  }

  function grid(count, rect) {
    var columns = Math.max(
      1,
      Math.round(Math.sqrt((count * rect.width) / rect.height)),
    );
    var rows = Math.max(1, Math.round(count / columns));
    var cells = [];
    for (var row = 0; row < rows; row += 1) {
      for (var column = 0; column < columns; column += 1) {
        cells.push({
          left: (column / columns) * PERCENT,
          right: ((column + 1) / columns) * PERCENT,
          top: (row / rows) * PERCENT,
          bottom: ((row + 1) / rows) * PERCENT,
        });
      }
    }
    return cells;
  }

  function centerOf(cell) {
    return {
      x: (cell.left + cell.right) * HALF,
      y: (cell.top + cell.bottom) * HALF,
    };
  }

  function insetClip(cell) {
    return (
      "inset(" +
      cell.top +
      "% " +
      (PERCENT - cell.right) +
      "% " +
      (PERCENT - cell.bottom) +
      "% " +
      cell.left +
      "%)"
    );
  }

  function shards(cell) {
    var topLeft = [cell.left, cell.top];
    var topRight = [cell.right, cell.top];
    var bottomRight = [cell.right, cell.bottom];
    var bottomLeft = [cell.left, cell.bottom];
    return [
      [topLeft, topRight, bottomLeft],
      [topRight, bottomRight, bottomLeft],
    ].map(function (points) {
      return {
        clip:
          "polygon(" +
          points
            .map(function (point) {
              return point[0] + "% " + point[1] + "%";
            })
            .join(", ") +
          ")",
        x: average(points, 0),
        y: average(points, 1),
      };
    });
  }

  function average(points, axis) {
    return (
      points.reduce(function (sum, point) {
        return sum + point[axis];
      }, 0) / points.length
    );
  }

  function origin(x, y) {
    return x + "% " + y + "%";
  }

  function shatter(handle, motionPlan, rng) {
    var exitMs = motionPlan.exitMs;
    var scale = motionPlan.intensity.scale;
    return withFragments(handle, function (layer, rect) {
      var cells = grid(
        cloneCount(motionPlan.intensity.fragments, rect, FRAGMENT_AREA_PX) /
          SHARDS_PER_CELL,
        rect,
      );
      cells.forEach(function (cell) {
        shards(cell).forEach(function (shard) {
          var clone = fragment(handle, layer.element, layer, rect, shard.clip);
          clone.style.transformOrigin = origin(shard.x, shard.y);
          var outX = shard.x / PERCENT - HALF;
          var outY = shard.y / PERCENT - HALF;
          var length = Math.max(Math.hypot(outX, outY), MIN_DIRECTION);
          var reach = SHATTER_DISTANCE_PX * scale * (HALF + rng());
          var x = (outX / length) * reach;
          var y = (outY / length) * reach;
          var delayMs = rng() * FRAGMENT_DELAY_SHARE * exitMs;
          play(
            handle,
            clone,
            [
              { opacity: 1, transform: "none" },
              {
                opacity: 1,
                transform: translate(
                  x * SHATTER_BREAK_SHARE,
                  y * SHATTER_BREAK_SHARE,
                ),
                offset: SHATTER_BREAK_OFFSET,
              },
              {
                opacity: 0,
                transform:
                  translate(x, y + SHATTER_FALL_PX * scale) +
                  " " +
                  rotate(signed(rng) * SHATTER_SPIN_DEG * scale),
              },
            ],
            exitMs - delayMs,
            delayMs,
            ACCELERATE_EASING,
          );
        });
      });
    });
  }

  function dissolve(handle, motionPlan, rng) {
    var exitMs = motionPlan.exitMs;
    return withFragments(handle, function (layer, rect) {
      var cells = grid(
        cloneCount(motionPlan.intensity.fragments, rect, FRAGMENT_AREA_PX),
        rect,
      );
      cells.forEach(function (cell) {
        var clone = fragment(handle, layer.element, layer, rect, insetClip(cell));
        var center = centerOf(cell);
        clone.style.transformOrigin = origin(center.x, center.y);
        var delayMs = rng() * DISSOLVE_DELAY_SHARE * exitMs;
        play(
          handle,
          clone,
          [
            { opacity: 1, transform: "none" },
            { opacity: 0, transform: scaled(DISSOLVE_SHRINK) },
          ],
          exitMs - delayMs,
          delayMs,
          ACCELERATE_EASING,
        );
      });
    });
  }

  function dust(handle, motionPlan, rng) {
    var exitMs = motionPlan.exitMs;
    var scale = motionPlan.intensity.scale;
    return withFragments(handle, function (layer, rect) {
      var cells = grid(
        cloneCount(
          motionPlan.intensity.fragments * DUST_FRAGMENT_FACTOR,
          rect,
          FRAGMENT_AREA_PX / DUST_FRAGMENT_FACTOR,
        ),
        rect,
      );
      cells.forEach(function (cell) {
        var clone = fragment(handle, layer.element, layer, rect, insetClip(cell));
        var center = centerOf(cell);
        var delayMs =
          ((center.x / PERCENT) * DUST_SWEEP_SHARE + rng() * DUST_JITTER_SHARE) *
          exitMs;
        play(
          handle,
          clone,
          [
            { opacity: 1, transform: "none" },
            {
              opacity: 0,
              transform: translate(
                DUST_DRIFT_PX * scale * (HALF + rng()),
                -DUST_RISE_PX * scale * (HALF + rng()),
              ),
            },
          ],
          exitMs - delayMs,
          delayMs,
          LINEAR_EASING,
        );
      });
    });
  }

  function smoke(handle, motionPlan, rng) {
    var exitMs = motionPlan.exitMs;
    var scale = motionPlan.intensity.scale;
    return withFragments(handle, function (layer, rect) {
      var bands = Math.max(1, Math.round(motionPlan.intensity.bands * budget));
      for (var band = 0; band < bands; band += 1) {
        var cell = {
          left: 0,
          right: PERCENT,
          top: (band / bands) * PERCENT,
          bottom: ((band + 1) / bands) * PERCENT,
        };
        var puff = document_.createElement("div");
        puff.style.position = "absolute";
        puff.style.left = "0";
        puff.style.top = "0";
        puff.style.width = layer.box.width + PIXEL_UNIT;
        puff.style.height = layer.box.height + PIXEL_UNIT;
        layer.element.appendChild(puff);
        fragment(handle, puff, layer, rect, insetClip(cell));
        var delayMs =
          ((bands - band - 1) / bands) * FRAGMENT_DELAY_SHARE * exitMs;
        play(
          handle,
          puff,
          [
            { opacity: 1, transform: "none", filter: "blur(0px)" },
            {
              opacity: 0,
              transform:
                translate(
                  signed(rng) * SMOKE_DRIFT_PX * scale,
                  -SMOKE_RISE_PX * scale * (HALF + rng()),
                ) +
                " " +
                scaled(1 + SMOKE_SWELL * scale),
              filter: "blur(" + SMOKE_BLUR_PX * scale + PIXEL_UNIT + ")",
            },
          ],
          exitMs - delayMs,
          delayMs,
          SETTLE_EASING,
        );
      }
    });
  }

  function accentColor(element) {
    var accent = window
      .getComputedStyle(element)
      .getPropertyValue(ACCENT_PROPERTY)
      .trim();
    return accent || FALLBACK_PARTICLE_COLOR;
  }

  function liftAbove(handle) {
    var element = handle.element;
    var computed = window.getComputedStyle(element);
    if (computed.zIndex !== AUTO_Z) {
      return;
    }
    var css = element.style;
    var previousPosition = css.position;
    var previousZ = css.zIndex;
    if (computed.position === STATIC_POSITION) {
      css.position = "relative";
    }
    css.zIndex = LIFTED_Z;
    handle.restores.push(function () {
      css.position = previousPosition;
      css.zIndex = previousZ;
    });
  }

  function openCanvas(handle, behind) {
    var layer = openLayer(handle);
    if (!layer) {
      return null;
    }
    var canvas = document_.createElement("canvas");
    var context = canvas.getContext("2d");
    if (!context) {
      return null;
    }
    if (behind) {
      liftAbove(handle);
    }
    var ratio = Math.min(window.devicePixelRatio || 1, PIXEL_RATIO_CAP);
    canvas.width = Math.ceil(layer.box.width * ratio);
    canvas.height = Math.ceil(layer.box.height * ratio);
    canvas.style.display = "block";
    canvas.style.width = "100%";
    canvas.style.height = "100%";
    layer.element.appendChild(canvas);
    return { context: context, canvas: canvas, ratio: ratio, box: layer.box };
  }

  function localRect(rect, box) {
    return {
      left: rect.left - box.left,
      top: rect.top - box.top,
      width: rect.width,
      height: rect.height,
      centerX: rect.left - box.left + rect.width * HALF,
      centerY: rect.top - box.top + rect.height * HALF,
    };
  }

  function emit(handle, behind, build) {
    var rect = handle.element.getBoundingClientRect();
    if (!measurable(rect)) {
      return;
    }
    var surface = openCanvas(handle, behind);
    if (!surface) {
      return;
    }
    handle.pending.push(
      runParticles(handle, surface, build(localRect(rect, surface.box), rect)),
    );
  }

  function burst(handle, motionPlan, rng) {
    var element = handle.element;
    var rect = element.getBoundingClientRect();
    if (!measurable(rect)) {
      return false;
    }
    var exitMs = motionPlan.exitMs;
    var scale = motionPlan.intensity.scale;
    handle.animations.push(
      element.animate(
        [
          { opacity: 1, transform: "none" },
          { opacity: 0, transform: scaled(1 + BURST_SWELL * scale) },
        ],
        {
          duration: exitMs * BURST_SWELL_SHARE,
          easing: ACCELERATE_EASING,
          fill: "forwards",
        },
      ),
    );
    var surface = openCanvas(handle, false);
    if (surface) {
      runParticles(
        handle,
        surface,
        burstParticles(motionPlan, localRect(rect, surface.box), rect, rng),
      );
    }
    return true;
  }

  function burstParticles(motionPlan, local, rect, rng) {
    var scale = motionPlan.intensity.scale;
    var count = pieceCount(motionPlan.intensity.particles, rect, PARTICLE_AREA_PX);
    var particles = [];
    for (var index = 0; index < count; index += 1) {
      var x = local.left + rng() * local.width;
      var y = local.top + rng() * local.height;
      var length = Math.max(
        Math.hypot(x - local.centerX, y - local.centerY),
        MIN_RECT_PX,
      );
      var speed = BURST_SPEED_PX * scale * (HALF + rng());
      particles.push(
        ballistic(x, y, {
          vx: ((x - local.centerX) / length) * speed,
          vy: ((y - local.centerY) / length) * speed - BURST_LIFT_PX * scale,
          gravity: GRAVITY_PX * scale,
          size: PARTICLE_MIN_PX + rng() * PARTICLE_SPREAD_PX,
          life:
            motionPlan.exitMs *
            (PARTICLE_LIFE_SHARE + rng() * (1 - PARTICLE_LIFE_SHARE)),
          draw: drawSquare,
        }),
      );
    }
    return particles;
  }

  function ballistic(x, y, traits) {
    return {
      x: x,
      y: y,
      vx: traits.vx,
      vy: traits.vy,
      gravity: traits.gravity || 0,
      drag: traits.drag || 0,
      size: traits.size,
      life: traits.life,
      delay: traits.delay || 0,
      color: traits.color || null,
      move: drift,
      fade: fadeOut,
      draw: traits.draw,
    };
  }

  function particleLife(entranceMs, rng) {
    return (
      Math.max(PARTICLE_MIN_LIFE_MS, entranceMs) *
      (PARTICLE_LIFE_SHARE + rng() * (1 - PARTICLE_LIFE_SHARE))
    );
  }

  function edgePoint(local, rng) {
    var horizontal = rng() < local.width / (local.width + local.height);
    var far = rng() < HALF;
    if (horizontal) {
      return {
        x: local.left + rng() * local.width,
        y: far ? local.top + local.height : local.top,
        nx: 0,
        ny: far ? 1 : -1,
      };
    }
    return {
      x: far ? local.left + local.width : local.left,
      y: local.top + rng() * local.height,
      nx: far ? 1 : -1,
      ny: 0,
    };
  }

  function sparks(handle, motionPlan, entranceMs, rng) {
    var scale = motionPlan.intensity.scale;
    arrive(handle, entranceFrames(POP, scale), entranceMs, OVERSHOOT_EASING);
    emit(handle, false, function (local, rect) {
      var count = pieceCount(motionPlan.intensity.particles, rect, PARTICLE_AREA_PX);
      var particles = [];
      for (var index = 0; index < count; index += 1) {
        var edge = edgePoint(local, rng);
        var speed = SPARK_SPEED_PX * scale * (HALF + rng());
        var sideways = signed(rng) * SPARK_JITTER * speed;
        particles.push(
          ballistic(edge.x, edge.y, {
            vx: edge.nx * speed + (edge.nx ? 0 : sideways),
            vy:
              edge.ny * speed + (edge.ny ? 0 : sideways) - SPARK_LIFT_PX * scale,
            gravity: GRAVITY_PX * scale,
            size: SPARK_WIDTH_PX + rng() * SPARK_WIDTH_PX,
            life: particleLife(entranceMs, rng),
            delay: rng() * SPARK_DELAY_SHARE * entranceMs,
            color: rng() < SPARK_HOT_SHARE ? HOT_PARTICLE_COLOR : null,
            draw: drawStreak,
          }),
        );
      }
      return particles;
    });
  }

  function assemble(handle, motionPlan, entranceMs, rng) {
    var scale = motionPlan.intensity.scale;
    arrive(
      handle,
      [
        { opacity: 0, transform: scaled(1 + ASSEMBLE_SWELL * scale) },
        {
          opacity: 0,
          transform: scaled(1 + ASSEMBLE_SWELL * scale),
          offset: ASSEMBLE_REVEAL_OFFSET,
        },
        { opacity: 1, transform: "none" },
      ],
      entranceMs,
      SETTLE_EASING,
    );
    emit(handle, false, function (local, rect) {
      var count = pieceCount(
        motionPlan.intensity.particles * ASSEMBLE_PARTICLE_FACTOR,
        rect,
        PARTICLE_AREA_PX / ASSEMBLE_PARTICLE_FACTOR,
      );
      var particles = [];
      for (var index = 0; index < count; index += 1) {
        var toX = local.left + rng() * local.width;
        var toY = local.top + rng() * local.height;
        var angle = rng() * TURN_RADIANS;
        var reach = ASSEMBLE_SCATTER_PX * scale * (HALF + rng());
        var landAt =
          entranceMs *
          (ASSEMBLE_REVEAL_OFFSET + rng() * (1 - ASSEMBLE_REVEAL_OFFSET));
        var life = landAt * ASSEMBLE_TRAVEL_SHARE;
        var fromX = toX + Math.cos(angle) * reach;
        var fromY = toY + Math.sin(angle) * reach;
        particles.push({
          x: fromX,
          y: fromY,
          fromX: fromX,
          fromY: fromY,
          toX: toX,
          toY: toY,
          size: PARTICLE_MIN_PX + rng() * PARTICLE_SPREAD_PX,
          life: life,
          delay: landAt - life,
          color: null,
          move: converge,
          fade: fadeSwell,
          draw: drawSquare,
        });
      }
      return particles;
    });
  }

  function glowBurst(handle, motionPlan, entranceMs, rng) {
    var scale = motionPlan.intensity.scale;
    arrive(
      handle,
      [
        { opacity: 0, transform: scaled(1 - GLOW_SHRINK * scale) },
        { opacity: 1, transform: "none" },
      ],
      entranceMs,
      OVERSHOOT_EASING,
    );
    emit(handle, true, function (local, rect) {
      var particles = [
        {
          x: local.centerX,
          y: local.centerY,
          halfWidth: local.width * HALF,
          halfHeight: local.height * HALF,
          spread: GLOW_SPREAD_PX * scale,
          blur: GLOW_BLUR_PX * scale,
          life: Math.max(PARTICLE_MIN_LIFE_MS, entranceMs) * GLOW_LIFE_SHARE,
          delay: 0,
          color: null,
          move: stay,
          fade: fadeLate,
          draw: drawGlow,
        },
      ];
      var count = pieceCount(
        motionPlan.intensity.particles * GLOW_RING_SHARE,
        rect,
        PARTICLE_AREA_PX,
      );
      for (var index = 0; index < count; index += 1) {
        var angle = rng() * TURN_RADIANS;
        var speed = GLOW_RING_SPEED_PX * scale * (HALF + rng());
        particles.push(
          ballistic(
            local.centerX + Math.cos(angle) * local.width * HALF,
            local.centerY + Math.sin(angle) * local.height * HALF,
            {
              vx: Math.cos(angle) * speed,
              vy: Math.sin(angle) * speed,
              drag: GLOW_RING_DRAG,
              size: GLOW_STREAK_WIDTH_PX + rng() * GLOW_STREAK_WIDTH_PX,
              life: particleLife(entranceMs, rng),
              color: rng() < SPARK_HOT_SHARE ? HOT_PARTICLE_COLOR : null,
              draw: drawStreak,
            },
          ),
        );
      }
      return particles;
    });
  }

  function easeOut(progress) {
    return 1 - Math.pow(1 - progress, EASE_OUT_POWER);
  }

  function drift(particle, seconds) {
    var keep = particle.drag ? Math.exp(-particle.drag * seconds) : 1;
    particle.vx *= keep;
    particle.vy = particle.vy * keep + particle.gravity * seconds;
    particle.x += particle.vx * seconds;
    particle.y += particle.vy * seconds;
  }

  function converge(particle, seconds, progress) {
    var eased = easeOut(progress);
    particle.x = particle.fromX + (particle.toX - particle.fromX) * eased;
    particle.y = particle.fromY + (particle.toY - particle.fromY) * eased;
  }

  function stay() {}

  function fadeOut(progress) {
    return 1 - progress;
  }

  function fadeLate(progress) {
    return 1 - progress * progress;
  }

  function fadeSwell(progress) {
    return Math.sin(Math.PI * progress);
  }

  function drawSquare(context, particle, ratio, color) {
    context.fillStyle = color;
    context.fillRect(
      particle.x * ratio,
      particle.y * ratio,
      particle.size * ratio,
      particle.size * ratio,
    );
  }

  function drawStreak(context, particle, ratio, color) {
    context.strokeStyle = color;
    context.lineWidth = particle.size * ratio;
    context.lineCap = "round";
    context.beginPath();
    context.moveTo(
      (particle.x - particle.vx * STREAK_SECONDS) * ratio,
      (particle.y - particle.vy * STREAK_SECONDS) * ratio,
    );
    context.lineTo(particle.x * ratio, particle.y * ratio);
    context.stroke();
  }

  function drawGlow(context, particle, ratio, color, progress) {
    var grow = particle.spread * easeOut(progress);
    var offscreen = context.canvas.width + context.canvas.height;
    context.fillStyle = color;
    context.shadowColor = color;
    context.shadowBlur = particle.blur * ratio;
    context.shadowOffsetX = offscreen;
    context.beginPath();
    context.ellipse(
      particle.x * ratio - offscreen,
      particle.y * ratio,
      (particle.halfWidth * GLOW_CORE_SHARE + grow) * ratio,
      (particle.halfHeight * GLOW_CORE_SHARE + grow) * ratio,
      0,
      0,
      TURN_RADIANS,
    );
    for (var pass = 0; pass < GLOW_PASSES; pass += 1) {
      context.fill();
    }
    context.shadowBlur = 0;
    context.shadowOffsetX = 0;
  }

  function runParticles(handle, surface, particles) {
    var context = surface.context;
    var canvas = surface.canvas;
    var ratio = surface.ratio;
    var accent = accentColor(handle.element);
    var started = 0;
    var last = 0;
    var frames = 0;
    var slowFrames = 0;
    var live = particles;

    return new Promise(function (resolve) {
      function step(now) {
        if (!started) {
          started = now;
          last = now;
        }
        var elapsed = now - started;
        var seconds = (now - last) / MS_PER_SECOND;
        if (now - last > SLOW_FRAME_MS) {
          slowFrames += 1;
        }
        last = now;
        frames += 1;
        if (frames === FRAME_SAMPLE && slowFrames > FRAME_SAMPLE * HALF) {
          budget = Math.max(BUDGET_FLOOR, budget * BUDGET_STEP);
          live = live.slice(0, Math.ceil(live.length * BUDGET_STEP));
        }

        context.clearRect(0, 0, canvas.width, canvas.height);
        var alive = 0;
        live.forEach(function (particle) {
          var age = elapsed - particle.delay;
          if (age >= particle.life) {
            return;
          }
          alive += 1;
          if (age < 0) {
            return;
          }
          var progress = age / particle.life;
          particle.move(particle, seconds, progress);
          context.globalAlpha = Math.max(0, particle.fade(progress));
          particle.draw(context, particle, ratio, particle.color || accent, progress);
        });
        context.globalAlpha = 1;
        if (alive) {
          handle.frame = window.requestAnimationFrame(step);
          return;
        }
        handle.frame = 0;
        resolve();
      }

      handle.frame = window.requestAnimationFrame(step);
    });
  }

  window.forgeMotion = Object.freeze({
    plan: plan,
    seed: seed,
    enter: enter,
    exit: exit,
    cancel: cancel,
    cancelAll: cancelAll,
    owns: owns,
  });
})();
