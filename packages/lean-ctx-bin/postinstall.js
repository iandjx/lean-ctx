#!/usr/bin/env node
// postinstall.js — Downloads the correct lean-ctx binary for the current platform.
// Runs automatically after `npm install -g lean-ctx-bin`.
"use strict";

const https = require("https");
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const zlib = require("zlib");

const REPO = "yvgude/lean-ctx";
const BIN_DIR = path.join(__dirname, "bin");
const BIN_PATH = path.join(BIN_DIR, process.platform === "win32" ? "lean-ctx.exe" : "lean-ctx");

// ── Platform detection ────────────────────────────────────────────────────────
function getTarget() {
  const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
  switch (process.platform) {
    case "linux":  return `${arch}-unknown-linux-gnu`;
    case "darwin": return `${arch}-apple-darwin`;
    case "win32":  return "x86_64-pc-windows-msvc";
    default:
      throw new Error(`Unsupported platform: ${process.platform}. Build from source: https://github.com/${REPO}`);
  }
}

// ── Fetch helpers ─────────────────────────────────────────────────────────────
function getJson(url) {
  return new Promise((resolve, reject) => {
    const opts = new URL(url);
    opts.headers = { "User-Agent": "lean-ctx-bin-installer" };
    https.get(opts, (res) => {
      if (res.statusCode === 301 || res.statusCode === 302) {
        return resolve(getJson(res.headers.location));
      }
      let data = "";
      res.on("data", (c) => (data += c));
      res.on("end", () => {
        try { resolve(JSON.parse(data)); }
        catch (e) { reject(new Error(`Failed to parse JSON from ${url}: ${e.message}`)); }
      });
    }).on("error", reject);
  });
}

function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    const opts = new URL(url);
    opts.headers = { "User-Agent": "lean-ctx-bin-installer" };
    https.get(opts, (res) => {
      if (res.statusCode === 301 || res.statusCode === 302) {
        return resolve(downloadFile(res.headers.location, dest));
      }
      if (res.statusCode !== 200) {
        return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
      }
      const out = fs.createWriteStream(dest);
      res.pipe(out);
      out.on("finish", resolve);
      out.on("error", reject);
    }).on("error", reject);
  });
}

// ── tar.gz extraction (no native tar dependency) ──────────────────────────────
function extractBinaryFromTarGz(tarGzPath, binaryName, destPath) {
  // Use system tar (available on macOS + Linux) or fallback to manual extraction
  try {
    fs.mkdirSync(path.dirname(destPath), { recursive: true });
    execSync(`tar -xzf "${tarGzPath}" -C "${path.dirname(destPath)}" "${binaryName}"`, {
      stdio: "pipe",
    });
    return;
  } catch (_) {
    // System tar failed — manual gz+tar parse
  }

  const buf = zlib.gunzipSync(fs.readFileSync(tarGzPath));
  // Walk TAR blocks (512-byte records)
  let offset = 0;
  while (offset + 512 <= buf.length) {
    const header = buf.slice(offset, offset + 512);
    const name = header.slice(0, 100).toString("utf8").replace(/\0/g, "");
    const sizeOctal = header.slice(124, 136).toString("utf8").replace(/\0/g, "").trim();
    const size = parseInt(sizeOctal, 8) || 0;
    offset += 512;
    if (name === binaryName || path.basename(name) === binaryName) {
      fs.mkdirSync(path.dirname(destPath), { recursive: true });
      fs.writeFileSync(destPath, buf.slice(offset, offset + size));
      return;
    }
    offset += Math.ceil(size / 512) * 512;
  }
  throw new Error(`'${binaryName}' not found in archive`);
}

// ── Main ──────────────────────────────────────────────────────────────────────
async function main() {
  if (fs.existsSync(BIN_PATH)) {
    console.log(`lean-ctx already installed at ${BIN_PATH}`);
    return;
  }

  const target = getTarget();
  console.log(`lean-ctx-bin: installing for ${target}...`);

  // Resolve latest release tag
  const release = await getJson(`https://api.github.com/repos/${REPO}/releases/latest`);
  const tag = release.tag_name;
  if (!tag) throw new Error("Could not determine latest release tag.");
  console.log(`  Latest: ${tag}`);

  fs.mkdirSync(BIN_DIR, { recursive: true });

  if (process.platform === "win32") {
    const url = `https://github.com/${REPO}/releases/download/${tag}/lean-ctx-${target}.zip`;
    console.log(`  Downloading: ${url}`);
    const zipPath = path.join(BIN_DIR, "lean-ctx.zip");
    await downloadFile(url, zipPath);
    // Use PowerShell to unzip
    execSync(
      `powershell -Command "Expand-Archive -Path '${zipPath}' -DestinationPath '${BIN_DIR}' -Force"`,
      { stdio: "pipe" }
    );
    fs.unlinkSync(zipPath);
  } else {
    const url = `https://github.com/${REPO}/releases/download/${tag}/lean-ctx-${target}.tar.gz`;
    console.log(`  Downloading: ${url}`);
    const tarPath = path.join(BIN_DIR, "lean-ctx.tar.gz");
    await downloadFile(url, tarPath);
    extractBinaryFromTarGz(tarPath, "lean-ctx", BIN_PATH);
    fs.unlinkSync(tarPath);
    fs.chmodSync(BIN_PATH, 0o755);
  }

  console.log(`  Installed:  ${BIN_PATH}`);
  console.log("lean-ctx ready. Run: lean-ctx --help");
}

main().catch((err) => {
  console.error(`\nlean-ctx-bin install failed: ${err.message}`);
  console.error(`Manual install: https://github.com/${REPO}/releases`);
  process.exit(1);
});
