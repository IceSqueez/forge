// Forge supplies window.forge. The frame stays up for the whole stream: it opens
// on the configured wording and each delivery replaces that wording in place.
forge.ready(function (config) {
  forge.set("headline", config.headline);
  forge.set("subline", config.subline);
  forge.show("#stage");

  forge.content(function (values) {
    forge.set("headline", values.headline);
    forge.set("subline", values.subline);
    forge.sound(config.sound);
  });
});
