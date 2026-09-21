/*
 * Forge supplies window.forge. This page draws nothing. Every delivery it
 * receives is one of two things:
 *
 *   an announcement  { clip_id, clip_path, report_path, clip_media_type,
 *                      clip_duration_ms }
 *   a command        { command: "stop" | "pause" | "resume", clip_id }
 *
 * An announcement is fetched exactly once, played through its own element, and
 * answered with exactly one verdict posted back to report_path: "played" once
 * the element reached its end, otherwise "refused" with a short reason. The
 * body carries the verdict and nothing else. A command with no clip_id reaches
 * every clip still in flight; clips overlap freely and each one settles on its
 * own.
 *
 * The addresses an announcement carries authorize that one clip and are never
 * written into the page, the console, or any request body. Nothing here runs on
 * a timer: a verdict that cannot be delivered, and a clip that never produces
 * one, are both left to forge's own window to close.
 *
 * Opened as its own preview the page registers no delivery handler at all, so
 * it cannot fetch or play anything, and it reveals its note instead.
 */

var PREVIEW_PARAM = "preview";
var PREVIEW_VALUE = "1";
var NOTE_SELECTOR = "#note";

var VERDICT_PLAYED = "played";
var VERDICT_REFUSED = "refused";

var REASON_FETCH_FAILED = "the clip could not be fetched";
var REASON_AUTOPLAY_BLOCKED = "the browser blocked playback without a click";
var REASON_DECODE_FAILED = "the clip could not be decoded";
var REASON_STOPPED = "stopped";
var REASON_STOPPED_BEFORE_START = "stopped before it started";

var COMMAND_STOP = "stop";
var COMMAND_PAUSE = "pause";
var COMMAND_RESUME = "resume";

var AUTOPLAY_ERROR = "NotAllowedError";
var REPORT_METHOD = "POST";
var JSON_MEDIA_TYPE = "application/json";
var NO_STORE = "no-store";

var inFlight = new Map();
var previewing =
  new URLSearchParams(window.location.search).get(PREVIEW_PARAM) ===
  PREVIEW_VALUE;

function text(value) {
  return typeof value === "string" ? value : "";
}

function announce(values) {
  var clipId = text(values.clip_id);
  var clipPath = text(values.clip_path);
  var reportPath = text(values.report_path);
  if (!clipId || !clipPath || !reportPath || inFlight.has(clipId)) {
    return;
  }

  var clip = {
    reportPath: reportPath,
    mediaType: text(values.clip_media_type),
    element: null,
    objectUrl: "",
    settled: false,
  };
  inFlight.set(clipId, clip);

  window
    .fetch(clipPath, { cache: NO_STORE })
    .then(function (response) {
      if (!response.ok) {
        throw new Error(REASON_FETCH_FAILED);
      }
      return response.arrayBuffer();
    })
    .then(function (bytes) {
      if (!clip.settled) {
        play(clipId, clip, bytes);
      }
    })
    .catch(function () {
      settle(clipId, clip, VERDICT_REFUSED, REASON_FETCH_FAILED);
    });
}

function play(clipId, clip, bytes) {
  var blob = clip.mediaType
    ? new window.Blob([bytes], { type: clip.mediaType })
    : new window.Blob([bytes]);
  clip.objectUrl = window.URL.createObjectURL(blob);

  var element = new window.Audio();
  clip.element = element;
  element.addEventListener("ended", function () {
    settle(clipId, clip, VERDICT_PLAYED, "");
  });
  element.addEventListener("error", function () {
    settle(clipId, clip, VERDICT_REFUSED, REASON_DECODE_FAILED);
  });
  element.src = clip.objectUrl;
  start(clipId, clip);
}

function start(clipId, clip) {
  var started = clip.element.play();
  if (!started || !started.catch) {
    return;
  }
  started.catch(function (error) {
    settle(
      clipId,
      clip,
      VERDICT_REFUSED,
      error && error.name === AUTOPLAY_ERROR
        ? REASON_AUTOPLAY_BLOCKED
        : REASON_DECODE_FAILED,
    );
  });
}

function control(values) {
  var command = text(values.command);
  var addressed = text(values.clip_id);

  Array.from(inFlight.keys()).forEach(function (clipId) {
    var clip = inFlight.get(clipId);
    if (clip && (!addressed || addressed === clipId)) {
      apply(clipId, clip, command);
    }
  });
}

function apply(clipId, clip, command) {
  if (command === COMMAND_STOP) {
    settle(
      clipId,
      clip,
      VERDICT_REFUSED,
      clip.element ? REASON_STOPPED : REASON_STOPPED_BEFORE_START,
    );
    return;
  }
  if (!clip.element) {
    return;
  }
  if (command === COMMAND_PAUSE) {
    clip.element.pause();
    return;
  }
  if (command === COMMAND_RESUME) {
    start(clipId, clip);
  }
}

function settle(clipId, clip, verdict, reason) {
  if (clip.settled) {
    return;
  }
  clip.settled = true;
  inFlight.delete(clipId);
  release(clip);
  report(clip.reportPath, verdict, reason);
}

function release(clip) {
  if (clip.element) {
    clip.element.pause();
  }
  if (clip.objectUrl) {
    window.URL.revokeObjectURL(clip.objectUrl);
    clip.objectUrl = "";
  }
}

function report(reportPath, verdict, reason) {
  var body =
    verdict === VERDICT_REFUSED
      ? { verdict: verdict, reason: reason }
      : { verdict: verdict };

  window
    .fetch(reportPath, {
      method: REPORT_METHOD,
      headers: { "Content-Type": JSON_MEDIA_TYPE },
      body: JSON.stringify(body),
      cache: NO_STORE,
    })
    .catch(function () {});
}

forge.ready(function () {
  if (previewing) {
    forge.show(NOTE_SELECTOR);
    return;
  }

  forge.content(function (values) {
    if (text(values.clip_path)) {
      announce(values);
      return;
    }
    if (text(values.command)) {
      control(values);
    }
  });
});
