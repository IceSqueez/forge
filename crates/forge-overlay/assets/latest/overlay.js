// Forge supplies window.forge. The card stays up for the whole stream: forge
// sends the slot's current value as soon as the page connects and again each
// time it changes, so the page keeps nothing of its own. An empty slot arrives
// as a placeholder, and an empty placeholder hides the card.
forge.ready(function (config) {
  document.body.dataset.slot = config.slot || "";

  forge.content(function (values) {
    var stage = document.getElementById("stage");
    var empty = typeof values.placeholder === "string";

    forge.set("label", values.label);
    forge.set("headline", empty ? values.placeholder : values.headline);
    forge.set("subline", empty ? "" : values.subline);
    stage.classList.toggle("empty", empty);

    if (empty && !values.placeholder) {
      stage.classList.add("hidden");
      return;
    }
    forge.show("#stage");
  });
});
