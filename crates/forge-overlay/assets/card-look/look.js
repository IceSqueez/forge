// A tintable icon is a mask the border color fills; an imported one is drawn as it was
// imported. Neither is ever parsed into this document.
var ICON_SOURCE_PROPERTY = "--icon-source";
var TINTED_CLASS = "tinted";
var STAGE_SELECTOR = "#stage";
var IMAGE_ABOVE_CLASS = "image-above";
var IMAGE_HEIGHT_PROPERTY = "--image-height";
var IMAGE_BELOW_CLASS = "image-below";
var IMAGE_POSITION_BELOW = "below";
var IMAGE_POSITION_LEFT = "left";
var IMAGE_POSITION_RIGHT = "right";
var TEXT_CUT_CLASS = "text-cut";
var TEXT_OVERFLOW_ELLIPSIS = "ellipsis";
var CARD_BOUNDS = [
  { key: "card_min_width", property: "--card-min-width" },
  { key: "card_min_height", property: "--card-min-height" },
  { key: "card_max_width", property: "--card-max-width" },
  { key: "card_max_height", property: "--card-max-height" },
];
var IMAGE_PLAY_PARAMETER = "?play=";
var SHOW_IMAGE_KEY = "show_image";
var FALLBACK_SURFACE = "#14141c";
var HEX_COLOR = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i;
var PERCENT = 100;
var imagePlays = 0;

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

function sizeCard(config) {
  var stage = document.querySelector(STAGE_SELECTOR);

  CARD_BOUNDS.forEach(function (bound) {
    var value = config[bound.key];
    if (typeof value !== "number" || !isFinite(value) || value <= 0) {
      return;
    }
    stage.style.setProperty(bound.property, value + "px");
  });
  if (config.text_overflow === TEXT_OVERFLOW_ELLIPSIS) {
    stage.classList.add(TEXT_CUT_CLASS);
  }
}

function placeImage(config) {
  var stage = document.querySelector(STAGE_SELECTOR);
  var image = document.createElement("img");
  image.className = "picture";
  image.alt = "";
  image.src = encodeURI(config.image);
  stage.style.setProperty(IMAGE_HEIGHT_PROPERTY, config.image_height + "px");
  var position = config.image_position;
  var beside = position === IMAGE_POSITION_LEFT || position === IMAGE_POSITION_RIGHT;
  if (!beside) {
    stage.classList.add(position === IMAGE_POSITION_BELOW ? IMAGE_BELOW_CLASS : IMAGE_ABOVE_CLASS);
  }
  if (position === IMAGE_POSITION_BELOW || position === IMAGE_POSITION_RIGHT) {
    stage.appendChild(image);
  } else {
    stage.insertBefore(image, stage.firstChild);
  }
  return image;
}

function playImage(image, file, config) {
  if (!file) {
    removeImage(image);
    return null;
  }
  var shown = image || placeImage(config);
  imagePlays += 1;
  shown.src = encodeURI(file) + IMAGE_PLAY_PARAMETER + imagePlays;
  return shown;
}

function removeImage(image) {
  if (!image) {
    return;
  }
  image.remove();
  var stage = document.querySelector(STAGE_SELECTOR);
  stage.classList.remove(IMAGE_ABOVE_CLASS, IMAGE_BELOW_CLASS);
  stage.style.removeProperty(IMAGE_HEIGHT_PROPERTY);
}
