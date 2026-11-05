// Forge supplies window.forge. Chat keeps its own bounded ring of rows: a
// delivery appends one row and the oldest node leaves once the ring is full.
// With hiding on, each row fades out a set time after it appears.
var DEFAULT_ROW_LIMIT = 15;
var ROW_LIMIT_MIN = 1;
var ROW_LIMIT_MAX = 100;
var HIDE_AFTER_MIN_SECS = 1;
var HIDE_AFTER_MAX_SECS = 300;
var ROW_FADE_MS = 300;
var LEAVING_CLASS = "leaving";
var EVENT_ROW_CLASS = "event-row";
var PREVIEW_PARAM = "preview";
var PREVIEW_ON = "1";
var SCOPE_MESSAGE = "message";
var SCOPE_USER = "user";
var SCOPE_PLATFORM = "platform";
var SECURE_SCHEME = "https://";
var EMOTE_CLASS = "emote";
var BADGE_CLASS = "badge";
var BADGE_WORD_CLASS = "badge-word";
var BADGE_ICON_DIRECTORY = "../forge-shared/";
var BADGE_ROLES = {
  BROADCASTER: { icon: "badge-broadcaster.svg", title: "Broadcaster" },
  MOD: { icon: "badge-moderator.svg", title: "Moderator" },
  VIP: { icon: "badge-vip.svg", title: "VIP" },
  SUB: { icon: "badge-subscriber.svg", title: "Subscriber" },
  MEMBER: { icon: "badge-subscriber.svg", title: "Member" },
};
var BADGE_WORD_SEPARATOR = /\s+/;
var PLATFORM_ICON_KEYS = { twitch: "twitch_icon", youtube: "youtube_icon", kick: "kick_icon" };
var FALLBACK_SURFACE = "#14141c";
var HEX_COLOR = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i;
var HEX_RADIX = 16;
var PERCENT = 100;
var ROW_WIDTH_FIT = "fit";
var FIT_ROWS_CLASS = "fit-rows";

function wholeIn(value, min, max) {
  return typeof value === "number" && Math.floor(value) === value && value >= min && value <= max;
}

function rowLimitOf(config) {
  return wholeIn(config.row_limit, ROW_LIMIT_MIN, ROW_LIMIT_MAX) ? config.row_limit : DEFAULT_ROW_LIMIT;
}

function isPreviewPage() {
  return new URLSearchParams(window.location.search).get(PREVIEW_PARAM) === PREVIEW_ON;
}

function hideAfterMsOf(config) {
  if (config.hide_rows !== true || isPreviewPage()) {
    return 0;
  }
  var secs = config.hide_rows_after_secs;
  return wholeIn(secs, HIDE_AFTER_MIN_SECS, HIDE_AFTER_MAX_SECS) ? secs * 1000 : 0;
}

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

function isPageImage(url) {
  return typeof url === "string" && url.toLowerCase().indexOf(SECURE_SCHEME) === 0;
}

function showAvatar(row, url) {
  var image = row.querySelector(".avatar");
  if (!image || !isPageImage(url)) {
    return;
  }
  image.onerror = function () {
    image.hidden = true;
    image.removeAttribute("src");
  };
  image.src = url;
  image.hidden = false;
}

function emoteImage(part) {
  var image = document.createElement("img");
  image.className = EMOTE_CLASS;
  image.alt = part.emote;
  image.onerror = function () {
    image.replaceWith(document.createTextNode(part.emote));
  };
  image.src = part.url;
  return image;
}

function badgeWord(word) {
  var label = document.createElement("span");
  label.className = BADGE_WORD_CLASS;
  label.textContent = word;
  return label;
}

function badgeImage(title, src, fallback) {
  var image = document.createElement("img");
  image.className = BADGE_CLASS;
  image.alt = title;
  image.title = title;
  image.onerror = function () {
    if (fallback) {
      image.replaceWith(badgeWord(fallback));
    } else {
      image.remove();
    }
  };
  image.src = src;
  return image;
}

function badgePictures(images) {
  if (!Array.isArray(images)) {
    return [];
  }
  return images
    .filter(function (image) {
      return image && typeof image.title === "string" && isPageImage(image.url);
    })
    .map(function (image) {
      return badgeImage(image.title, image.url, "");
    });
}

function roleBadge(word) {
  if (!Object.prototype.hasOwnProperty.call(BADGE_ROLES, word)) {
    return badgeWord(word);
  }
  var role = BADGE_ROLES[word];
  return badgeImage(role.title, BADGE_ICON_DIRECTORY + role.icon, word);
}

function fillBadges(target, values) {
  var marks = badgePictures(values.badge_images);
  if (marks.length === 0) {
    marks = (values.badges || "")
      .split(BADGE_WORD_SEPARATOR)
      .filter(function (word) {
        return word !== "";
      })
      .map(roleBadge);
  }
  target.textContent = "";
  marks.forEach(function (mark) {
    target.appendChild(mark);
  });
}

