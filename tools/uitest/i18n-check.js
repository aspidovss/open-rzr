// Every translation key used by the window exists in English and Russian, and both files have the
// same keys. Exit code 1 when something is missing.
//   node tools/uitest/i18n-check.js
const fs = require("fs");
const path = require("path");
const ui = path.resolve(__dirname, "..", "..", "ui");
const window = {};
for (const l of ["en", "ru"]) new Function("window", fs.readFileSync(path.join(ui, "i18n", l + ".js"), "utf8"))(window);
const en = window.I18N.en, ru = window.I18N.ru;
const src = fs.readFileSync(path.join(ui, "app.js"), "utf8") + fs.readFileSync(path.join(ui, "index.html"), "utf8");
const used = new Set();
for (const m of src.matchAll(/\bt\(\s*"([a-z_0-9]+\.[a-z_0-9.]+)"/g)) used.add(m[1]);
for (const m of src.matchAll(/data-i18n(?:-ph|-title)?="([^"]+)"/g)) used.add(m[1]);
for (const m of src.matchAll(/(?:it|item)\("([a-z_]+\.[a-z_0-9]+)"/g)) used.add(m[1]);
for (const m of src.matchAll(/\["[a-z_]+", "(mac\.[a-z_0-9]+)"\]/g)) used.add(m[1]);
// keys built at run time ("btn.plate_" + n) are prefixes, not keys
for (const k of [...used]) if (/[._]$/.test(k)) used.delete(k);
const problems = [
  ["missing in en", [...used].filter((k) => !(k in en))],
  ["missing in ru", [...used].filter((k) => !(k in ru))],
  ["only in en", Object.keys(en).filter((k) => !(k in ru))],
  ["only in ru", Object.keys(ru).filter((k) => !(k in en))],
].filter(([, v]) => v.length);
for (const [what, v] of problems) console.log(`${what}: ${v.join(" ")}`);
console.log(problems.length ? "i18n: FAILED" : `i18n: OK (${used.size} keys used, ${Object.keys(en).length} defined)`);
process.exit(problems.length ? 1 : 0);
