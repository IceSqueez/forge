/*
 * Forge supplies window.forge. This look draws nothing: the audio forge sends
 * this overlay is played by the shared runtime, like on every other look. Opened
 * as its own preview the page reveals a note saying so.
 */

var PREVIEW_PARAM = "preview";
var PREVIEW_VALUE = "1";
var NOTE_SELECTOR = "#note";

forge.ready(function () {
  if (
    new URLSearchParams(window.location.search).get(PREVIEW_PARAM) ===
    PREVIEW_VALUE
  ) {
    forge.show(NOTE_SELECTOR);
  }
});
