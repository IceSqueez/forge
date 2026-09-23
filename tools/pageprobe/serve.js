// Mock forge page host: serves the real crate assets for one overlay kind plus a
// config.json you pass in, and answers the runtime's auth frame before pushing one
// content frame. Nothing here is shipped; it exists so a page can be probed.
"use strict";

const http = require("http");
const fs = require("fs");
const path = require("path");
const { WebSocketServer } = require("ws");

const ASSETS = path.resolve(
  __dirname,
  "../../../crates/forge-overlay/assets",
);

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
};

function start({ kind, config, content }) {
  const files = new Map();
  for (const name of ["index.html", "overlay.css", "overlay.js"]) {
    files.set(
      `/${kind}/${name}`,
      fs.readFileSync(path.join(ASSETS, kind, name)),
    );
  }
  files.set(
    "/forge-shared/runtime-v1.js",
    fs.readFileSync(path.join(ASSETS, "shared", "runtime-v1.js")),
  );
  files.set(
    `/${kind}/config.json`,
    Buffer.from(
      JSON.stringify({
        documentVersion: 1,
        generatorVersion: 1,
        overlayId: "probe",
        credential: "probe-credential",
        displayName: "Probe",
        kindId: `overlay.${kind}`,
        configSchemaVersion: 1,
        config,
      }),
    ),
  );
  files.set(
    `/${kind}/sample.json`,
    Buffer.from(
      JSON.stringify({
        documentVersion: 1,
        generatorVersion: 1,
        overlayId: "probe",
        kindId: `overlay.${kind}`,
        content,
      }),
    ),
  );

  const server = http.createServer((request, response) => {
    const body = files.get(request.url.split("?")[0]);
    if (!body) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, {
      "content-type": TYPES[path.extname(request.url.split("?")[0])],
    });
    response.end(body);
  });

  const sockets = new WebSocketServer({ server, path: "/ws/v1/" });
  sockets.on("connection", (socket) => {
    socket.on("message", (raw) => {
      const frame = JSON.parse(String(raw));
      if (frame.request !== "auth") {
        return;
      }
      socket.send(JSON.stringify({ id: frame.id, status: "ok" }));
      socket.send(JSON.stringify({ frame: "content", content }));
    });
  });

  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      resolve({
        url: `http://127.0.0.1:${server.address().port}/${kind}/index.html`,
        close: () =>
          new Promise((done) => {
            sockets.close();
            server.close(done);
          }),
      });
    });
  });
}

module.exports = { start };
