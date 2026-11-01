// Forge supplies window.forge. The frame stays up for the whole stream: it opens
// on the configured wording and each delivery replaces that wording in place.
var LABEL_X_PROPERTY = "--label-x";
var LABEL_Y_PROPERTY = "--label-y";

forge.ready(function (config) {
  placeLabel(config);
  forge.set("headline", config.headline);
  forge.set("subline", config.subline);
  forge.show("#stage");

  forge.content(function (values) {
    forge.set("headline", values.headline);
    forge.set("subline", values.subline);
  });
});

function placeLabel(config) {
  var label = document.querySelector(".corner");

  label.dataset.side = config.label_side || "";
  label.style.setProperty(LABEL_X_PROPERTY, (config.label_offset_x || 0) + "px");
  label.style.setProperty(LABEL_Y_PROPERTY, (config.label_offset_y || 0) + "px");
}
