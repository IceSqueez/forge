// Forge supplies window.forge. The strip stays away until an action delivers a
// line, runs it for the delivery's own duration, then leaves again.
var FALLBACK_MS = 8000;

forge.ready(function (config) {
  forge.content(function (values, durationMs) {
    forge.set("headline", values.headline);
    forge.set("subline", values.subline);
    forge.sound(config.sound);
    forge.show("#stage", durationMs || config.duration * 1000 || FALLBACK_MS);
  });
});
