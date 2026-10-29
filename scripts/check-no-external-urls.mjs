// Fails when the built frontend would load anything from the web (fonts, styles, scripts,
// images) or when the app's CSP allows a remote source. Elevant Mail only talks to mail servers.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const DIST = "dist";
const ALLOWED =
  /^https?:\/\/(localhost|ipc\.localhost|mail-asset\.localhost|tauri\.localhost)([:/]|$)/;

function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? files(path) : [path];
  });
}

const problems = [];

for (const file of files(DIST).filter((f) => /\.(html|css)$/.test(f))) {
  const text = readFileSync(file, "utf8");
  // url(...) in style sheets, src= and href= in HTML.
  const pattern = /url\(\s*["']?(https?:\/\/[^"')\s]+)|(?:src|href)\s*=\s*["'](https?:\/\/[^"']+)/g;
  for (const match of text.matchAll(pattern)) {
    const url = match[1] ?? match[2];
    if (!ALLOWED.test(url)) problems.push(`${file}: loads ${url}`);
  }
}

const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
for (const [key, csp] of Object.entries(config.app.security)) {
  if (typeof csp !== "object" || csp === null) continue;
  for (const [directive, sources] of Object.entries(csp)) {
    for (const source of String(sources).split(/\s+/)) {
      if (/^https?:/.test(source) && !ALLOWED.test(source)) {
        problems.push(`tauri.conf.json ${key} ${directive}: allows ${source}`);
      }
      if (source === "https:" || source === "http:" || source === "*") {
        problems.push(`tauri.conf.json ${key} ${directive}: allows every ${source} source`);
      }
    }
  }
}

if (problems.length > 0) {
  console.error("External addresses found:\n" + problems.map((p) => `  ${p}`).join("\n"));
  process.exit(1);
}
console.log(`No external addresses in ${DIST}/ or the app's CSP.`);
