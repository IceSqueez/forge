var MESSAGE_SELECTOR = ".message";
var BODY_SELECTOR = ".body";
var ICON_SELECTOR = ".icon";

forge.ready(function (config) {
  placeIcon(config.icon);
  paintSurface(config);
  sizeCard(config);
  var image = config.image ? placeImage(config) : null;

  forge.content(function (values) {
    image = playImage(image, values[SHOW_IMAGE_KEY] || config.image, config);
    present(values.headline, values.subline, image);
  });
});

function present(headline, subline, image) {
  forge.set("headline", headline);
  forge.set("subline", subline);

  var worded = Boolean(headline) || Boolean(subline);
  var iconed = !document.querySelector(ICON_SELECTOR).hidden;
  document.querySelector(BODY_SELECTOR).hidden = !worded;
  document.querySelector(MESSAGE_SELECTOR).hidden = !worded && !iconed;

  if (!worded && !iconed && !image) {
    document.querySelector(STAGE_SELECTOR).classList.add("hidden");
    return;
  }
  forge.show(STAGE_SELECTOR);
}
