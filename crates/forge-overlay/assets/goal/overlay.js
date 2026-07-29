// Forge supplies window.forge. The bar stays up for the whole stream: it opens
// on the configured progress and each delivery replaces it in place.
forge.ready(function (config) {
  applyGoal(config.label, config.value, config.target);
  forge.show("#stage");

  forge.content(function (values) {
    applyGoal(values.label, values.value, values.target);
    forge.sound(config.sound);
  });
});

function applyGoal(label, value, target) {
  forge.set("label", label);
  forge.set("value", value);
  forge.set("target", target);

  var fill = document.querySelector(".fill");
  if (fill) {
    fill.style.width = percentOf(value, target) + "%";
  }
}

function percentOf(value, target) {
  var numerator = Number(value);
  var denominator = Number(target);
  if (!isFinite(numerator) || !isFinite(denominator) || denominator <= 0) {
    return 0;
  }
  return Math.min(100, Math.max(0, (numerator / denominator) * 100));
}
