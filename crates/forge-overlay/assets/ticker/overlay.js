// Forge supplies window.forge; every value below arrives from config.json at runtime.
forge.ready(function (config) {
  forge.on(config.event, function (payload) {
    forge.set("headline", forge.tpl(config.headline, payload));
    forge.set("subline", forge.tpl(config.subline, payload));
    forge.sound(config.sound);
    forge.show("#stage", config.duration * 1000);
  });
});
