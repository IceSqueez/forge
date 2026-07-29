// Forge supplies window.forge. Nothing shows until an action delivers content;
// the values arrive already expanded, so this page only places and reveals them.
var FALLBACK_MS = 5000;

forge.ready(function (config) {
  forge.content(function (values, durationMs) {
    forge.set("headline", values.headline);
    forge.set("subline", values.subline);
    forge.sound(config.sound);
    forge.show("#stage", durationMs || config.duration * 1000 || FALLBACK_MS);
  });
});
