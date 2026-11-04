// Forge supplies window.forge. Nothing shows until an action delivers content;
// the values arrive already expanded, so this page only places and reveals them.
var FALLBACK_MS = 5000;

forge.ready(function (config) {
  placeIcon(config.icon);
  paintSurface(config);
  sizeCard(config);
  var image = config.image ? placeImage(config) : null;

  forge.content(function (values, durationMs) {
    forge.set("headline", values.headline);
    forge.set("subline", values.subline);
    image = playImage(image, values[SHOW_IMAGE_KEY] || config.image, config);
    forge.show(STAGE_SELECTOR, durationMs || config.duration * 1000 || FALLBACK_MS);
  });
});
