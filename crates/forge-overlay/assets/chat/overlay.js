// Forge supplies window.forge. Chat keeps its own bounded ring of rows: a
// delivery appends one row and the oldest node leaves once the ring is full.
var ROW_CAP = 15;
var SCOPE_MESSAGE = "message";
var SCOPE_USER = "user";
var SCOPE_PLATFORM = "platform";

function isRemoved(row, removal) {
  if (row.dataset.platform !== removal.platform) {
    return false;
  }
  if (removal.scope === SCOPE_PLATFORM) {
    return true;
  }
  if (removal.scope === SCOPE_MESSAGE) {
    return removal.messageId !== "" && row.dataset.messageId === removal.messageId;
  }
  if (removal.scope === SCOPE_USER) {
    return removal.userId !== "" && row.dataset.userId === removal.userId;
  }
  return false;
}

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

    row.dataset.platform = values.platform || "";
    row.dataset.messageId = values.message_id || "";
    row.dataset.userId = values.user_id || "";

    if (newestOnTop) {
      rows.insertBefore(row, rows.firstChild);
    } else {
      rows.appendChild(row);
    }

    forge.show(row);

    while (rows.children.length > ROW_CAP) {
      rows.removeChild(newestOnTop ? rows.lastChild : rows.firstChild);
    }
  });

  forge.remove(function (removal) {
    Array.from(rows.children).forEach(function (row) {
      if (isRemoved(row, removal)) {
        rows.removeChild(row);
      }
    });
  });
});
