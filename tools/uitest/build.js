// Builds ui/test.html: the real ui/index.html plus a test-only stand-in for Tauri (devmock.js).
// Also writes sigs.js: the argument names of every #[tauri::command] in app/src (camelCase, as
// Tauri expects them from JavaScript), so the mock rejects calls the real app would reject.
//
//   node tools/uitest/build.js
//   open ui/test.html?lang=en  (in a browser, or let smoke.js drive it)
//
// Page options (query string): lang=en|ru, db (device database), ord (OpenRazer driver present),
// rz (Razer Synapse running), os=windows|linux, style=synapse|classic, poly (Polychromatic).
const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..", "..");
const ui = path.join(root, "ui");

function rustFiles(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name);
    return e.isDirectory() ? rustFiles(p) : e.name.endsWith(".rs") ? [p] : [];
  });
}

const camel = (s) => s.replace(/_([a-z])/g, (_, c) => c.toUpperCase());

/** Split at top-level commas (not inside <>, (), []). */
function splitTop(s) {
  const out = [];
  let depth = 0, cur = "";
  for (const ch of s) {
    if ("<([".includes(ch)) depth++;
    if (">)]".includes(ch)) depth--;
    if (ch === "," && depth === 0) { out.push(cur); cur = ""; } else cur += ch;
  }
  out.push(cur);
  return out.map((a) => a.trim()).filter(Boolean);
}

const sigs = {};
for (const f of rustFiles(path.join(root, "app", "src"))) {
  const src = fs.readFileSync(f, "utf8");
  const re = /#\[tauri::command\]\s*(?:pub(?:\(crate\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*(?:<[^>]*>)?\s*\(/g;
  for (const m of src.matchAll(re)) {
    // the parameter list, with balanced parentheses
    let i = m.index + m[0].length, depth = 1, params = "";
    for (; i < src.length && depth > 0; i++) {
      const ch = src[i];
      if (ch === "(") depth++;
      if (ch === ")") depth--;
      if (depth > 0) params += ch;
    }
    sigs[m[1]] = splitTop(params)
      .filter((a) => !/:\s*(?:tauri::)?(State|Window|AppHandle|WebviewWindow)\b/.test(a))
      .map((a) => camel(a.split(":")[0].trim()));
  }
}
fs.writeFileSync(path.join(__dirname, "sigs.js"), "window.__SIGS__ = " + JSON.stringify(sigs) + ";\n");

const html = fs.readFileSync(path.join(ui, "index.html"), "utf8");
const inject = `<script>const _q = new URLSearchParams(location.search); window.__LANG__ = _q.has("lang") ? _q.get("lang") : "ru"; window.__DB__ = _q.has("db") ? (_q.get("db") || true) : false; window.__ORD__ = _q.has("ord"); window.__RZ__ = _q.has("rz"); window.__OS__ = _q.get("os"); window.__STYLE__ = _q.get("style"); window.__POLY__ = _q.has("poly");</script>
<script src="../tools/uitest/sigs.js"></script>
<script src="../tools/uitest/devmock.js"></script>
<script src="i18n/en.js"></script>`;
const marker = '<script src="i18n/en.js"></script>';
if (!html.includes(marker)) throw new Error("ui/index.html: i18n script tag not found");
fs.writeFileSync(path.join(ui, "test.html"), html.replace(marker, inject));
console.log(`ui/test.html written; ${Object.keys(sigs).length} commands in sigs.js`);
