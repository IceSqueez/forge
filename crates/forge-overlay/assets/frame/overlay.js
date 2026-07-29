// Forge supplies window.forge; every value below arrives from config.json at runtime.
forge.ready(function (config) {
  forge.set("headline", config.headline);
  forge.set("subline", config.subline);
  forge.show("#stage");

  forge.on(config.event, function (payload) {
    forge.set("headline", forge.tpl(config.headline, payload));
    forge.set("subline", forge.tpl(config.subline, payload));
    forge.sound(config.sound);
  });
});
