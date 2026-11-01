// Forge supplies window.forge. Nothing shows until an action delivers content;
// the values arrive already expanded, so this page only places and reveals them.
// A tintable icon is a mask the accent fills; an imported one is drawn as it was
// imported. Neither is ever parsed into this document.
var FALLBACK_MS = 5000;
var ICON_SOURCE_PROPERTY = "--icon-source";
var TINTED_CLASS = "tinted";
var STAGE_SELECTOR = "#stage";
var IMAGE_ABOVE_CLASS = "image-above";
var IMAGE_HEIGHT_PROPERTY = "--image-height";
var IMAGE_POSITION_LEFT = "left";
var IMAGE_PLAY_PARAMETER = "?play=";
var FALLBACK_SURFACE = "#14141c";
var HEX_COLOR = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i;
var PERCENT = 100;
var imagePlays = 0;

forge.ready(function (config) {
  placeIcon(config.icon);
  paintSurface(config);
  var image = placeImage(config);

  forge.content(function (values, durationMs) {
    forge.set("headline", values.headline);
    forge.set("subline", values.subline);
    playImage(image, config);
    forge.show(STAGE_SELECTOR, durationMs || config.duration * 1000 || FALLBACK_MS);
  });
});

function placeIcon(icon) {
  var box = document.querySelector(".icon");
  var file = icon && icon.file;

  if (!box) {
    return;
  }
  if (!file) {
    box.hidden = true;
    return;
  }
  if (icon.tintable) {
    box.style.setProperty(ICON_SOURCE_PROPERTY, 'url("' + encodeURI(file) + '")');
    box.classList.add(TINTED_CLASS);
    return;
  }

  var image = document.createElement("img");
  image.src = encodeURI(file);
  image.alt = "";
  box.appendChild(image);
}

function hexChannels(color) {
  var match = HEX_COLOR.exec(color || "");

  return match && [parseInt(match[1], 16), parseInt(match[2], 16), parseInt(match[3], 16)];
}

function paintSurface(config) {
  var opacity = config.background_opacity;

  if (typeof opacity !== "number") {
    return;
  }
  var channels = hexChannels(config.background_color) || hexChannels(FALLBACK_SURFACE);
  document.querySelector(STAGE_SELECTOR).style.background =
    "rgba(" + channels.join(", ") + ", " + opacity / PERCENT + ")";
}

function placeImage(config) {
  if (!config.image) {
    return null;
  }
  var stage = document.querySelector(STAGE_SELECTOR);
  var image = document.createElement("img");
  image.className = "picture";
  image.alt = "";
  image.src = encodeURI(config.image);
  stage.style.setProperty(IMAGE_HEIGHT_PROPERTY, config.image_height + "px");
  if (config.image_position !== IMAGE_POSITION_LEFT) {
    stage.classList.add(IMAGE_ABOVE_CLASS);
  }
  stage.insertBefore(image, stage.firstChild);
  return image;
}

function playImage(image, config) {
  if (!image) {
    return;
  }
  imagePlays += 1;
  image.src = encodeURI(config.image) + IMAGE_PLAY_PARAMETER + imagePlays;
}
