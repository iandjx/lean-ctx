#!/usr/bin/env node
// Thin shim: finds the downloaded lean-ctx binary and execs it.
"use strict";

const path = require("path");
const fs = require("fs");
const { spawnSync } = require("child_process");

const BIN = path.join(__dirname, process.platform === "win32" ? "lean-ctx.exe" : "lean-ctx");

if (!fs.existsSync(BIN)) {
  console.error(
    "lean-ctx binary not found. Re-run: npm install -g lean-ctx-bin\n" +
    `Expected: ${BIN}`
  );
  process.exit(1);
}

const result = spawnSync(BIN, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 1);
