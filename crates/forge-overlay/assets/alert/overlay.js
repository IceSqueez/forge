// Forge supplies window.forge. Nothing shows until an action delivers content;
// the values arrive already expanded, so this page only places and reveals them.
// A tintable icon is a mask the accent fills; an imported one is drawn as it was
// imported. Neither is ever parsed into this document.
var FALLBACK_MS = 5000;
var ICON_SOURCE_PROPERTY = "--icon-source";
var TINTED_CLASS = "tinted";

forge.ready(function (config) {
  placeIcon(config.icon);

  forge.content(function (values, durationMs) {
    forge.set("headline", values.headline);
    forge.set("subline", values.subline);
    forge.show("#stage", durationMs || config.duration * 1000 || FALLBACK_MS);
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