function fillMessage(target, values) {
  var parts = values.message_parts;
  if (!Array.isArray(parts) || parts.length === 0) {
    target.textContent = values.message || "";
    return;
  }
  target.textContent = "";
  parts.forEach(function (part) {
    if (!part || typeof part !== "object") {
      return;
    }
    if (typeof part.emote === "string" && isPageImage(part.url)) {
      target.appendChild(emoteImage(part));
    } else if (typeof part.emote === "string") {
      target.appendChild(document.createTextNode(part.emote));
    } else if (typeof part.text === "string") {
      target.appendChild(document.createTextNode(part.text));
    }
  });
}

function hexChannels(color) {
  var match = HEX_COLOR.exec(color || "");

  return match && [parseInt(match[1], HEX_RADIX), parseInt(match[2], HEX_RADIX), parseInt(match[3], HEX_RADIX)];
}

function rowSurfaceOf(config) {
  var opacity = config.background_opacity;

  if (typeof opacity !== "number") {
    return "";
  }
  var channels = hexChannels(config.background_color) || hexChannels(FALLBACK_SURFACE);
  return "rgba(" + channels.join(", ") + ", " + opacity / PERCENT + ")";
}

function platformIconsOf(config) {
  var icons = {};
  Object.keys(PLATFORM_ICON_KEYS).forEach(function (platform) {
    var file = config[PLATFORM_ICON_KEYS[platform]];
    if (typeof file === "string" && file !== "") {
      icons[platform] = file;
    }
  });
  return icons;
}

function showPlatformIcon(row, file) {
  var image = row.querySelector(".platform-icon");
  if (!image || !file) {
    return;
  }
  image.onerror = function () {
    image.hidden = true;
    image.removeAttribute("src");
  };
  image.src = encodeURI(file);
  image.hidden = false;
}

function awaitsAvatar(row, avatar) {
  var image = row.querySelector(".avatar");
  return (
    Boolean(image) &&
    image.hidden &&
    avatar.userId !== "" &&
    row.dataset.platform === avatar.platform &&
    row.dataset.userId === avatar.userId
  );
}

forge.ready(function (config) {
  var rows = document.getElementById("rows");
  var template = document.getElementById("row-template");
  var newestOnTop = config.position === "top";
  var rowLimit = rowLimitOf(config);
  var hideAfterMs = hideAfterMsOf(config);
  var showsAvatars = config.show_avatars === true;
  var rowSurface = rowSurfaceOf(config);
  var platformIcons = platformIconsOf(config);
  rows.classList.toggle(FIT_ROWS_CLASS, config.row_width === ROW_WIDTH_FIT);

  function detach(row) {
    if (row.parentNode === rows) {
      rows.removeChild(row);
    }
  }

  function fadeOut(row) {
    if (row.parentNode !== rows) {
      return;
    }
    row.classList.add(LEAVING_CLASS);
    window.setTimeout(function () {
      detach(row);
    }, ROW_FADE_MS);
  }

  forge.content(function (values) {
    var row = template.content.firstElementChild.cloneNode(true);

    var author = row.querySelector('[data-bind="author"]');
    author.textContent = values.author || "";
    author.style.color = values.author_color || "";

    fillBadges(row.querySelector(".badges"), values);
    fillMessage(row.querySelector('[data-bind="message"]'), values);

    var event = (values.event || "").trim();
    row.querySelector(".event").textContent = event;
    row.classList.toggle(EVENT_ROW_CLASS, event !== "");

    row.dataset.platform = values.platform || "";
    row.dataset.messageId = values.message_id || "";
    row.dataset.userId = values.user_id || "";

    if (rowSurface) {
      row.style.background = rowSurface;
    }
    if (Object.prototype.hasOwnProperty.call(platformIcons, row.dataset.platform)) {
      showPlatformIcon(row, platformIcons[row.dataset.platform]);
    }

    if (showsAvatars) {
      showAvatar(row, values.avatar || "");
    }

    if (newestOnTop) {
      rows.insertBefore(row, rows.firstChild);
    } else {
      rows.appendChild(row);
    }

    forge.show(row);

    if (hideAfterMs > 0) {
      window.setTimeout(function () {
        fadeOut(row);
      }, hideAfterMs);
    }

    while (rows.children.length > rowLimit) {
      detach(newestOnTop ? rows.lastChild : rows.firstChild);
    }
  });

  forge.remove(function (removal) {
    Array.from(rows.children).forEach(function (row) {
      if (isRemoved(row, removal)) {
        detach(row);
      }
    });
  });

  forge.avatar(function (avatar) {
    if (!showsAvatars) {
      return;
    }
    Array.from(rows.children).forEach(function (row) {
      if (awaitsAvatar(row, avatar)) {
        showAvatar(row, avatar.url);
      }
    });
  });
});
