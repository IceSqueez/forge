// Forge supplies window.forge. Chat keeps its own bounded ring of rows: a
// delivery appends one row and the oldest node leaves once the ring is full.
var ROW_CAP = 15;

forge.ready(function (config) {
  var rows = document.getElementById("rows");
  var template = document.getElementById("row-template");
  var newestOnTop = config.position === "top";

  forge.content(function (values) {
    var row = template.content.firstElementChild.cloneNode(true);

    var author = row.querySelector('[data-bind="author"]');
    author.textContent = values.author || "";
    author.style.color = values.author_color || "";

    row.querySelector(".badges").textContent = values.badges || "";
    row.querySelector('[data-bind="message"]').textContent = values.message || "";

    if (newestOnTop) {
      rows.insertBefore(row, rows.firstChild);
    } else {
      rows.appendChild(row);
    }

    while (rows.children.length > ROW_CAP) {
      rows.removeChild(newestOnTop ? rows.lastChild : rows.firstChild);
    }
  });
});
