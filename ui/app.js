"use strict";
// open-rzr UI. All device work happens in Rust (api.call); this file draws, collects input and translates.

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
const el = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k === "text") e.textContent = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else if (v !== undefined && v !== null && v !== false) e.setAttribute(k, v === true ? "" : v);
  }
  for (const c of kids.flat()) if (c != null) e.append(c.nodeType ? c : document.createTextNode(String(c)));
  return e;
};
const hex2 = (n) => "0x" + n.toString(16).toUpperCase().padStart(2, "0");
const rgbHex = (c) => "#" + c.map((x) => x.toString(16).padStart(2, "0")).join("");
const hexRgb = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
// DPI stage indicator colours on the mouse (stage 1..5)
const STAGE_COLORS = ["#e5484d", "#46a758", "#3e63dd", "#12a5b0", "#f5d90a"];

const S = {
  tables: null, info: null, settings: null, conn: null, groups: [], buttons: [], names: {}, selRow: -1,
  plate: null, libSel: null, mouseSel: null, onMouse: [], lib: [],
  logSeq: 0, busy: 0, hotplug: null,
};

// ------------------------------------------------------------------ helpers
function ulog(msg, level = "INFO") { api.call("ui_log", { level, msg }).catch(() => {}); }

/** Rust errors look like "[code] English message": translate by code, keep the detail. */
function errText(e) {
  const m = /^\[(\w+)\]\s*([\s\S]*)$/.exec(String(e));
  if (!m) return String(e);
  const [, code, msg] = m;
  if (["disconnected", "not_found", "no_access", "conflict", "unsupported", "bluetooth"].includes(code)) return t("err." + code);
  if (["failed", "invalid", "io"].includes(code)) return `${t("err." + code)}: ${macroErr(msg)}`;
  return msg;
}
/** Macro compiler errors: "[code|param] message" (see macro_doc.rs). */
function macroErr(msg) {
  const m = /^\[(\w+)\|([^\]]*)\]\s*([\s\S]*)$/.exec(String(msg));
  if (!m) return String(msg);
  const v = t("mace." + m[1], { p: m[2] });
  return v === "mace." + m[1] ? m[3] : v;
}
const errCode = (e) => (/^\[(\w+)\]/.exec(String(e)) || [])[1];

function status(msg, err = false) {
  const s = $("#status");
  s.textContent = (err ? "✗ " : "✓ ") + msg;
  s.classList.toggle("err", err);
  clearTimeout(status.t);
  status.t = setTimeout(() => { s.textContent = ""; s.classList.remove("err"); }, err ? 15000 : 8000);
}

async function run(cmd, args, okMsg) {
  S.busy++; $("#busy").classList.remove("hidden");
  try {
    const r = await api.call(cmd, args);
    if (okMsg) status(typeof okMsg === "function" ? okMsg(r) : okMsg);
    return r;
  } catch (e) {
    status(errText(e), true);
    if (errCode(e) === "disconnected") setDisconnected();
    if (errCode(e) === "conflict") quiet(checkRazer());
    throw e;
  } finally {
    if (--S.busy === 0) $("#busy").classList.add("hidden");
  }
}
const quiet = (p) => p.catch(() => undefined);

function needMouse() {
  if (!S.conn) { status(t("common.connect_first"), true); return false; }
  return true;
}

function fillSelect(sel, items, value) {
  sel.innerHTML = "";
  for (const [v, txt] of items) sel.append(el("option", { value: String(v), text: txt }));
  // keep the first option when `value` is not in the list (an empty <select> would lose the choice)
  if (value !== undefined && items.some(([v]) => String(v) === String(value))) sel.value = String(value);
  else if (sel.options.length) sel.selectedIndex = 0;
}

// labels for codes coming from Rust tables (fall back to the English name from Rust)
const tl = (prefix, code, fallback) => { const k = `${prefix}.${code}`; const v = t(k); return v === k ? fallback : v; };
const keyName = (c) => (S.tables.keys.find(([k]) => k === c) || [0, hex2(c)])[1];
const comboName = (mods, key) => {
  const m = HID.MOD_NAMES.filter(([bit]) => mods & bit).map(([, n]) => n);
  return [...m, key ? keyName(key) : ""].filter(Boolean).join("+");
};

/** Human-readable binding, in the UI language (mirrors Action::describe in Rust). */
/** Service macros (id ≥ 0xF000) that open-rzr writes for media-key turbo: "consumer, pause, release, pause". */
const SERVICE_MACRO = 0xf000;
function serviceTurbo(id) {
  if (id < SERVICE_MACRO || !S.onMouse) return null;
  const m = S.onMouse.find((x) => x.id === id);
  if (!m || m.steps.length !== 4 || m.steps[0].kind !== "consumer") return null;
  return { code: m.steps[0].value, interval: m.steps[1].value + m.steps[3].value };
}

function describe(a) {
  const mouse = (b) => tl("mouse", b, hex2(b));
  switch (a.kind) {
    case "disabled": return t("desc.disabled");
    case "mouse": return t("desc.mouse", { b: mouse(a.button) });
    case "double_click": return t("desc.double", { b: mouse(a.button) });
    case "mouse_turbo": return t("desc.mouse_turbo", { b: mouse(a.button), ms: a.interval });
    case "keyboard": return t("desc.key", { k: comboName(a.modifiers, a.key) });
    case "keyboard_turbo": return t("desc.key_turbo", { k: comboName(a.modifiers, a.key), ms: a.interval });
    case "consumer": return t("desc.consumer", { c: tl("consumer", a.code, "0x" + a.code.toString(16)) });
    case "system": return t("desc.system", { s: tl("system_op", a.code, String(a.code)) });
    case "macro": {
      const svc = serviceTurbo(a.macro_id);
      if (svc) return t("desc.media_turbo", { c: tl("consumer", svc.code, "0x" + svc.code.toString(16)), ms: svc.interval });
      const mode = t("macro_mode." + a.macro_mode);
      return a.macro_mode === "once" ? t("desc.macro_times", { id: a.macro_id, mode, n: a.repeat }) : t("desc.macro", { id: a.macro_id, mode });
    }
    case "dpi":
      if (a.code === 3) return t("desc.dpi_stage", { n: a.stage });
      if (a.code === 5) return t("desc.sniper", { dpi: a.dpi_x });
      return t("desc.dpi", { op: tl("dpi_op", a.code, String(a.code)) });
    case "profile":
      return a.code === 3 ? t("desc.profile_n", { n: a.profile }) : t("desc.profile", { op: tl("profile_op", a.code, String(a.code)) });
    case "hypershift": return t("desc.hypershift");
    case "scroll_mode": return t("desc.scroll_mode");
    default: return t("desc.raw", { hex: (a.raw || []).map((x) => x.toString(16).padStart(2, "0")).join(" ") });
  }
}

/** Device name by USB product id, translated (falls back to the English name from Rust). */
const deviceName = (pid, fallback) => tl("dev", "0x" + pid.toString(16).padStart(4, "0"), fallback);

/** Button name: user's own name, else a translated default by id. */
function buttonName(id) {
  const own = S.names[id.toString(16).padStart(2, "0")];
  if (own) return own;
  const naga = !S.conn || /Naga/.test(S.conn.name || "");
  if (id >= 0x40 && id <= 0x4b && naga) return S.conn && !S.conn.side_plates ? t("name.side", { n: id - 0x40 + 1 }) : t("name.plate", { p: 12, n: id - 0x40 + 1 });
  if (id >= 0x50 && id <= 0x55 && naga) return t("name.plate", { p: 6, n: id - 0x50 + 1 });
  // ids the same on every Razer mouse; the rest are Naga Pro names
  if (!naga && ![1, 2, 3, 4, 5, 9, 0x0a, 0x34, 0x35].includes(id)) return t("name.button", { id: hex2(id) });
  const k = "name.0x" + id.toString(16).padStart(2, "0");
  const v = t(k);
  return v === k ? t("name.button", { id: hex2(id) }) : v;
}

// ------------------------------------------------------------------ modal dialogs
function modal(build) {
  return new Promise((resolve) => {
    const box = $("#dialog");
    box.className = "dialog";
    box.innerHTML = "";
    delete box.dataset.capture;
    const close = (v) => { $("#modal").classList.add("hidden"); document.removeEventListener("keydown", esc, true); resolve(v); };
    const esc = (e) => { if (e.key === "Escape" && !box.dataset.capture) { e.preventDefault(); close(null); } };
    document.addEventListener("keydown", esc, true);
    build(box, close);
    $("#modal").classList.remove("hidden");
    const f = box.querySelector("[autofocus]") || box.querySelector("input,select,button");
    if (f) f.focus();
  });
}

function confirmBox(title, text, okText) {
  return modal((box, close) => {
    box.append(el("h2", { text: title }), el("div", { text }),
      el("div", { class: "buttons" },
        el("button", { text: t("common.cancel"), onclick: () => close(false) }),
        el("button", { class: "primary", text: okText || t("common.yes"), onclick: () => close(true) })));
  });
}

function promptBox(title, label, value = "") {
  return modal((box, close) => {
    const inp = el("input", { value, autofocus: true });
    inp.addEventListener("keydown", (e) => { if (e.key === "Enter") close(inp.value.trim() || null); });
    box.append(el("h2", { text: title }), el("div", { class: "form" }, el("label", { text: label }), inp),
      el("div", { class: "buttons" },
        el("button", { text: t("common.cancel"), onclick: () => close(null) }),
        el("button", { class: "primary", text: t("common.ok"), onclick: () => close(inp.value.trim() || null) })));
  });
}

// ------------------------------------------------------------------ navigation
const DEVICE_PAGES = ["buttons", "perf", "lighting", "power", "device"];
function showPage(name) {
  closePopover();
  S.page = name;
  rememberUi();
  const dev = DEVICE_PAGES.includes(name);
  if (dev) S.devPage = name;
  $$("#nav button").forEach((b) => b.classList.toggle("active", b.dataset.page === name));
  $$("#tabs button").forEach((b) => b.classList.toggle("active", dev ? b.dataset.tab === "device" : b.dataset.tab === name));
  $("#devbar").classList.toggle("hidden", !dev);
  $$(".page").forEach((p) => p.classList.toggle("hidden", p.id !== "page-" + name));
  const needs = !["diag", "settings", "help"].includes(name);
  $("#no-mouse").classList.toggle("hidden", !needs || !!S.conn);
  ulog("tab: " + name, "DEBUG");
  if (name === "diag") pollLog();
  if (name === "settings") loadSettings();
  if (name === "help") quiet(openHelp());
  if (name === "buttons") drawCustomize();
}
const currentPage = () => ($$(".page").find((p) => !p.classList.contains("hidden")) || { id: "" }).id.replace("page-", "");

// ------------------------------------------------------------------ help (ui/docs/<lang>/*.md — the same files are the guide on GitHub)
const HELP_PAGES = ["getting-started", "profiles", "customize", "macros", "performance", "devices", "windows", "linux",
  "command-line", "troubleshooting", "building", "credits"];
const HELP = { lang: null, docs: {}, page: "getting-started", anchor: "" };

async function loadDocs() {
  const lang = i18n.lang === "ru" ? "ru" : "en";
  if (HELP.lang === lang) return;
  const get = async (l, id) => { try { const r = await fetch(`docs/${l}/${id}.md`); return r.ok ? await r.text() : null; } catch (_) { return null; } };
  const docs = {};
  for (const id of HELP_PAGES) {
    const text = (await get(lang, id)) || (await get("en", id));
    if (text) docs[id] = { text, title: (/^#\s+(.+)$/m.exec(text) || [, id])[1], lang: (await get(lang, id)) ? lang : "en" };
  }
  HELP.docs = docs; HELP.lang = lang;
}

/** Open the guide at a page ("macros") or a section ("macros#program-actions"). */
async function openHelp(topic) {
  if (topic) { const [p, a] = topic.split("#"); HELP.page = p; HELP.anchor = a || ""; }
  if (currentPage() !== "help") { showPage("help"); return; }
  await loadDocs();
  drawHelpList();
  drawHelpPage();
}

function drawHelpList() {
  const q = ($("#help-search").value || "").trim().toLowerCase();
  const ul = $("#help-list");
  ul.innerHTML = "";
  for (const id of HELP_PAGES) {
    const d = HELP.docs[id];
    if (!d) continue;
    const words = q.split(/\s+/).filter(Boolean);
    if (words.length && !words.every((w) => d.text.toLowerCase().includes(w))) continue;
    const li = el("li", { class: id === HELP.page ? "sel" : "", text: d.title });
    li.addEventListener("click", () => { HELP.page = id; HELP.anchor = ""; drawHelpList(); drawHelpPage(); });
    ul.append(li);
  }
  if (!ul.childElementCount) ul.append(el("li", { class: "muted", text: t("help.nothing") }));
}

function drawHelpPage() {
  const body = $("#help-body");
  const d = HELP.docs[HELP.page];
  if (!d) { body.textContent = t("help.missing"); return; }
  body.innerHTML = renderMarkdown(d.text, `docs/${d.lang}/`);
  // search words are highlighted in the text
  const words = ($("#help-search").value || "").trim().split(/\s+/).filter((w) => w.length > 1);
  if (words.length) markWords(body, words);
  const target = HELP.anchor ? body.querySelector(`[id="${CSS.escape(HELP.anchor)}"]`) : null;
  if (target) { target.scrollIntoView({ block: "start" }); target.classList.add("flash"); } else body.scrollTop = 0;
  if (!target) body.parentElement.scrollTop = 0;
}

function markWords(root, words) {
  const re = new RegExp("(" + words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|") + ")", "gi");
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const nodes = [];
  while (walk.nextNode()) if (re.test(walk.currentNode.nodeValue)) nodes.push(walk.currentNode);
  for (const n of nodes) {
    const span = document.createElement("span");
    span.innerHTML = escHtml(n.nodeValue).replace(re, "<mark>$1</mark>");
    n.replaceWith(span);
  }
}

const escHtml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** A small Markdown renderer for the guide: headings, paragraphs, lists, tables, code, links, images. */
function renderMarkdown(md, base) {
  const slug = (s) => s.toLowerCase().replace(/<[^>]+>/g, "").replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-|-$/g, "");
  const inline = (s) => {
    const codes = [];
    s = s.replace(/`([^`]+)`/g, (_, c) => { codes.push(c); return `\u0000${codes.length - 1}\u0000`; });
    s = escHtml(s)
      .replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, (_, alt, src) => `<img alt="${alt}" src="${/^https?:/.test(src) ? "" : resolveDoc(base, src)}">`)
      .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, txt, href) => `<a href="#" data-href="${href}">${txt}</a>`)
      .replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>")
      .replace(/(^|[^*\w])\*([^*\n]+)\*(?!\w)/g, "$1<i>$2</i>");
    return s.replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${escHtml(codes[+i])}</code>`);
  };
  const lines = md.replace(/\r/g, "").split("\n");
  let html = "", i = 0;
  const para = [];
  const flush = () => { if (para.length) { html += `<p>${inline(para.join(" "))}</p>`; para.length = 0; } };
  while (i < lines.length) {
    const l = lines[i];
    let m;
    if (/^```/.test(l)) {
      flush();
      const code = [];
      for (i++; i < lines.length && !/^```/.test(lines[i]); i++) code.push(lines[i]);
      html += `<pre><code>${escHtml(code.join("\n"))}</code></pre>`;
      i++; continue;
    }
    if ((m = /^<a id="([\w-]+)"><\/a>$/.exec(l.trim()))) { flush(); html += `<a id="${m[1]}"></a>`; i++; continue; }
    if ((m = /^(#{1,4})\s+(.+)$/.exec(l))) {
      flush();
      const n = m[1].length, txt = inline(m[2]);
      html += `<h${n} id="${slug(m[2])}">${txt}</h${n}>`;
      i++; continue;
    }
    if (/^\s*\|/.test(l) && i + 1 < lines.length && /^\s*\|[\s:|-]+\|\s*$/.test(lines[i + 1])) {
      flush();
      const cells = (row) => row.trim().replace(/^\||\|$/g, "").split("|").map((c) => inline(c.trim()));
      html += "<table><thead><tr>" + cells(l).map((c) => `<th>${c}</th>`).join("") + "</tr></thead><tbody>";
      for (i += 2; i < lines.length && /^\s*\|/.test(lines[i]); i++) html += "<tr>" + cells(lines[i]).map((c) => `<td>${c}</td>`).join("") + "</tr>";
      html += "</tbody></table>";
      continue;
    }
    if ((m = /^(\s*)([-*]|\d+\.)\s+(.*)$/.exec(l))) {
      flush();
      const ordered = /\d/.test(m[2]);
      html += ordered ? "<ol>" : "<ul>";
      while (i < lines.length && (m = /^(\s*)([-*]|\d+\.)\s+(.*)$/.exec(lines[i]))) {
        let item = m[3];
        // continuation lines of the same item
        for (i++; i < lines.length && /^\s{2,}\S/.test(lines[i]) && !/^\s*([-*]|\d+\.)\s/.test(lines[i]); i++) item += " " + lines[i].trim();
        html += `<li>${inline(item)}</li>`;
      }
      html += ordered ? "</ol>" : "</ul>";
      continue;
    }
    if (/^>\s?/.test(l)) { flush(); html += `<blockquote>${inline(l.replace(/^>\s?/, ""))}</blockquote>`; i++; continue; }
    if (!l.trim()) { flush(); i++; continue; }
    para.push(l.trim());
    i++;
  }
  flush();
  return html;
}

function resolveDoc(base, src) {
  const parts = (base + src).split("/");
  const out = [];
  for (const p of parts) { if (p === "..") out.pop(); else if (p !== ".") out.push(p); }
  return out.join("/");
}

function bindHelp() {
  $("#help-search").addEventListener("input", () => {
    drawHelpList();
    // the open page has no match: show the first page that has
    const first = $("#help-list li:not(.muted)");
    if (first && !$("#help-list li.sel")) { HELP.page = HELP_PAGES.find((id) => HELP.docs[id] && HELP.docs[id].title === first.textContent) || HELP.page; HELP.anchor = ""; drawHelpList(); }
    drawHelpPage();
  });
  // links inside the guide: other pages open here; web addresses are copied (open-rzr has no network access)
  $("#help-body").addEventListener("click", async (e) => {
    const a = e.target.closest("a[data-href]");
    if (!a) return;
    e.preventDefault();
    const href = a.dataset.href;
    if (/^https?:/.test(href)) {
      try { await navigator.clipboard.writeText(href); status(t("help.copied", { url: href })); } catch (_) { status(href); }
      return;
    }
    const [file, anchor] = href.split("#");
    HELP.page = file ? file.replace(/^.*\//, "").replace(/\.md$/, "") : HELP.page;
    HELP.anchor = anchor || "";
    drawHelpList(); drawHelpPage();
  });
  // "?" buttons anywhere in the window
  document.addEventListener("click", (e) => {
    const b = e.target.closest("[data-help]");
    if (b) { e.preventDefault(); e.stopPropagation(); quiet(openHelp(b.dataset.help)); }
  }, true);
}

// ------------------------------------------------------------------ connection
function setDisconnected() {
  if (!S.conn && S.hotplug) return;
  S.conn = null;
  document.body.classList.add("no-mouse");
  $("#batt-chip").textContent = "";
  $("#batt-bar").style.width = "0"; $("#batt-text").textContent = "—"; $("#batt-charge").textContent = "";
  $("#dev-kv").innerHTML = "";
  $("#btn-table tbody").innerHTML = "";
  S.onMouse = []; S.profiles = null; S.profile = null;
  $("#prof-select").innerHTML = ""; $("#auto-chip").classList.add("hidden");
  $("#tab-device-name").textContent = t("tab.mouse");
  drawCustomize();
  $("#mouse-macros").innerHTML = ""; $("#flash").textContent = "";
  $("#cur-dpi").textContent = t("perf.current_none");
  $("#dev-notice").classList.add("hidden");
  applyFeatures();
  const sel = $("#dev-select");
  sel.innerHTML = ""; sel.append(el("option", { text: t("top.none") }));
  showPage(currentPage() || "device");
  // look for the mouse until it is back: USB notifications do it at once (onDeviceChange),
  // this slow poll is only a safety net; without notifications poll every 3 s
  clearInterval(S.hotplug);
  S.hotplug = setInterval(async () => {
    if (S.conn || S.busy) return;
    const groups = await quiet(api.call("list_devices"));
    if (groups && groups.length) { ulog("mouse is back"); await rescan(); }
  }, S.info && S.info.hotplug ? 20000 : 3000);
}

function setConnected() {
  clearInterval(S.hotplug); S.hotplug = null;
  document.body.classList.remove("no-mouse");
  $("#no-mouse").classList.add("hidden");
}

async function rescan() {
  ulog("find mice");
  const groups = await run("list_devices");
  S.groups = groups;
  const sel = $("#dev-select");
  sel.innerHTML = "";
  if (!groups.length) {
    setDisconnected();
    status(t("err.not_found"), true);
    return;
  }
  groups.forEach((g, i) => sel.append(el("option", { value: i, text: deviceName(g.pid, g.label) })));
  // reopened from the tray: the mouse that was chosen before
  const i = S.devIndex > 0 && S.devIndex < groups.length ? S.devIndex : 0;
  sel.value = i;
  await connect(i);
}

/** Page and mouse are kept by the app while the window is closed to the tray (the page is destroyed). */
function rememberUi() {
  if (!S.uiRestored) return;
  quiet(api.call("ui_state_set", { state: JSON.stringify({ page: S.page, dev: S.devIndex || 0 }) }));
}

/** Close button with “close to tray”: the window is destroyed, so unsaved work would be lost. */
async function onCloseRequested() {
  if (!(await leaveEditor())) { quiet(api.call("close_cancelled")); return; }
  ulog("window closed to the tray");
  await quiet(api.call("hide_to_tray"));
}

async function connect(i) {
  const g = S.groups[i];
  if (!g) return;
  S.devIndex = i;
  rememberUi();
  try {
    const c = await run("connect", { candidates: g.candidates }, (c) => t("status.connected", { name: deviceName(g.pid, c.name), fw: c.firmware }));
    c.pid = g.pid;
    S.conn = c;
    setConnected();
    S.names = (await quiet(api.call("button_names"))) || {};
    loadDevice(c);
    applyFeatures();
    loadPerf(c);
    loadPower(c);
    buildZones();
    S.profiles = null; S.profile = null; S.defaults = {};
    $("#tab-device-name").textContent = shortName(deviceName(c.pid || 0, c.name));
    if (has("macros")) await quiet(refreshMacros());
    if (has("buttons")) await loadProfiles();
    if (c.lighting.length) await quiet(loadLighting());
    await quiet(checkOpenRazer());
    await quiet(checkRazer());
  } catch (e) {
    setDisconnected();
    if (S.info.os === "linux" && errCode(e) === "no_access") {
      if (await confirmBox(t("udev.title"), t("udev.text"), t("udev.install"))) {
        await run("install_udev", {}, t("udev.done"));
        setTimeout(rescan, 1500);
      }
    }
  }
}

// ------------------------------------------------------------------ features
const has = (f) => !!S.conn && (S.conn.features || []).includes(f);

/** Show only the tabs and cards the connected mouse supports (all of them while disconnected). */
function applyFeatures() {
  const c = S.conn;
  const page = {
    perf: ["dpi", "dpi_stages", "polling"], buttons: ["buttons"], macros: ["macros"],
    power: ["idle", "low_battery"], lighting: ["lighting"],
  };
  for (const [p, fs] of Object.entries(page)) {
    const on = !c || fs.some(has) && (p !== "lighting" || c.lighting.length > 0);
    const b = $(`#nav button[data-page="${p}"]`) || $(`#tabs button[data-tab="${p}"]`);
    if (b) b.classList.toggle("off", !on);
  }
  $$("[data-feat]").forEach((e) => e.classList.toggle("hidden", !!c && !has(e.dataset.feat)));
  $("#btn-dpi").closest(".row").classList.toggle("hidden", !!c && !has("dpi_stages"));
  const cur = currentPage();
  const btn = $(`#nav button[data-page="${cur}"]`) || $(`#tabs button[data-tab="${cur}"]`);
  if (btn && btn.classList.contains("off")) showPage(c && c.features.includes("buttons") ? "buttons" : "device");
}

// ------------------------------------------------------------------ USB notifications
/** A Razer device was plugged in or removed (several events per device: one per HID interface). */
function onDeviceChange(what) {
  ulog("USB: Razer device " + what, "DEBUG");
  clearTimeout(S.usbT);
  S.usbT = setTimeout(async () => {
    if (S.busy) return onDeviceChange(what);
    const groups = (await quiet(api.call("list_devices"))) || [];
    if (!S.conn) {
      if (groups.length) { ulog("mouse plugged in"); await rescan(); }
      return;
    }
    if (!groups.some((g) => g.pid === S.conn.pid)) {
      ulog("mouse removed");
      status(t("err.disconnected"), true);
      setDisconnected();
      return;
    }
    // e.g. the cable was plugged into a wireless mouse: refresh the list, keep the connection
    const sel = $("#dev-select"), cur = S.groups[+sel.value];
    S.groups = groups;
    sel.innerHTML = "";
    groups.forEach((g, i) => sel.append(el("option", { value: i, text: deviceName(g.pid, g.label) })));
    const i = groups.findIndex((g) => cur && g.pid === cur.pid);
    if (i >= 0) sel.value = String(i);
  }, S.info && S.info.os === "linux" ? 1200 : 700); // Linux: let udev set the access rights first
}

// ------------------------------------------------------------------ Razer software (Windows)
async function loadRazer() {
  if (!S.info || S.info.os !== "windows") return null;
  const r = await api.call("razer_software");
  S.razer = r;
  $("#rz-card").classList.remove("hidden");
  const kv = $("#rz-kv");
  kv.innerHTML = "";
  const svc = r.services.map((s) => `${s.display || s.name} — ${s.running ? t("or.running") : t("rz.stopped")}`).join("\n");
  kv.append(el("dt", { text: t("rz.services") }), el("dd", { text: svc || "—", style: "white-space:pre-wrap" }),
    el("dt", { text: t("rz.programs") }), el("dd", { text: r.processes.join(", ") || "—" }));
  $("#btn-rz-stop").disabled = !(r.processes.length || r.services.some((s) => s.running)) && !$("#rz-autostart").checked;
  $("#btn-rz-start").disabled = !r.can_restore;
  return r;
}

/** Synapse talks to the mouse too: warn on the Device tab. */
async function checkRazer() {
  const r = await quiet(loadRazer());
  const active = !!r && (r.processes.length > 0 || r.services.some((s) => s.running));
  $("#rz-notice").classList.toggle("hidden", !active);
  const names = active ? [...r.services.filter((s) => s.running).map((s) => s.display || s.name), ...r.processes] : [];
  if (active) $("#rz-notice-text").textContent = t("rz.notice", { list: names.join(", ") });
  // log only changes: this runs every few seconds
  const key = names.join(", ");
  if (key !== S.razerSeen) {
    if (key) ulog("Razer software running: " + key, "WARN"); else if (S.razerSeen) ulog("Razer software stopped");
    S.razerSeen = key;
  }
}

/** Synapse may start or stop at any time (it restarts its services itself): keep the notice current. */
function watchRazer() {
  if (!S.info || S.info.os !== "windows") return;
  setInterval(() => { if (document.visibilityState === "visible") quiet(checkRazer()); }, 4000);
}

async function stopRazer() {
  const autostart = $("#rz-autostart").checked;
  if (!(await confirmBox(t("rz.title"), t(autostart ? "rz.confirm_autostart" : "rz.confirm"), t("rz.stop")))) return;
  try {
    await run("stop_razer_software", { autostart }, t("rz.stopped_ok"));
  } catch (e) {
    if (errCode(e) === "no_access") status(t("rz.no_uac"), true);
    return;
  }
  await quiet(checkRazer());
  await rescan();
  status(t("rz.stopped_ok"));
}

async function startRazer() {
  if (!(await confirmBox(t("rz.title"), t("rz.confirm_start"), t("rz.start")))) return;
  try { await run("start_razer_software", {}, t("rz.started_ok")); } catch (e) { if (errCode(e) === "no_access") status(t("rz.no_uac"), true); }
  await quiet(checkRazer());
}

// ------------------------------------------------------------------ diagnostics: HID devices, drivers
const hex4 = (n) => n.toString(16).padStart(4, "0");

/** Every HID endpoint: which ones are Razer, their usage pages, Bluetooth — for "my mouse is not found". */
async function showHidList() {
  const v = await run("all_hid_devices");
  await modal((box, close) => {
    box.classList.add("wide");
    const only = el("input", { type: "checkbox", checked: true });
    const table = el("table", { class: "list small" });
    const draw = () => {
      table.innerHTML = "";
      table.append(el("tr", {}, ...["VID:PID", t("diag.hid_name"), t("diag.hid_if"), t("diag.hid_page"), t("diag.hid_bus")].map((h) => el("th", { text: h }))));
      v.filter((d) => !only.checked || d.vid === 0x1532).forEach((d) => table.append(el("tr", { title: d.path },
        el("td", { class: "mono", text: `${hex4(d.vid)}:${hex4(d.pid)}` }), el("td", { text: d.name || "—" }),
        el("td", { text: d.interface < 0 ? "—" : String(d.interface) }),
        el("td", { class: "mono", text: `${hex4(d.usage_page)}/${hex4(d.usage)}${d.usage_page >= 0xff00 ? " ★" : ""}` }),
        el("td", { text: d.bluetooth ? "Bluetooth" : "USB" }))));
      if (table.rows.length === 1) table.append(el("tr", {}, el("td", { colspan: 5, class: "muted", text: t("common.empty") })));
    };
    only.addEventListener("change", draw); draw();
    box.append(el("h2", { text: t("diag.hid") }), el("div", { class: "muted small", text: t("diag.hid_note") }),
      el("label", { class: "check" }, only, " " + t("diag.hid_razer")), el("div", { class: "scroll-box" }, table),
      el("div", { class: "buttons" }, el("button", { class: "primary", text: t("common.ok"), onclick: () => close(null) })));
  });
}

/** The vendor GATT service of Razer mice on Bluetooth (write 1524, notify 1525/1526). */
const RAZER_GATT = "52401523-f97c-7f90-0e7f-6c6f4e36db1c";

/** Diagnostics → Wireless: what a mouse on Bluetooth offers (HID collections, GATT) and probes. */
async function showWireless() {
  let r = await run("wireless_debug", { probe: "" });
  const col = (path) => { const m = /&col(\d+)/i.exec(path || ""); return m ? "Col" + m[1] : (path || "").split(/[\\/#]/).filter(Boolean).pop(); };
  const ids = (a) => (a && a.length ? a.map((x) => "0x" + x.toString(16).padStart(2, "0")).join(" ") : "—");
  const asText = () => {
    const L = [`open-rzr wireless report — address ${r.address || "?"}, battery ${r.battery ?? "?"}`];
    r.hid.forEach((c) => L.push(`HID ${col(c.path)} page ${hex4(c.usage_page)}/${hex4(c.usage)} in ${c.input_len} out ${c.output_len} feature ${c.feature_len} ids in ${ids(c.input_ids)} out ${ids(c.output_ids)} feature ${ids(c.feature_ids)} ${c.error}${c.descriptor ? "\n  descriptor " + c.descriptor : ""}`));
    r.probes.forEach((p) => L.push(`probe ${col(p.path)} ${p.method}: ${p.ok ? "RAZER REPLY" : "—"} ${p.detail}`));
    if (r.gatt_error) L.push("GATT: " + r.gatt_error);
    r.gatt.forEach((g) => { L.push(`GATT service ${g.uuid} ${g.name} (handle ${g.handle})`); g.chars.forEach((c) => L.push(`  ${c.uuid} ${c.name} [${c.props}] ${c.value}`)); });
    r.notes.forEach((n) => L.push("note: " + n));
    xs.forEach((x) => {
      L.push(`GATT exchange — write [${x.request || "listen only"}]${x.error ? " error: " + x.error : ""}`);
      x.steps.forEach((s) => L.push("  " + s));
      x.notifications.forEach((n) => L.push(`  notify +${n.t_ms} ms ${n.uuid} (handle ${n.handle}) ${n.len} bytes: ${n.hex}`));
      if (!x.notifications.length) L.push("  no notifications");
    });
    return L.join("\n");
  };
  const xs = []; // GATT exchanges of this dialog
  await modal((box, close) => {
    box.classList.add("wide");
    const body = el("div", { class: "scroll-box wl" });
    const draw = () => {
      body.innerHTML = "";
      body.append(el("div", { class: "wl-head" },
        el("span", { text: t("wl.address", { a: r.address || "—" }) }),
        el("span", { text: t("wl.battery", { b: r.battery != null ? r.battery + " %" : "—" }) })));
      r.notes.forEach((n) => body.append(el("div", { class: "notice", text: n })));
      // HID collections
      body.append(el("h3", { text: t("wl.hid") }));
      const th = el("table", { class: "list small" });
      th.append(el("tr", {}, ...["", t("diag.hid_page"), t("wl.lens"), t("wl.ids"), ""].map((h) => el("th", { text: h }))));
      r.hid.forEach((c) => th.append(el("tr", { title: c.path + (c.descriptor ? "\n" + c.descriptor : "") },
        el("td", { class: "mono", text: col(c.path) }),
        el("td", { class: "mono", text: `${hex4(c.usage_page)}/${hex4(c.usage)}${c.usage_page >= 0xff00 ? " ★" : ""}` }),
        el("td", { class: "mono", text: `${c.input_len} / ${c.output_len} / ${c.feature_len}` }),
        el("td", { class: "mono small", text: `in ${ids(c.input_ids)} · out ${ids(c.output_ids)} · feat ${ids(c.feature_ids)}` }),
        el("td", { class: "muted small", text: c.error }))));
      body.append(th);
      // probes
      if (r.probes.length) {
        body.append(el("h3", { text: t("wl.probes") }));
        const tp = el("table", { class: "list small" });
        r.probes.forEach((p) => tp.append(el("tr", { class: p.ok ? "ok" : "" }, el("td", { class: "mono", text: col(p.path) }),
          el("td", { text: p.method }), el("td", { class: "mono small", text: (p.ok ? "✓ " : "") + p.detail }))));
        body.append(tp);
      }
      // GATT
      body.append(el("h3", { text: t("wl.gatt") }));
      if (r.gatt_error) body.append(el("div", { class: "muted small", text: r.gatt_error }));
      r.gatt.forEach((g) => {
        body.append(el("div", { class: "wl-svc" + (g.name === "vendor-specific" ? " vendor" : "") }, el("b", { text: g.name || "—" }), el("span", { class: "mono small muted", text: " " + g.uuid })));
        const tg = el("table", { class: "list small" });
        g.chars.forEach((c) => tg.append(el("tr", {}, el("td", { class: "mono small", text: c.uuid.length > 8 ? c.uuid : "0x" + c.uuid }),
          el("td", { text: c.name || "—" }), el("td", { class: "small", text: c.props }), el("td", { class: "mono small", text: c.value }))));
        body.append(tg);
      });
      // Razer GATT channel
      if (r.gatt.some((g) => g.uuid === RAZER_GATT)) {
        body.append(el("h3", { text: t("wl.x_title") }), el("div", { class: "muted small", text: t("wl.x_note") }));
        const mode = el("select", {},
          el("option", { value: "", text: t("wl.x_listen") }),
          el("option", { value: "fw", text: t("wl.x_fw") }),
          el("option", { value: "hex", text: t("wl.x_hex") }));
        const hexIn = el("input", { type: "text", class: "mono", placeholder: "00 1f 00 00 00 02 00 81", spellcheck: "false" });
        const ms = el("input", { type: "number", min: "100", max: "10000", step: "100", value: "1500", class: "num" });
        mode.value = xMode; hexIn.value = xHex; ms.value = xMs;
        const sync = () => { xMode = mode.value; xHex = hexIn.value; xMs = ms.value; hexIn.hidden = mode.value !== "hex"; };
        mode.addEventListener("change", sync); hexIn.addEventListener("input", sync); ms.addEventListener("input", sync);
        sync();
        const send = el("button", { text: t("wl.x_send"), onclick: () => quiet(exchange(mode.value === "hex" ? hexIn.value : mode.value, Number(ms.value) || 1500)) });
        body.append(el("div", { class: "wl-x" }, mode, hexIn, el("label", { class: "small" }, t("wl.x_ms") + " ", ms), send));
        xs.slice().reverse().forEach((x) => {
          const box = el("div", { class: "wl-xres" });
          box.append(el("div", { class: "small" }, el("b", { text: x.request ? t("wl.x_wrote", { n: x.request.split(" ").length }) : t("wl.x_listened") }),
            el("span", { class: "mono small muted", text: " " + (x.request || "") })));
          if (x.error) box.append(el("div", { class: "notice", text: x.error }));
          x.steps.forEach((st) => box.append(el("div", { class: "mono small muted", text: st })));
          if (x.notifications.length) {
            const tn = el("table", { class: "list small" });
            x.notifications.forEach((n) => tn.append(el("tr", { class: "ok" }, el("td", { class: "mono small", text: "+" + n.t_ms + " ms" }),
              el("td", { class: "mono small", text: n.uuid.slice(0, 8) }), el("td", { class: "mono small", text: n.len + " B" }), el("td", { class: "mono small", text: n.hex }))));
            box.append(tn);
          } else if (!x.error) box.append(el("div", { class: "small", text: t("wl.x_none") }));
          body.append(box);
        });
      }
    };
    let xMode = "fw", xHex = "", xMs = "1500";
    const exchange = async (request, listenMs) => {
      status(t("wl.x_busy"));
      const x = await run("wireless_gatt", { request, listenMs }, (y) => y.error || t(y.notifications.length ? "wl.x_got" : "wl.x_none", { n: y.notifications.length }));
      xs.push(x);
      const top = body.scrollTop;
      draw();
      body.scrollTop = top;
    };
    draw();
    const probe = async (scope) => {
      if (scope === "all" && !(await confirmBox(t("wl.title"), t("wl.all_warn"), t("wl.probe_all")))) return;
      status(t("wl.probing"));
      r = await run("wireless_debug", { probe: scope }, (x) => t(x.probes.some((p) => p.ok) ? "wl.found" : "wl.none"));
      draw();
    };
    box.append(el("h2", { text: t("wl.title") }), el("div", { class: "muted small", text: t("wl.note") }), body,
      el("div", { class: "buttons" },
        el("button", { text: t("wl.copy"), onclick: async () => { try { await navigator.clipboard.writeText(asText()); status(t("wl.copied")); } catch (_) { status(t("wl.copy_fail"), true); } } }),
        el("button", { text: t("wl.probe_all"), onclick: () => quiet(probe("all")) }),
        el("button", { text: t("wl.probe"), onclick: () => quiet(probe("vendor")) }),
        el("button", { class: "primary", text: t("common.ok"), onclick: () => close(null) })));
  });
}

/** Drivers of Razer devices: the standard HID driver is all open-rzr needs. */
async function showDrivers() {
  const v = await run("razer_drivers");
  const razer = v.filter((d) => d.razer);
  await modal((box, close) => {
    box.classList.add("wide");
    const table = el("table", { class: "list small" });
    table.append(el("tr", {}, ...[t("diag.drv_device"), t("diag.drv_provider"), t("diag.drv_version"), t("diag.drv_service")].map((h) => el("th", { text: h }))));
    v.forEach((d) => table.append(el("tr", { title: d.instance + (d.inf ? "\n" + d.inf : ""), class: d.razer ? "warn" : "" },
      el("td", { text: d.device || d.instance }), el("td", { text: d.provider || "—" }), el("td", { text: d.version || "—" }),
      el("td", { text: [d.service, d.filters].filter(Boolean).join(" + ") || "—" }))));
    if (!v.length) table.append(el("tr", {}, el("td", { colspan: 4, class: "muted", text: t("common.empty") })));
    box.append(el("h2", { text: t("diag.drivers") }),
      el("div", { class: razer.length ? "notice" : "muted small", text: razer.length ? t("diag.drv_razer", { n: razer.length }) : t("diag.drv_ok") }),
      el("div", { class: "scroll-box" }, table),
      el("div", { class: "buttons" }, el("button", { class: "primary", text: t("common.ok"), onclick: () => close(null) })));
  });
}

// ------------------------------------------------------------------ device page
function plateText(p) {
  if (p == null) return t("plate.unknown");
  return p === 0 ? t("plate.none") : t("plate.n", { n: p });
}

/** Untested models: buttons/macros/profiles found by probing, or a module built from a sibling. */
function showExperimental(c) {
  const exp = (c.experimental || []).map((f) => t("feat." + f));
  const key = "openrzr.exp-hidden." + (c.serial || c.pid);
  let hidden = false;
  try { hidden = localStorage.getItem(key) === "1"; } catch (_) { /* no storage */ }
  const text = exp.length ? t("exp.probed", { list: exp.join(", ") }) : c.source === "family" ? t("exp.family") : "";
  $("#exp-notice-text").textContent = text;
  $("#exp-notice").classList.toggle("hidden", !text || hidden);
  $("#btn-exp-hide").onclick = () => { try { localStorage.setItem(key, "1"); } catch (_) { /* no storage */ } $("#exp-notice").classList.add("hidden"); };
  $("#btn-exp-backup").onclick = () => quiet(doBackup());
}

function loadDevice(c) {
  const kv = $("#dev-kv");
  kv.innerHTML = "";
  const rows = [[t("device.model"), deviceName(c.pid || 0, c.name)],
    [t("device.module"), c.verified ? t("device.module_verified") : c.source === "family" ? t("device.module_family") : t("device.module_db")],
    [t("device.via"), c.openrazer ? t("device.via_or") : t("device.via_hid")],
    [t("device.conn"), c.wireless ? t("device.wireless") : t("device.wired")],
    [t("device.fw"), c.firmware], [t("device.serial"), c.serial || "—"], [t("device.profiles"), c.profiles.join(", ")],
    ...(c.side_plates ? [[t("device.plate"), plateText(S.plate ?? c.plate)]] : []),
    [t("device.path"), c.path]];
  for (const [k, v] of rows) kv.append(el("dt", { text: k }), el("dd", { text: v }));
  const n = $("#dev-notice");
  n.innerHTML = "";
  if (!c.verified) {
    n.append(el("div", { text: t("device.db_note") }),
      el("div", { class: "muted small", text: t("device.features", { list: (c.features || []).map((f) => t("feat." + f)).join(", ") }) }));
  }
  if (c.openrazer) n.append(el("div", { class: "muted small", text: t("device.or_note", { dir: c.openrazer }) }));
  n.classList.toggle("hidden", !n.childElementCount);
  showExperimental(c);
  $("#batt-card").classList.toggle("hidden", !c.has_battery);
  if (c.battery != null) showBattery([c.battery, c.charging]);
  if (S.plate === null || S.plate === undefined) S.plate = c.side_plates ? (c.plate ?? null) : null;
  S.plateChoice = c.plate_choice || "auto";
}

/** "Razer Naga Pro (wired)" -> "NAGA PRO" for the device tab. */
function shortName(n) {
  return n.replace(/^Razer\s+/i, "").replace(/\s*\(.*\)\s*$/, "").toUpperCase();
}

function showBattery([lvl, ch]) {
  $("#batt-bar").style.width = lvl + "%";
  $("#batt-text").textContent = lvl + "%";
  $("#batt-charge").textContent = ch ? t("device.charging") : "";
  $("#batt-chip").textContent = (ch ? "⚡ " : "🔋 ") + lvl + "%";
}

async function refreshBattery() {
  // only with a connected mouse: no fake "0 %" when it is unplugged
  if (!S.conn || !S.conn.has_battery || S.busy) return;
  try { showBattery(await api.call("battery")); } catch (e) {
    if (errCode(e) === "disconnected") { status(t("err.disconnected"), true); setDisconnected(); }
  }
}

// ------------------------------------------------------------------ performance
function buildStages(max) {
  const body = $("#stages-body");
  body.innerHTML = "";
  const defs = [400, 800, 1600, 3200, 6400];
  for (let i = 0; i < max; i++) {
    const x = el("input", { type: "number", class: "num", min: 100, max: 20000, step: 50, value: defs[i] || 800 });
    const y = el("input", { type: "number", class: "num", min: 100, max: 20000, step: 50, value: defs[i] || 800 });
    const link = el("input", { type: "checkbox", class: "link", checked: true });
    const en = el("input", { type: "checkbox", class: "en", checked: i < 3 });
    x.addEventListener("input", () => { if (link.checked) y.value = x.value; });
    link.addEventListener("change", () => { if (link.checked) y.value = x.value; });
    en.addEventListener("change", paintStageDots);
    body.append(el("tr", {},
      el("td", {}, el("span", { class: "dot" }), el("span", { class: "stage-label" })),
      el("td", {}, en),
      el("td", {}, el("input", { type: "radio", name: "active", class: "act", checked: i === 0 })),
      el("td", {}, x), el("td", {}, y), el("td", {}, link)));
  }
  paintStageDots();
}

/** The mouse numbers enabled stages 1..N; the indicator colour follows that number. */
function paintStageDots() {
  let n = 0;
  $$("#stages-body tr").forEach((tr, i) => {
    const on = $(".en", tr).checked;
    if (on) n++;
    const dot = $(".dot", tr);
    dot.style.background = on ? STAGE_COLORS[(n - 1) % STAGE_COLORS.length] : "transparent";
    dot.title = on ? t("perf.color_hint") : "";
    $(".stage-label", tr).textContent = t("perf.stage", { n: i + 1 });
  });
}

function showStages(st, max) {
  const rows = $$("#stages-body tr");
  rows.forEach((tr, i) => {
    const on = st && i < st.stages.length;
    $(".en", tr).checked = !!on;
    const [x, y] = $$("input.num", tr);
    if (on) {
      const [sx, sy] = st.stages[i];
      x.value = sx; y.value = sy; $(".link", tr).checked = sx === sy;
    }
    x.max = y.max = max;
  });
  if (st && rows[st.active - 1]) $(".act", rows[st.active - 1]).checked = true;
  paintStageDots();
}

function showCurDpi(d) { $("#cur-dpi").textContent = d ? t("perf.current", { x: d[0], y: d[1] }) : t("perf.current_none"); }

/** DPI stages of the selected profile. */
function showProfileDpi() {
  const p = curProfile();
  $("#dpi-target").textContent = p ? t("perf.target", { name: p.name, where: slotText(p) }) : "";
  if (S.profile && S.profile.dpi_stages && S.conn) showStages(S.profile.dpi_stages, S.conn.max_dpi);
}

function loadPerf(c) {
  buildStages(c.max_stages || 5);
  showStages(c.stages, c.max_dpi);
  showCurDpi(c.dpi);
  if (c.poll) $("#poll").value = String(c.poll);
}

async function applyDpi() {
  if (!needMouse()) return;
  const stages = [];
  let active = 1;
  $$("#stages-body tr").forEach((tr) => {
    if (!$(".en", tr).checked) return;
    const [x, y] = $$("input.num", tr).map((i) => Math.max(100, Math.min(20000, parseInt(i.value) || 800)));
    stages.push([x, y]);
    if ($(".act", tr).checked) active = stages.length;
  });
  if (!stages.length) return status(t("perf.need_stage"), true);
  const p = curProfile();
  if (p) {
    showCurDpi(await run("profile_set_dpi", { id: p.id, stages, active }, t("perf.saved")));
    markChanged();
    S.profile.dpi_stages = { active, stages };
  } else {
    showCurDpi(await run("set_dpi_stages", { stages, active, profile: 1 }, t("perf.saved")));
  }
}

// ------------------------------------------------------------------ profiles (library, on-board slots, linking)
// Slot colours of the profile indicator: slot 1..5
const SLOT_COLORS = ["#f2f2f2", "#e5484d", "#46a758", "#3e63dd", "#12a5b0"];

const curProfile = () => S.profiles && S.profiles.list.find((p) => p.id === S.profiles.current);

async function loadProfiles() {
  if (!has("buttons")) {
    S.profiles = null; S.profile = null;
    $("#prof-box").classList.add("hidden");
    return;
  }
  $("#prof-box").classList.remove("hidden");
  if (!S.profiles) status(t("prof.reading"));
  S.profiles = await run("profiles");
  const cp = S.profiles.list.find((p) => p.id === S.profiles.current);
  if (cp) status(t("prof.selected", { name: cp.name }));
  if (!S.profiles.current && S.profiles.list.length) S.profiles.current = S.profiles.list[0].id;
  drawProfileSelect();
  $("#set-autowrite").checked = !!S.profiles.autowrite;
  if (S.profiles.current) await openProfile(S.profiles.current, false);
  drawAutoChip();
}

function drawProfileSelect() {
  const sel = $("#prof-select");
  sel.innerHTML = "";
  for (const p of S.profiles.list) {
    const slots = p.slots.length ? "  ●" + p.slots.join(",") : "";
    sel.append(el("option", { value: p.id, text: p.name + slots }));
  }
  sel.value = S.profiles.current;
  drawWriteState();
}

/** "Write to mouse": changes live in the temporary layer until written into the profile's slot. */
function drawWriteState() {
  const b = $("#btn-write");
  const p = S.profiles && S.profiles.list.find((x) => x.id === S.profiles.current);
  if (!p) { b.classList.add("hidden"); return; }
  b.classList.remove("hidden", "ok", "dirty");
  const slots = p.slots.join(", ");
  if (!p.slots.length) {
    $("#write-state").textContent = "";
    $("#write-text").textContent = t("wr.write") + " ▾";
    b.title = t("wr.choose_slot");
  } else if (p.written) {
    b.classList.add("ok");
    $("#write-state").textContent = "✓ ";
    $("#write-text").textContent = t("wr.in_slot", { n: slots });
    b.title = S.profiles.autowrite ? t("wr.auto_hint") : t("wr.written_hint", { n: slots });
  } else {
    b.classList.add("dirty");
    $("#write-state").textContent = "● ";
    $("#write-text").textContent = t("wr.write");
    b.title = t("wr.dirty_hint", { n: slots });
  }
}

/** Mark the selected profile as changed (its slot no longer matches) until the list is reloaded. */
function markChanged() {
  const p = S.profiles && S.profiles.list.find((x) => x.id === S.profiles.current);
  if (p && p.slots.length && !S.profiles.autowrite) { p.written = false; drawWriteState(); }
}

async function writeToMouse(anchor) {
  if (!needMouse()) return;
  const p = S.profiles.list.find((x) => x.id === S.profiles.current);
  if (!p) return;
  if (p.slots.length) {
    if (p.written) return status(t("wr.already", { n: p.slots.join(", ") }));
    status(t("wr.writing"));
    S.profiles = await run("profile_write", { id: p.id }, t("wr.done", { name: p.name, n: p.slots.join(", ") }));
    drawProfileSelect();
    return;
  }
  // in no slot yet: choose one (what is in each slot now is shown)
  popover(anchor, (box, close) => {
    box.append(el("div", { class: "hd" }, el("span", { class: "grow", text: t("wr.choose_slot") })));
    for (let slot = 1; slot <= 5; slot++) {
      const cur = S.profiles.slots[slot - 1];
      const curName = cur ? (S.profiles.list.find((x) => x.id === cur) || {}).name || cur : t("onb.empty");
      const ok = S.conn.profiles.includes(slot);
      box.append(el("button", { class: "mi slot-pick", disabled: !ok, onclick: async () => {
        close();
        if (cur && !(await confirmBox(t("wr.title"), t("wr.replace", { n: slot, old: curName, name: p.name }), t("wr.write")))) return;
        status(t("onb.writing", { n: slot }));
        S.profiles = await run("slot_assign", { slot, id: p.id }, t("onb.written", { n: slot, name: p.name }));
        drawProfileSelect();
      } }, el("span", { class: "sd", style: `background:${SLOT_COLORS[slot - 1]}` }), el("b", { text: String(slot) }),
        el("span", { class: "muted", text: curName })));
    }
  }, "onb");
}

/** Load a profile into the window; `select` = make it the selected one (written to the mouse if needed). */
async function openProfile(id, select = true) {
  S.profile = select ? await run("profile_select", { id }, (p) => t("prof.selected", { name: p.name })) : await run("profile_get", { id });
  S.profiles.current = id;
  $("#prof-select").value = id;
  await loadDefaults();
  drawCustomize();
  showProfileDpi();
}

/** Factory bindings of the buttons in the profile (to mark changed ones in green). */
async function loadDefaults() {
  S.defaults = S.defaults || {};
  for (const k of Object.keys(S.profile.buttons)) {
    const id = parseInt(k, 16);
    if (!(id in S.defaults)) S.defaults[id] = (await quiet(api.call("default_action", { id }))) || null;
  }
}

function sameAction(a, b) {
  if (!a || !b) return false;
  return describe(a) === describe(b);
}

function slotText(p) {
  if (!p || !p.slots.length) return t("prof.temporary");
  return S.profiles && S.profiles.autowrite ? t("prof.in_slots", { list: p.slots.join(", ") }) : t("wr.temp_then", { list: p.slots.join(", ") });
}

function profileMenu(anchor) {
  const p = curProfile();
  popover(anchor, (box, close) => {
    const item = (key, fn, dis = false) => box.append(el("button", { class: "mi", text: t(key), disabled: dis, onclick: () => { close(); fn(); } }));
    item("prof.add", addProfile);
    item("prof.import", importProfile);
    box.append(el("hr"));
    item("prof.link", linkPrograms, !p);
    box.append(el("hr"));
    item("prof.rename", renameProfile, !p);
    item("prof.duplicate", duplicateProfile, !p);
    item("prof.export", exportProfile, !p);
    box.append(el("hr"));
    item("prof.reload", reloadProfile, !p);
    item("prof.reset", resetProfile, !p);
    item("prof.delete", deleteProfile, !p || S.profiles.list.length < 2);
    box.append(el("hr"));
    item("btn.backup", doBackup);
    item("btn.restore", doRestore);
  });
}

/** Small menu under a button; closes on outside click / Esc. */
function popover(anchor, build, cls = "") {
  closePopover();
  const box = el("div", { class: "pop " + cls });
  const close = () => closePopover();
  build(box, close);
  document.body.append(box);
  const r = anchor.getBoundingClientRect();
  box.style.left = Math.min(r.left, window.innerWidth - box.offsetWidth - 8) + "px";
  box.style.top = r.bottom + 4 + "px";
  S.pop = box;
  setTimeout(() => {
    S.popOff = (e) => { if (!box.contains(e.target)) closePopover(); };
    S.popEsc = (e) => { if (e.key === "Escape") closePopover(); };
    document.addEventListener("mousedown", S.popOff);
    document.addEventListener("keydown", S.popEsc);
  });
  return box;
}
function closePopover() {
  if (S.pop) S.pop.remove();
  S.pop = null;
  document.removeEventListener("mousedown", S.popOff);
  document.removeEventListener("keydown", S.popEsc);
}

async function addProfile() {
  const r = await modal((box, close) => {
    const name = el("input", { class: "grow", value: t("prof.new_name") });
    const from = el("select");
    fillSelect(from, [["default", t("prof.from_default")], ["current", t("prof.from_current")],
      ...S.conn.profiles.map((n) => ["slot:" + n, t("prof.from_slot", { n })])], "default");
    box.append(el("h2", { text: t("prof.add") }),
      el("div", { class: "form" }, el("label", { text: t("prof.name") }), name, el("label", { text: t("prof.from") }), from),
      el("div", { class: "buttons" }, el("button", { text: t("common.cancel"), onclick: () => close(null) }),
        el("button", { class: "primary", text: t("prof.add"), onclick: () => close({ name: name.value.trim(), from: from.value }) })));
  });
  if (!r || !r.name) return;
  const id = await run("profile_create", r);
  S.profiles = await run("profiles");
  drawProfileSelect();
  await openProfile(id);
}

async function importProfile() {
  const input = el("input", { type: "file", accept: ".json,application/json" });
  input.addEventListener("change", async () => {
    const f = input.files[0];
    if (!f) return;
    const text = await f.text();
    const id = await run("profile_import", { text }, t("prof.imported", { file: f.name }));
    S.profiles = await run("profiles");
    drawProfileSelect();
    await openProfile(id);
  });
  input.click();
}

async function renameProfile() {
  const p = curProfile();
  const name = await promptBox(t("prof.rename"), t("prof.name"), p.name);
  if (!name || name === p.name) return;
  await run("profile_rename", { id: p.id, name }, t("prof.renamed"));
  S.profiles = await run("profiles");
  drawProfileSelect();
}

async function duplicateProfile() {
  const id = await run("profile_duplicate", { id: S.profiles.current }, t("prof.duplicated"));
  S.profiles = await run("profiles");
  drawProfileSelect();
  await openProfile(id);
}

async function exportProfile() {
  const path = await run("profile_export", { id: S.profiles.current });
  const open = await confirmBox(t("prof.export"), t("prof.exported", { path }), t("btn.open_folder"));
  if (open) quiet(api.call("open_folder_of", { path }));
}

async function reloadProfile() {
  const p = curProfile();
  if (!(await confirmBox(t("prof.reload"), t("prof.reload_text", { where: slotText(p) }), t("prof.reload")))) return;
  S.profile = await run("profile_reload", { id: p.id }, t("prof.reloaded"));
  S.profiles = await run("profiles"); drawProfileSelect();
  drawCustomize(); showProfileDpi();
}

async function resetProfile() {
  const p = curProfile();
  if (!(await confirmBox(t("prof.reset"), t("prof.reset_text", { name: p.name, where: slotText(p) }), t("prof.reset")))) return;
  S.profile = await run("profile_reset", { id: p.id }, t("prof.reset_done"));
  markChanged();
  drawCustomize(); showProfileDpi();
}

async function deleteProfile() {
  const p = curProfile();
  if (!(await confirmBox(t("prof.delete"), t("prof.delete_text", { name: p.name }), t("prof.delete")))) return;
  await run("profile_delete", { id: p.id }, t("prof.deleted"));
  S.profiles = await run("profiles");
  S.profiles.current = S.profiles.list[0].id;
  drawProfileSelect();
  await openProfile(S.profiles.current);
}

/** On-board memory: which library profile sits in which of the 5 slots. */
function onboardMenu(anchor) {
  popover(anchor, (box) => {
    box.append(el("div", { class: "hd" }, el("span", { class: "grow", text: t("onb.title") }),
      el("span", { class: "help", title: t("onb.help"), text: "?" })));
    for (let slot = 1; slot <= 5; slot++) {
      const sel = el("select");
      fillSelect(sel, [["", t("onb.empty")], ...S.profiles.list.map((p) => [p.id, p.name])], S.profiles.slots[slot - 1] || "");
      sel.disabled = !S.conn.profiles.includes(slot);
      sel.addEventListener("change", async () => {
        const id = sel.value || null;
        if (id) status(t("onb.writing", { n: slot }));
        S.profiles = await run("slot_assign", { slot, id }, id ? t("onb.written", { n: slot, name: sel.selectedOptions[0].text }) : t("onb.cleared", { n: slot }));
        drawProfileSelect();
      });
      box.append(el("div", { class: "slot" }, el("span", { class: "sd", style: `background:${SLOT_COLORS[slot - 1]}` }),
        el("span", { class: "muted small", text: String(slot) }), sel));
    }
    const f = S.profiles.flash;
    const free = f && f[0] ? Math.round((f[1] / f[0]) * 10000) / 100 : null;
    box.append(el("div", { class: "ft" }, el("span", { class: "grow", text: t("onb.available") }), el("span", { text: free == null ? "—" : free + "%" })));
  }, "onb");
}

/** "Link programs": like Synapse's ADD A GAME/PROGRAM, plus the auto switch settings. */
async function linkPrograms() {
  const p = curProfile();
  let links = [...p.links];
  const progs = await run("running_programs");
  const a = S.profiles.autoswitch;
  const r = await modal((box, close) => {
    box.classList.add("wide");
    const linked = el("div", { class: "linked" });
    const drawLinked = () => {
      linked.innerHTML = "";
      if (!links.length) linked.append(el("span", { class: "muted", style: "border:0;background:none", text: t("link.none") }));
      for (const l of links) linked.append(el("span", {}, l, el("button", { text: "×", title: t("common.delete"), onclick: () => { links = links.filter((x) => x !== l); drawLinked(); draw(); } })));
    };
    const q = el("input", { class: "grow", placeholder: t("link.search") });
    const list = el("div", { class: "proglist" });
    const draw = () => {
      const s = q.value.trim().toLowerCase();
      list.innerHTML = "";
      for (const g of progs) {
        if (s && !g.name.toLowerCase().includes(s) && !g.title.toLowerCase().includes(s) && !g.path.toLowerCase().includes(s)) continue;
        const on = links.some((l) => l.toLowerCase() === g.name.toLowerCase());
        list.append(el("div", { class: "prog" },
          el("div", { class: "pn" }, el("div", { text: g.title || g.name }), el("div", { class: "pp", text: g.path })),
          el("button", { class: on ? "" : "add", text: on ? t("link.added") : t("link.add"), disabled: on,
            onclick: () => { links.push(g.name); drawLinked(); draw(); } })));
      }
      if (!list.childElementCount) list.append(el("div", { class: "prog muted", text: t("link.empty") }));
    };
    const browse = el("input", { type: "file", accept: ".exe", style: "display:none" });
    browse.addEventListener("change", () => { const f = browse.files[0]; if (f && !links.includes(f.name)) { links.push(f.name); drawLinked(); draw(); } });
    const manual = el("input", { class: "mono", placeholder: t("link.manual_ph"), style: "width:200px" });
    const en = el("input", { type: "checkbox", checked: a.enabled });
    const fb = el("select");
    fillSelect(fb, [["", t("link.fallback_keep")], ...S.profiles.list.map((x) => [x.id, x.name])], a.fallback);
    const tg = el("select");
    fillSelect(tg, [[0, t("link.target_temp")], ...[1, 2, 3, 4, 5].filter((n) => S.conn.profiles.includes(n)).map((n) => [n, t("link.target_slot", { n })])], a.target);
    q.addEventListener("input", draw);
    drawLinked(); draw();
    box.append(el("h2", { text: t("link.title", { name: p.name }) }),
      el("div", { class: "muted small", text: t("link.text") }),
      ...(S.profiles.linking ? [] : [el("div", { class: "notice warn", text: t("link.unsupported") })]),
      linked,
      el("div", { class: "row" }, q, el("button", { text: t("link.refresh"), onclick: async () => { progs.splice(0, progs.length, ...(await run("running_programs"))); draw(); } })),
      list,
      el("div", { class: "row" }, el("span", { class: "muted", text: t("link.not_listed") }),
        el("button", { text: t("link.browse"), onclick: () => browse.click() }), manual,
        el("button", { text: t("link.add"), onclick: () => { const v = manual.value.trim(); if (v && !links.includes(v)) { links.push(v); manual.value = ""; drawLinked(); draw(); } } }), browse),
      el("div", { class: "card" },
        el("label", { class: "check" }, en, el("span", { text: t("link.enable") })),
        el("div", { class: "form" }, el("label", { text: t("link.fallback") }), fb, el("label", { text: t("link.target") }), tg),
        el("div", { class: "muted small", text: t("link.target_note") })),
      el("div", { class: "buttons" }, el("button", { text: t("common.cancel"), onclick: () => close(null) }),
        el("button", { class: "primary", text: t("common.save"), onclick: () => close({ links, enabled: en.checked, fallback: fb.value, target: +tg.value }) })));
  });
  if (!r) return;
  await run("profile_links", { id: p.id, links: r.links });
  await run("set_autoswitch", { enabled: r.enabled, fallback: r.fallback, target: r.target }, t("link.saved"));
  S.profiles = await run("profiles");
  drawAutoChip();
}

function drawAutoChip() {
  const c = $("#auto-chip");
  const on = S.profiles && S.profiles.autoswitch.enabled && S.profiles.list.some((p) => p.links.length);
  c.classList.toggle("hidden", !on);
  c.textContent = t("link.chip");
  c.title = S.profiles ? S.profiles.list.filter((p) => p.links.length).map((p) => `${p.name}: ${p.links.join(", ")}`).join("\n") : "";
}

/** The program in front changed and a linked profile was written to the mouse. */
function onProfileSwitched(e) {
  status(t("link.switched", { name: e.name, program: e.program }));
  if (S.profiles && S.profiles.list.some((p) => p.id === e.id)) {
    S.profiles.current = e.id;
    quiet(openProfile(e.id, false));
  }
}

// ------------------------------------------------------------------ customize (buttons)
// Mouse schematic (original drawing): anchor points of the top/wheel buttons and where their labels go.
const FIG = {
  0x01: { at: [403, 92], side: "l", y: 70 },
  0x03: { at: [450, 118], side: "l", y: 122 },
  0x34: { at: [440, 132], side: "l", y: 174 },
  0x04: { at: [372, 226], side: "l", y: 278 },
  0x05: { at: [372, 196], side: "l", y: 226 },
  0x02: { at: [497, 92], side: "r", y: 70 },
  0x09: { at: [450, 104], side: "r", y: 114 },
  0x0a: { at: [450, 146], side: "r", y: 158 },
  0x35: { at: [460, 132], side: "r", y: 202 },
  0x0b: { at: [450, 170], side: "r", y: 246 },
  0x0c: { at: [450, 188], side: "r", y: 290 },
  0x0e: { at: [450, 352], side: "r", y: 360 },
};

const pairOf = (id) => S.profile && S.profile.buttons[hex2(id).toLowerCase()];
/** Button that holds Hypershift (its standard binding is "hypershift"). */
const isHsMod = (id) => { const b = pairOf(id); return !!b && b.normal.kind === "hypershift"; };

/** Binding in the shown layer. The Hypershift layer shows what is really there: an empty binding is "Not assigned". */
function bindingOf(id) {
  const b = pairOf(id);
  if (!b) return null;
  if (!S.hs || b.normal.kind === "hypershift") return b.normal;
  return b.hypershift || { ...defaultAction(), kind: "disabled" };
}

/** Label text on the drawing / list: ⇧ marks the Hypershift button; empty Hypershift bindings say so. */
function bindText(id, a, short = true) {
  const s = S.hs && a.kind === "disabled" ? t("hs.unset") : short ? shortDesc(a) : describe(a);
  return isHsMod(id) ? "⇧ " + s : s;
}

/** Binding text as Synapse shows it: "1", "Left click", "DPI: Stage up". */
function shortDesc(a) {
  const d = describe(a);
  return ["keyboard", "mouse"].includes(a.kind) ? d.replace(/^[^:]+:\s*/, "") : d;
}

/** A Naga without plates (Naga X, Left-Handed, V2 HyperSpeed): 12 fixed thumb keys 0x40..0x4B. */
const hasKeypad = () => !!S.conn && !S.conn.side_plates && !!S.profile && range(0x40, 12).some((id) => pairOf(id));

function drawFigure() {
  const view = String(S.figView);
  if ((S.conn.side_plates && ["2", "6", "12"].includes(view)) || (hasKeypad() && view === "12")) return drawSide(+view);
  const fig = $("#fig");
  fig.innerHTML = "";
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 860 420");
  svg.innerHTML = MOUSE_ART.defs + MOUSE_ART.top + (S.conn.side_plates ? "" : MOUSE_ART.topSide);
  // parts this mouse does not have (other models) are left out of the drawing
  svg.querySelectorAll(".mouse-part[data-id]").forEach((p) => { if (!bindingOf(+p.dataset.id)) p.remove(); });
  const L = 330, R = 570;
  for (const [k, f] of Object.entries(FIG)) {
    const id = +k;
    const a = bindingOf(id);
    if (!a) continue;
    if (S.conn.side_plates && plateOf(id)) continue; // side buttons are on the side views
    const x2 = f.side === "l" ? L : R;
    const line = document.createElementNS(ns, "polyline");
    line.setAttribute("class", "lead");
    line.setAttribute("points", `${f.at[0]},${f.at[1]} ${x2 + (f.side === "l" ? 40 : -40)},${f.y} ${x2},${f.y}`);
    const dot = document.createElementNS(ns, "circle");
    dot.setAttribute("class", "lead-dot"); dot.setAttribute("cx", f.at[0]); dot.setAttribute("cy", f.at[1]); dot.setAttribute("r", 2.2);
    svg.append(line, dot);
    const def = S.defaults && S.defaults[id];
    const mod = def ? !sameAction(a, def) : false;
    const lbl = el("div", { class: `lbl ${f.side}${mod ? " mod" : ""}${a.kind === "disabled" ? " off" : ""}${isHsMod(id) ? " hsmod" : ""}`, title: `${buttonName(id)} (${hex2(id)})` },
      bindText(id, a), el("span", { class: "sm", text: buttonName(id) }));
    lbl.style.top = f.y + "px";
    if (f.side === "l") lbl.style.right = (860 - L + 8) + "px"; else lbl.style.left = (R + 8) + "px";
    lbl.addEventListener("click", () => editButton(id));
    lbl.addEventListener("contextmenu", (e) => { e.preventDefault(); rowMenu(id); });
    lbl.addEventListener("mouseenter", () => { const p = svg.querySelector(`[data-id="${id}"]`); if (p) p.classList.add("hl"); });
    lbl.addEventListener("mouseleave", () => { const p = svg.querySelector(`[data-id="${id}"]`); if (p) p.classList.remove("hl"); });
    fig.append(lbl);
  }
  // buttons without a place on the drawing (e.g. a sensitivity clutch of another model): a free slot each
  const extra = Object.keys(S.profile.buttons).map((k) => parseInt(k, 16))
    .filter((id) => !FIG[id] && !plateOf(id) && bindingOf(id) && (bindingOf(id).kind !== "disabled" || S.hs));
  const slots = [["l", 330], ["l", 382], ["r", 404], ["l", 20], ["r", 20]];
  extra.slice(0, slots.length).forEach((id, i) => {
    const [side, y] = slots[i], a = bindingOf(id);
    const lbl = el("div", { class: `lbl ${side} extra${a.kind === "disabled" ? " off" : ""}${isHsMod(id) ? " hsmod" : ""}`, title: `${buttonName(id)} (${hex2(id)})` },
      bindText(id, a), el("span", { class: "sm", text: buttonName(id) }));
    lbl.style.top = y + "px";
    if (side === "l") lbl.style.right = (860 - L + 8) + "px"; else lbl.style.left = (R + 8) + "px";
    lbl.addEventListener("click", () => editButton(id));
    lbl.addEventListener("contextmenu", (e) => { e.preventDefault(); rowMenu(id); });
    fig.append(lbl);
  });
  if (extra.length > slots.length) fig.append(el("div", { class: "fig-more muted small", text: t("btn.more_in_list", { n: extra.length - slots.length }) }));
  svg.querySelectorAll(".mouse-part[data-id]").forEach((p) => p.addEventListener("click", () => { const id = +p.dataset.id; if (bindingOf(id)) editButton(id); }));
  fig.prepend(svg);
}

/** Side view of the mouse with the keypad of a plate; numbered keys left and right like the original. */
function drawSide(plate) {
  const fig = $("#fig");
  fig.innerHTML = "";
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 860 420");
  // original side silhouette, thumb side towards the viewer
  let keys = "";
  const ids = plate === 12 ? range(0x40, 12) : plate === 6 ? range(0x50, 6) : [0x05, 0x04];
  // outlines: ui/naga-side.js (source units, placed with one transform)
  const art = NAGA_SIDE[plate], k = NAGA_SIDE.scale;
  const geo = {};
  const at = ([x, y]) => [NAGA_SIDE.dx + x * k, NAGA_SIDE.dy + y * k];
  ids.forEach((id, i) => {
    const a = bindingOf(id), def = S.defaults && S.defaults[id];
    const cls = `side-key${a && def && !sameAction(a, def) ? " mod" : ""}${isHsMod(id) ? " hsmod" : ""}`;
    if (plate === 2) {
      const key = art.keys[i];
      keys += `<path class="${cls}" data-id="${id}" d="${key.d}"/><circle class="side-key-dot" cx="${key.dot[0]}" cy="${key.dot[1]}" r="3.5"/>`;
      geo[id] = at(key.dot);
    } else {
      const [x, y, w, h, r] = art.keys[i];
      keys += `<rect class="${cls}" data-id="${id}" x="${x}" y="${y}" width="${w}" height="${h}" rx="${r}"/>`;
      keys += `<text class="side-key-n big" x="${x + w / 2}" y="${y + h / 2 + 1}">${i + 1}</text>`;
      geo[id] = at([x + w / 2, y + h / 2]);
    }
  });
  svg.innerHTML = MOUSE_ART.defs + `<defs>${NAGA_SIDE.defs}</defs><g transform="translate(${NAGA_SIDE.dx} ${NAGA_SIDE.dy}) scale(${k})">${art.shell}${keys}${art.marks || ""}</g>`;
  fig.append(svg);
  const label = (id, x, y, side) => {
    const a = bindingOf(id);
    if (!a) return;
    const def = S.defaults && S.defaults[id];
    const lbl = el("div", { class: `lbl ${side}${def && !sameAction(a, def) ? " mod" : ""}${a.kind === "disabled" ? " off" : ""}${isHsMod(id) ? " hsmod" : ""}`, title: `${buttonName(id)} (${hex2(id)})` }, bindText(id, a));
    lbl.style.top = y + "px";
    if (side === "l") lbl.style.right = (860 - x) + "px"; else lbl.style.left = x + "px";
    hookKey(lbl, id, svg);
    fig.append(lbl);
  };
  const circle = (id, n, x, y) => {
    if (!bindingOf(id)) return;
    const a = bindingOf(id), def = S.defaults && S.defaults[id];
    const c = el("div", { class: "knum" + (def && !sameAction(a, def) ? " mod" : "") + (isHsMod(id) ? " hsmod" : ""), text: String(n), title: `${buttonName(id)} (${hex2(id)})` });
    c.style.left = x + "px"; c.style.top = y + "px";
    hookKey(c, id, svg);
    fig.append(c);
  };
  if (plate === 12 || plate === 6) {
    // 12: 1..6 on the left, 7..12 on the right; 6: 6,5,4 on the left, 1,2,3 on the right
    const left = plate === 12 ? ids.slice(0, 6) : [ids[5], ids[4], ids[3]];
    const right = plate === 12 ? ids.slice(6) : ids.slice(0, 3);
    const rows = plate === 12 ? 6 : 3, y0 = plate === 12 ? 62 : 90, dy = plate === 12 ? 56 : 62;
    left.forEach((id, i) => { const y = y0 + i * dy; circle(id, ids.indexOf(id) + 1, 260, y); label(id, 226, y, "l"); });
    right.forEach((id, i) => { const y = y0 + i * dy; circle(id, ids.indexOf(id) + 1, 600, y); label(id, 634, y, "r"); });
    void rows;
  } else {
    // 2-button plate: leader lines like the top view
    [[0x05, 150], [0x04, 230]].forEach(([id, y]) => {
      const [cx, cy] = geo[id];
      const line = document.createElementNS(ns, "polyline");
      line.setAttribute("class", "lead");
      line.setAttribute("points", `${cx},${cy} ${560},${y} ${600},${y}`);
      const dot = document.createElementNS(ns, "circle");
      dot.setAttribute("class", "lead-dot"); dot.setAttribute("cx", cx); dot.setAttribute("cy", cy); dot.setAttribute("r", 2.2);
      svg.append(line, dot);
      label(id, 608, y, "r");
    });
  }
  svg.querySelectorAll(".side-key").forEach((k) => k.addEventListener("click", () => { if (bindingOf(+k.dataset.id)) editButton(+k.dataset.id); }));
  if (!ids.some((id) => bindingOf(id))) {
    // the mouse reports only the buttons of the attached plate
    fig.append(el("div", { class: "fig-empty", text: t("btn.view_absent", { n: plate }) }));
  }
}

/** Click = edit, right click = menu, hover highlights the key on the drawing. */
function hookKey(node, id, svg) {
  node.addEventListener("click", () => editButton(id));
  node.addEventListener("contextmenu", (e) => { e.preventDefault(); rowMenu(id); });
  node.addEventListener("mouseenter", () => svg.querySelectorAll(`[data-id="${id}"]`).forEach((p) => p.classList.add("hl")));
  node.addEventListener("mouseleave", () => svg.querySelectorAll(`[data-id="${id}"]`).forEach((p) => p.classList.remove("hl")));
}

const plateOf = (id) => (id >= 0x40 && id <= 0x4b ? 12 : id >= 0x50 && id <= 0x55 ? 6 : id === 4 || id === 5 ? 2 : null);
/** Plate whose buttons are shown: the chosen one, or the detected one in Auto. */
function activePlate() {
  if (["2", "6", "12"].includes(String(S.figView))) return +S.figView;
  const c = S.plateChoice || "auto";
  if (c === "auto") return S.plate || 12;
  if (c === "all") return 0;
  return +c;
}

function drawPad() {
  const box = $("#sidepad");
  const plate = activePlate();
  const ids = plate === 12 ? range(0x40, 12) : plate === 6 ? range(0x50, 6) : plate === 2 ? [0x05, 0x04] : [];
  const shown = ids.filter((id) => bindingOf(id));
  box.classList.add("hidden"); // side buttons live on the side views now
  $("#sidepad-title").textContent = t("btn.pad_title", { n: plate });
  const pad = $("#pad");
  pad.className = "pad p" + plate;
  pad.innerHTML = "";
  for (const id of shown) {
    const a = bindingOf(id), def = S.defaults && S.defaults[id];
    const k = el("button", { class: "key" + (def && !sameAction(a, def) ? " mod" : ""), title: hex2(id) },
      el("b", { text: buttonName(id) }), el("span", { text: describe(a) }));
    k.addEventListener("click", () => editButton(id));
    k.addEventListener("contextmenu", (e) => { e.preventDefault(); rowMenu(id); });
    pad.append(k);
  }
}
const range = (a, n) => Array.from({ length: n }, (_, i) => a + i);

function drawList() {
  const tb = $("#btn-table tbody");
  tb.innerHTML = "";
  const plate = activePlate();
  const ids = Object.keys(S.profile ? S.profile.buttons : {}).map((k) => parseInt(k, 16)).sort((a, b) => a - b)
    .filter((id) => !S.conn.side_plates || !plateOf(id) || !plate || plateOf(id) === plate)
    .filter((id) => $("#btn-all").checked || S.hs || bindingOf(id).kind !== "disabled" || buttonName(id) !== t("name.button", { id: hex2(id) }));
  for (const id of ids) {
    const tr = el("tr", { "data-id": id, class: bindingOf(id).kind === "disabled" && S.hs ? "off" : "" }, el("td", { text: hex2(id) }), el("td", { text: buttonName(id) }), el("td", { text: bindText(id, bindingOf(id), false) }));
    tr.addEventListener("dblclick", () => editButton(id));
    tr.addEventListener("click", () => { $$("#btn-table tbody tr").forEach((x) => x.classList.toggle("sel", x === tr)); });
    tr.addEventListener("contextmenu", (e) => { e.preventDefault(); rowMenu(id); });
    tb.append(tr);
  }
}

function drawPlates() {
  const keypad = hasKeypad();
  $("#plate-box").classList.toggle("hidden", !S.conn || (!S.conn.side_plates && !keypad));
  const view = String(S.figView || "top");
  $$("#plate-box .plate").forEach((b) => {
    b.classList.toggle("hidden", keypad && !["top", "12"].includes(b.dataset.view));
    b.classList.toggle("on", b.dataset.view === view);
    b.classList.toggle("det", b.dataset.view === String(S.plate));
    b.title = b.dataset.view === "top" ? t("btn.view_top") : t("btn.plate_" + b.dataset.view) + (b.dataset.view === String(S.plate) ? " — " + t("btn.view_attached") : "");
  });
}

function drawCustomize() {
  if (!S.conn || !S.profile) {
    $("#fig").innerHTML = ""; $("#pad").innerHTML = ""; $("#btn-table tbody").innerHTML = "";
    return;
  }
  $$("#layer-seg button").forEach((b) => b.classList.toggle("on", +b.dataset.hs === (S.hs ? 1 : 0)));
  $("#cust-figure").classList.toggle("hidden", S.view === "list");
  $("#cust-list").classList.toggle("hidden", S.view !== "list");
  drawPlates();
  drawFigure();
  drawPad();
  drawList();
  drawHypershift();
}

/** Hypershift: who holds it, warnings when the layer cannot be reached or is empty. */
function drawHypershift() {
  const ids = Object.keys(S.profile.buttons).map((k) => parseInt(k, 16));
  const mods = ids.filter(isHsMod);
  const own = ids.filter((id) => !isHsMod(id)).some((id) => {
    const p = pairOf(id);
    return p.hypershift && p.hypershift.kind !== "disabled";
  });
  const chip = $("#hs-chip");
  chip.classList.toggle("hidden", !mods.length);
  chip.textContent = "⇧ " + t("hs.held_by", { b: mods.map(buttonName).join(", ") });
  $("#btn-layer").classList.toggle("hidden", !S.hs);
  const box = $("#hs-notice"), btns = $("#hs-notice-btns");
  btns.innerHTML = "";
  let text = "";
  // bindings someone set up for Hypershift (not just the factory copy of the standard layer)
  const custom = ids.filter((id) => !isHsMod(id)).some((id) => {
    const p = pairOf(id);
    return p.hypershift && p.hypershift.kind !== "disabled" && !sameAction(p.hypershift, p.normal);
  });
  if (!mods.length && (S.hs || custom)) {
    text = t(S.hs ? "hs.no_mod" : "hs.unreachable");
    btns.append(el("button", { class: "primary", text: t("hs.make_btn"), onclick: makeHsDialog }));
  } else if (mods.length && S.hs && !own) {
    text = t("hs.empty");
    btns.append(el("button", { text: t("hs.copy"), onclick: () => layerOp("copy") }));
  }
  box.classList.toggle("hidden", !text);
  $("#hs-notice-text").textContent = text;
}

/** Choose a button to hold Hypershift. */
async function makeHsDialog() {
  if (!needMouse()) return;
  const ids = Object.keys(S.profile.buttons).map((k) => parseInt(k, 16)).filter((id) => !isHsMod(id) && id !== 1 && id !== 2)
    .filter((id) => !S.conn.side_plates || !plateOf(id) || plateOf(id) === activePlate());
  const id = await modal((box, close) => {
    const sel = el("select");
    // a side key suits best (the thumb holds it); else the last button
    const side = ids.filter((i) => plateOf(i) && plateOf(i) !== 2);
    fillSelect(sel, ids.map((i) => [i, `${buttonName(i)} — ${describe(pairOf(i).normal)}`]), side.length ? side[side.length - 1] : ids[ids.length - 1]);
    box.append(el("h2", { text: t("hs.make_title") }), el("div", { class: "muted", text: t("hs.make_text") }),
      el("div", { class: "form" }, el("label", { text: t("hs.make_label") }), sel),
      el("div", { class: "buttons" }, el("button", { text: t("common.cancel"), onclick: () => close(null) }),
        el("button", { class: "primary", text: t("hs.make_btn"), onclick: () => close(+sel.value) })));
  });
  if (id != null) await makeHs(id);
}
const makeHs = (id) => setButton(id, { ...defaultAction(), kind: "hypershift" }, false);

/** "copy" = standard layer → Hypershift; "clear" = every Hypershift binding off. */
async function layerOp(op) {
  if (!needMouse()) return;
  if (op === "clear" && !(await confirmBox(t("hs.clear"), t("hs.clear_text"), t("hs.clear_ok")))) return;
  const p = curProfile();
  const n = await run("profile_layer", { id: p.id, op }, (n) => t(op === "copy" ? "hs.copied" : "hs.cleared", { n }));
  void n;
  S.profile = await run("profile_get", { id: p.id });
  markChanged();
  drawCustomize();
}

function layerMenu(anchor) {
  popover(anchor, (box, close) => {
    const it = (key, fn) => box.append(el("button", { class: "mi", text: t(key), onclick: () => { close(); fn(); } }));
    it("hs.copy", () => layerOp("copy"));
    it("hs.clear", () => layerOp("clear"));
    box.append(el("hr"));
    it("hs.make_btn", makeHsDialog);
  });
}

async function checkPlate() {
  if (!S.conn || !S.conn.side_plates || S.busy || (S.plateChoice || "auto") !== "auto") return;
  if ($("#page-buttons").classList.contains("hidden")) return;
  const p = await quiet(api.call("side_plate"));
  if (p === undefined || p === S.plate) return;
  S.plate = p;
  ulog(`side plate changed: ${p}`);
  status(t("status.plate", { p: plateText(p) }));
  drawCustomize();
}

async function rowMenu(id) {
  const choice = await modal((box, close) => {
    box.append(el("h2", { text: `${buttonName(id)} (${hex2(id)})` }),
      el("div", { class: "buttons" },
        el("button", { text: t("btn.menu_edit"), onclick: () => close("edit") }),
        el("button", { text: t("btn.menu_rename"), onclick: () => close("rename") }),
        el("button", { text: t("btn.menu_default"), onclick: () => close("default") }),
        isHsMod(id) ? null : el("button", { text: t("hs.make_this"), onclick: () => close("hs") }),
        el("button", { class: "danger", text: t("btn.menu_off"), onclick: () => close("off") }),
        el("button", { text: t("common.cancel"), onclick: () => close(null) })));
  });
  if (choice === "edit") editButton(id);
  if (choice === "rename") {
    const name = await promptBox(t("btn.rename_title"), t("btn.rename_label"), buttonName(id));
    if (name) {
      await run("rename_button", { id, name }, t("btn.renamed"));
      S.names[id.toString(16).padStart(2, "0")] = name;
      drawCustomize();
    }
  }
  if (choice === "default") await resetDefault(id);
  if (choice === "hs") await makeHs(id);
  if (choice === "off") await setButton(id, { ...defaultAction(), kind: "disabled" });
}

async function resetDefault(id) {
  const act = (S.defaults && S.defaults[id]) || (await api.call("default_action", { id }));
  if (!act) return status(t("btn.no_default"), true);
  await setButton(id, act);
}

function defaultAction() {
  return { kind: "disabled", button: 1, modifiers: 0, key: 0, interval: 100, code: 0, macro_id: 0,
    macro_mode: "once", repeat: 1, stage: 1, dpi_x: 400, dpi_y: 400, profile: 1, raw: [] };
}

async function setButton(id, action, hs = !!S.hs) {
  const p = curProfile();
  const wasMod = isHsMod(id);
  const back = await run("profile_set_button", { id: p.id, button: id, hypershift: hs, action });
  const k = hex2(id).toLowerCase();
  const pair = S.profile.buttons[k] || (S.profile.buttons[k] = { normal: back, hypershift: null });
  if (hs) pair.hypershift = back; else pair.normal = back;
  // Rust keeps the modifier's own Hypershift binding in step (see profile_set_button)
  markChanged();
  if (!hs && back.kind === "hypershift") pair.hypershift = back;
  else if (!hs && wasMod) pair.hypershift = { ...defaultAction(), kind: "disabled" };
  status(t("btn.written", { name: buttonName(id), desc: describe(back), where: p.slots.length ? t("btn.where_profile", { n: p.slots.join(", ") }) : t("btn.where_temp") }));
  drawCustomize();
}

async function editButton(id) {
  if (!needMouse()) return;
  if (S.hs && isHsMod(id)) return status(t("hs.locked", { b: buttonName(id) }), true);
  ulog(`binding editor ${hex2(id)}`, "DEBUG");
  let act = await bindingDialog({ id, action: bindingOf(id) || defaultAction() });
  if (act === "default") return resetDefault(id);
  if (act && act.turboMedia) {
    const mid = await run("turbo_media", { code: act.code, interval: act.interval });
    act = { ...defaultAction(), kind: "macro", macro_id: mid, macro_mode: "hold" };
    await quiet(refreshMacros()); // the service macro is now in the mouse
  }
  if (act) await setButton(id, act);
}

function bindingDialog(b) {
  const T = S.tables, a = { ...defaultAction(), ...b.action };
  // a media key with turbo is stored as a service macro: show it as what it is
  const svc = a.kind === "macro" ? serviceTurbo(a.macro_id) : null;
  if (svc) Object.assign(a, { kind: "consumer", code: svc.code, interval: svc.interval, svcTurbo: true });
  const turboOn = ["keyboard_turbo", "mouse_turbo"].includes(a.kind) || !!a.svcTurbo || (a.kind === "macro" && a.macro_mode === "hold");
  return modal((box, close) => {
    const kindSel = el("select");
    // the Hypershift layer cannot hold Hypershift itself
    const shown = ["default", "keyboard", "mouse", "macro", "dpi", "profile", "consumer", "hypershift", "system", "scroll_mode", "disabled", "raw"]
      .filter((k) => !(S.hs && k === "hypershift"));
    const groupKind = { keyboard_turbo: "keyboard", double_click: "mouse", mouse_turbo: "mouse" };
    fillSelect(kindSel, shown.map((k) => [k, t("kind." + k)]), groupKind[a.kind] || a.kind);
    const form = el("div", { class: "form" });
    const f = {};

    // keyboard: physical keys (layout independent)
    const cap = el("div", { class: "capture", tabindex: 0 });
    let mods = a.modifiers, key = a.key, held = 0;
    if (!["keyboard", "keyboard_turbo"].includes(a.kind)) { mods = 0; key = 0; }
    const showCombo = () => { cap.textContent = comboName(mods, key) || t("bind.combo_ph"); };
    showCombo();
    cap.addEventListener("keydown", (e) => {
      e.preventDefault(); e.stopPropagation();
      const c = HID.fromEvent(e);
      if (c == null) return;
      if (HID.isModifier(c)) { held |= HID.modBit(c); mods = held; key = 0; } else { mods = held; key = c; }
      showCombo();
    });
    cap.addEventListener("keyup", (e) => { const c = HID.fromEvent(e); if (c != null && HID.isModifier(c)) held &= ~HID.modBit(c); });
    cap.addEventListener("focus", () => { box.dataset.capture = "1"; });
    cap.addEventListener("blur", () => { delete box.dataset.capture; held = 0; });
    // keys a keyboard may not have (F13–F24 are free for programs and macro triggers)
    f.keyPick = el("select");
    fillSelect(f.keyPick, [["", t("bind.key_list")], ...S.tables.keys.map(([c, n]) => [c, n])], "");
    f.keyPick.addEventListener("change", () => { if (f.keyPick.value) { key = +f.keyPick.value; showCombo(); } });
    // Turbo / repeat: one switch for every action that can repeat while the button is held
    f.turbo = el("input", { type: "checkbox" });
    f.turbo.checked = turboOn;
    f.iv = el("input", { type: "number", class: "num", min: 10, max: 5000, value: turboOn && a.interval ? a.interval : 50 });
    const ivRow = el("span", { class: "turbo-iv" }, f.iv, el("span", { class: "muted", text: " " + t("mac.ms") }));
    const turboHint = el("span", { class: "muted small turbo-hint" });
    // mouse
    f.mbtn = el("select"); fillSelect(f.mbtn, T.mouse_fn.map(([c, n]) => [c, tl("mouse", c, n)]), a.button);
    f.mmode = el("select"); fillSelect(f.mmode, [["mouse", t("bind.mode_click")], ["double_click", t("bind.mode_double")]],
      a.kind === "double_click" ? "double_click" : "mouse");
    // macro
    const macroItems = macroChoices();
    if (a.kind === "macro" && !macroItems.some(([id]) => id === a.macro_id)) macroItems.push([a.macro_id, "#" + a.macro_id]);
    f.mid = el("select"); fillSelect(f.mid, macroItems.length ? macroItems : [["", t("bind.macro_none")]], a.macro_id);
    f.mm = el("select"); fillSelect(f.mm, T.macro_modes.map(([k]) => [k, t("macro_mode." + k)]), a.macro_mode);
    f.rep = el("input", { type: "number", class: "num", min: 1, max: 255, value: a.repeat });
    // dpi / profile / consumer / system / raw
    f.dop = el("select"); fillSelect(f.dop, T.dpi_ops.map(([c, n]) => [c, tl("dpi_op", c, n)]), a.kind === "dpi" ? a.code : 6);
    f.dstage = el("input", { type: "number", class: "num", min: 1, max: 5, value: a.stage });
    f.daim = el("input", { type: "number", class: "num", min: 100, max: 20000, step: 50, value: a.dpi_x });
    f.pop = el("select"); fillSelect(f.pop, T.profile_ops.map(([c, n]) => [c, tl("profile_op", c, n)]), a.kind === "profile" ? a.code : 4);
    f.pn = el("input", { type: "number", class: "num", min: 1, max: 5, value: a.profile });
    f.cons = el("select"); fillSelect(f.cons, T.consumer.map(([c, n]) => [c, tl("consumer", c, n)]), a.kind === "consumer" ? a.code : 0xcd);
    f.sys = el("select"); fillSelect(f.sys, T.system_ops.map(([c, n]) => [c, tl("system_op", c, n)]), a.kind === "system" ? a.code : 2);
    f.raw = el("input", { class: "mono", value: a.raw && a.raw.length ? a.raw.map((x) => x.toString(16).padStart(2, "0")).join(" ") : "" });
    const note = (k) => [["", el("span", { class: "muted", text: t(k) })]];

    const pages = {
      default: note("bind.default_text"),
      keyboard: [[t("bind.combo"), cap], [t("bind.key_other"), f.keyPick]],
      mouse: [[t("bind.button"), f.mbtn], [t("bind.mode"), f.mmode]],
      macro: [[t("bind.macro"), f.mid], [t("bind.run"), f.mm], [t("bind.repeat"), f.rep]],
      dpi: [[t("bind.action"), f.dop], [t("bind.stage"), f.dstage], [t("bind.sniper"), f.daim]],
      profile: [[t("bind.action"), f.pop], [t("bind.profile"), f.pn]],
      consumer: [[t("bind.key"), f.cons]],
      system: [[t("bind.action"), f.sys]],
      raw: [[t("bind.raw"), f.raw]],
      hypershift: note("bind.hs_text"), scroll_mode: note("bind.scroll_text"), disabled: note("bind.off_text"),
    };
    const TURBO = ["keyboard", "mouse", "macro", "consumer"];
    const turboUi = () => {
      const k = kindSel.value, on = f.turbo.checked;
      ivRow.classList.toggle("hidden", !on || k === "macro");
      f.mmode.disabled = k === "mouse" && on;
      f.mm.disabled = k === "macro" && on;
      if (k === "macro" && on) f.mm.value = "hold";
      turboHint.textContent = !on ? "" : k === "macro" ? t("bind.turbo_macro") : k === "consumer" ? t("bind.turbo_media") : t("bind.turbo_hw");
    };
    f.turbo.addEventListener("change", turboUi);
    const draw = () => {
      form.innerHTML = "";
      for (const [l, w] of pages[kindSel.value] || []) form.append(el("label", { text: l }), w);
      if (TURBO.includes(kindSel.value)) {
        form.append(el("label", { text: t("bind.turbo_title") }),
          el("div", { class: "turbo-row" }, el("label", { class: "check" }, f.turbo, " " + t("bind.turbo_label")), ivRow));
        form.append(el("span"), turboHint);
      }
      turboUi();
    };
    kindSel.addEventListener("change", draw);
    draw();

    const err = el("div", { class: "muted" });
    const iv = () => Math.max(10, Math.min(5000, Math.round(+f.iv.value || 50)));
    const result = () => {
      const k = kindSel.value, r = defaultAction();
      switch (k) {
        case "default": return "default";
        case "keyboard":
          if (!key && !mods) throw t("bind.need_key");
          return { ...r, kind: f.turbo.checked ? "keyboard_turbo" : "keyboard", modifiers: mods, key, interval: iv() };
        case "mouse": return { ...r, kind: f.turbo.checked ? "mouse_turbo" : f.mmode.value, button: +f.mbtn.value, interval: iv() };
        case "macro":
          if (!f.mid.value) throw t("bind.need_macro");
          return { ...r, kind: "macro", macro_id: +f.mid.value, macro_mode: f.turbo.checked ? "hold" : f.mm.value, repeat: +f.rep.value };
        case "dpi": return { ...r, kind: "dpi", code: +f.dop.value, stage: +f.dstage.value, dpi_x: +f.daim.value, dpi_y: +f.daim.value };
        case "profile": return { ...r, kind: "profile", code: +f.pop.value, profile: +f.pn.value };
        case "consumer":
          // no turbo for media keys in the mouse: a service macro does it (see turbo_media)
          if (f.turbo.checked) return { turboMedia: true, code: +f.cons.value, interval: iv() };
          return { ...r, kind: "consumer", code: +f.cons.value };
        case "system": return { ...r, kind: "system", code: +f.sys.value };
        case "raw": {
          const bytes = f.raw.value.trim().split(/\s+/).filter(Boolean).map((x) => parseInt(x, 16));
          if (!bytes.length || bytes.some((x) => isNaN(x) || x > 255)) throw t("bind.bad_raw");
          return { ...r, kind: "raw", raw: bytes.slice(0, 7) };
        }
        default: return { ...r, kind: k };
      }
    };
    box.append(
      el("h2", { text: t("bind.title", { name: buttonName(b.id), id: hex2(b.id) }) }),
      el("div", { class: "muted", text: t("bind.now", { desc: describe(b.action) }) }),
      kindSel, form, err,
      el("div", { class: "buttons" },
        el("button", { text: t("common.cancel"), onclick: () => close(null) }),
        el("button", { class: "primary", text: t("common.apply"), onclick: () => {
          try { close(result()); } catch (e) { err.textContent = String(e); err.style.color = "#ff8a8a"; }
        } })));
  });
}

async function detectButton() {
  if (!S.profile) return status(t("btn.scan_first"), true);
  const hit = await modal((box, close) => {
    const pad = el("div", { class: "capture big", tabindex: 0, autofocus: true, text: t("btn.detect_pad") });
    box.dataset.capture = "1";
    pad.addEventListener("keydown", (e) => { e.preventDefault(); const c = HID.fromEvent(e); if (c != null) close({ key: c }); });
    pad.addEventListener("mousedown", (e) => { if (e.button !== 0) { e.preventDefault(); close({ mouse: HID.mouseBinding[e.button] }); } });
    pad.addEventListener("contextmenu", (e) => e.preventDefault());
    box.append(el("h2", { text: t("btn.detect_title") }), el("div", { class: "muted", text: t("btn.detect_text") }), pad,
      el("div", { class: "buttons" }, el("button", { text: t("common.cancel"), onclick: () => close(null) })));
  });
  if (!hit) return;
  const ids = Object.keys(S.profile.buttons).map((k) => parseInt(k, 16));
  const id = ids.find((i) => { const a = bindingOf(i); return a && ((hit.key != null && ["keyboard", "keyboard_turbo"].includes(a.kind) && a.key === hit.key)
    || (hit.mouse != null && a.kind === "mouse" && a.button === hit.mouse)); });
  if (id == null) return status(t("btn.detect_none"), true);
  // highlight it in the figure / pad / list
  $$("#fig .lbl, #pad .key").forEach((e) => e.classList.remove("flash"));
  const row = $(`#btn-table tbody tr[data-id="${id}"]`);
  if (row) row.classList.add("sel");
  status(t("btn.detect_found", { name: buttonName(id), id: hex2(id) }));
  if (await confirmBox(t("btn.detect_title"), t("btn.detect_found", { name: buttonName(id), id: hex2(id) }), t("btn.menu_edit"))) editButton(id);
}

async function reloadAfterRestore() {
  const p = curProfile();
  if (p.slots.length) S.profile = await run("profile_reload", { id: p.id });
  drawCustomize(); showProfileDpi();
}

async function doBackup() {
  if (!needMouse()) return;
  status(t("btn.backup_wait"));
  await run("backup", {}, (p) => t("btn.backup_saved", { path: p }));
}

async function doRestore() {
  if (!needMouse()) return;
  const files = await run("list_backups");
  const path = await modal((box, close) => {
    const list = el("ul", { class: "pick" });
    let sel = null;
    if (!files.length) list.append(el("li", { class: "muted", text: t("btn.restore_none") }));
    files.forEach((f) => {
      const li = el("li", { text: `${f.name}  (${Math.round(f.size / 1024)} KiB)` });
      li.addEventListener("click", () => { sel = f.path; $$("li", list).forEach((x) => x.classList.toggle("sel", x === li)); });
      list.append(li);
    });
    box.append(el("h2", { text: t("btn.restore_title") }), list, el("div", { class: "muted small", text: t("btn.restore_warn") }),
      el("div", { class: "buttons" },
        el("button", { text: t("btn.open_folder"), onclick: () => api.call("open_backup_folder") }),
        el("button", { text: t("common.cancel"), onclick: () => close(null) }),
        el("button", { class: "primary", text: t("btn.restore_go"), onclick: () => sel && close(sel) })));
  });
  if (!path) return;
  await run("restore", { path }, (n) => t("btn.restored", { n }));
  if (curProfile()) await reloadAfterRestore();
}

// ------------------------------------------------------------------ macros
// Editor document: `items` = hardware steps (compiled into the mouse by Rust), `soft` = program
// actions (performed by open-rzr when the mouse presses the macro's F13–F24 trigger key).
const M = { items: [], soft: [], selHw: new Set(), selSw: new Set(), undo: [], redo: [], dirty: false, check: null, status: null };
const MAX_MACRO_BYTES = 4096;
const item = (kind, value = 0, extra = 0, text = "") => ({ kind, value, extra, text, on: true });
const soft = (kind, text = "", value = 0) => ({ kind, value, text, on: true });

function macroChoices() {
  const names = {};
  S.lib.forEach((m) => { names[m.id] = m.name; });
  return [...new Set(S.onMouse.map((m) => m.id))].filter((id) => id < SERVICE_MACRO).sort((a, b) => a - b).map((id) => [id, `#${id} ${names[id] || ""}`]);
}

async function refreshMacros() {
  S.lib = (await quiet(api.call("macro_library"))) || [];
  drawLib();
  M.status = (await quiet(api.call("program_actions_status"))) || null;
  drawSoftStatus();
  if (!S.conn) return;
  const r = await run("macros_on_mouse");
  S.onMouse = r.macros;
  drawMouseMacros(r.flash);
}

function drawMouseMacros(flash) {
  const ul = $("#mouse-macros");
  ul.innerHTML = "";
  const names = {};
  S.lib.forEach((m) => { names[m.id] = m.name; });
  S.onMouse.forEach((m) => {
    const li = el("li", { text: `#${m.id}  ${names[m.id] || ""} · ${t("mac.steps", { n: m.steps.length })}`, class: S.mouseSel === m.id ? "sel" : "" });
    li.addEventListener("click", async () => {
      if (!(await leaveEditor())) return;
      S.mouseSel = m.id; S.libSel = null;
      $$("li", ul).forEach((x) => x.classList.toggle("sel", x === li)); drawLib();
      const own = S.lib.find((x) => x.id === m.id);
      $("#m-id").value = m.id; $("#m-name").value = names[m.id] || t("mac.default_name", { id: m.id });
      // the library copy has loops, text and program actions; the mouse only has the compiled steps
      loadDoc(own ? own.items : m.steps.map((s) => item(s.kind, s.value)), own ? own.software : []);
    });
    ul.append(li);
  });
  if (!S.onMouse.length) ul.append(el("li", { class: "muted", text: t("common.empty") }));
  if (flash !== undefined) S.flash = flash;
  $("#flash").textContent = S.flash ? t("mac.flash", { free: Math.round(S.flash[1] / 1024), total: Math.round(S.flash[0] / 1024) }) : "";
}

function drawLib() {
  const ul = $("#lib-list");
  ul.innerHTML = "";
  S.lib.forEach((m) => {
    const li = el("li", { class: S.libSel === m.name ? "sel" : "", title: m.trigger ? t("mac.lib_sw", { k: m.trigger }) : "" },
      el("span", { class: "grow", text: m.name }),
      m.software && m.software.length ? el("span", { class: "badge sw", text: "⚡" }) : null,
      el("span", { class: "muted small", text: "#" + m.id }));
    li.addEventListener("click", async () => {
      if (S.libSel === m.name || !(await leaveEditor())) return;
      S.libSel = m.name; S.mouseSel = null; drawLib();
      $$("#mouse-macros li").forEach((x) => x.classList.remove("sel"));
      $("#m-name").value = m.name; $("#m-id").value = m.id;
      loadDoc(m.items, m.software);
    });
    li.addEventListener("contextmenu", (e) => { e.preventDefault(); S.libSel = m.name; drawLib(); libMenu(li); });
    ul.append(li);
  });
  if (!S.lib.length) ul.append(el("li", { class: "muted", text: t("mac.lib_empty") }));
}

/** Unsaved changes: ask before throwing them away. */
async function leaveEditor() {
  if (!M.dirty) return true;
  return confirmBox(t("mac.discard_title"), t("mac.discard_text"), t("mac.discard"));
}

function loadDoc(items, software) {
  M.items = (items || []).map((s) => ({ ...item(s.kind), ...s, on: s.on !== false }));
  M.soft = (software || []).map((s) => ({ ...soft(s.kind), ...s, on: s.on !== false }));
  M.selHw.clear(); M.selSw.clear(); M.undo = []; M.redo = [];
  setDirty(false);
  drawSteps();
}

function setDirty(d) { M.dirty = d; $("#m-dirty").classList.toggle("hidden", !d); }

// ---- undo / redo
const snap = () => JSON.stringify({ items: M.items, soft: M.soft });
function pushUndo() { M.undo.push(snap()); if (M.undo.length > 200) M.undo.shift(); M.redo = []; }
function restore(s) { const d = JSON.parse(s); M.items = d.items; M.soft = d.soft; M.selHw.clear(); M.selSw.clear(); changed(); }
function undo() { if (!M.undo.length) return; M.redo.push(snap()); restore(M.undo.pop()); }
function redo() { if (!M.redo.length) return; M.undo.push(snap()); restore(M.redo.pop()); }
/** Change the document: snapshot for undo, apply, redraw, re-check. */
function edit(fn, redraw = true) { pushUndo(); fn(); changed(redraw); }
function changed(redraw = true) { setDirty(true); if (redraw) drawSteps(); else scheduleCheck(); }

// ---- step texts and pickers
const MOUSE_BITS = [1, 2, 4, 8, 16];
const isAscii = (s) => /^[\x20-\x7E\n\t\r]*$/.test(s);
const consumerList = () => S.tables.consumer.map(([c, n]) => [c, tl("consumer", c, n)]);

function stepIcon(kind) {
  return { delay: "⏱", key_press: "⌨", key_down: "⌨", key_up: "⌨", mouse_click: "🖱", mouse_down: "🖱", mouse_up: "🖱", mouse: "🖱",
    wheel: "↕", media: "♫", consumer: "♫", loop_start: "↻", loop_end: "↻", macro: "▤", text: "T", system: "⏻",
    launch: "▶", run: ">_", type: "✎" }[kind] || "•";
}

function numInput(value, min, max, onset, cls = "num") {
  const i = el("input", { type: "number", class: cls, value, min, max });
  i.addEventListener("change", () => { const v = Math.max(min, Math.min(max, Math.round(+i.value || 0))); edit(() => onset(v), false); i.value = v; });
  return i;
}
function selInput(items, value, onset) {
  const s = el("select");
  fillSelect(s, items, value);
  s.addEventListener("change", () => edit(() => onset(s.value)));
  return s;
}
function textInput(value, ph, onset, cls = "grow") {
  const i = el("input", { class: cls, value, placeholder: ph });
  i.addEventListener("change", () => edit(() => onset(i.value), false));
  return i;
}

/** "Press a key" dialog with a list as a fallback (keys like Pause, F13–F24, numpad). */
function keyDialog(cur) {
  return modal((box, close) => {
    box.dataset.capture = "1";
    const pick = el("select");
    fillSelect(pick, S.tables.keys, cur || 0x04);
    const zone = el("div", { class: "capture", tabindex: "0", text: t("mac.key_press") });
    zone.addEventListener("keydown", (e) => {
      e.preventDefault(); e.stopPropagation();
      const c = HID.fromEvent(e);
      if (c != null) { pick.value = String(c); zone.textContent = keyName(c); }
    });
    box.append(el("h2", { text: t("mac.key_title") }), zone,
      el("div", { class: "form" }, el("label", { text: t("mac.key_list") }), pick),
      el("div", { class: "buttons" }, el("button", { text: t("common.cancel"), onclick: () => close(null) }),
        el("button", { class: "primary", text: t("common.ok"), onclick: () => close(+pick.value) })));
    setTimeout(() => zone.focus());
  });
}

/** Multi-line text popup (Synapse "text function"). `ascii`: warn about what the mouse cannot type. */
function textDialog(title, value, ascii) {
  return modal((box, close) => {
    const ta = el("textarea", { rows: 6, class: "grow" });
    ta.value = value || "";
    const warn = el("div", { class: "muted small" });
    const check = () => {
      const bad = ascii ? [...new Set([...ta.value].filter((c) => !isAscii(c)))].join("") : "";
      warn.textContent = bad ? t("mac.text_bad", { c: bad.slice(0, 12) }) : ascii ? t("mac.text_hw_note") : t("mac.text_sw_note");
      warn.classList.toggle("err", !!bad);
    };
    ta.addEventListener("input", check); check();
    box.append(el("h2", { text: title }), ta, warn,
      el("div", { class: "buttons" }, el("button", { text: t("common.cancel"), onclick: () => close(null) }),
        el("button", { class: "primary", text: t("common.ok"), onclick: () => close(ta.value) })));
    setTimeout(() => ta.focus());
  });
}

/** Pick a running program for "Launch". */
async function pickProgram(anchor, cb) {
  const list = (await quiet(api.call("running_programs"))) || [];
  popover(anchor, (box, close) => {
    if (!list.length) box.append(el("div", { class: "muted small pad", text: t("mac.no_programs") }));
    list.slice(0, 30).forEach((p) => box.append(el("button", { class: "mi", title: p.path, text: p.title ? `${p.name} — ${p.title}` : p.name, onclick: () => { close(); cb(p.path); } })));
  });
}

// ---- rows
function hwEditor(it) {
  const set = (k) => (v) => { it[k] = v; };
  const keyBtn = () => el("button", { class: "keycap", text: keyName(it.value), onclick: async () => {
    const k = await keyDialog(it.value);
    if (k != null) edit(() => { it.value = k; });
  } });
  const hold = () => [el("span", { class: "muted", text: t("mac.hold") }), numInput(it.extra || 20, 1, 60000, set("extra")), el("span", { class: "muted", text: t("mac.ms") })];
  switch (it.kind) {
    case "delay":
      return [el("b", { text: t("mac.s_delay") }), numInput(it.value, 1, 3600000, set("value")), el("span", { class: "muted", text: t("mac.ms") })];
    case "key_press": case "key_down": case "key_up":
      return [selInput([["key_press", t("mac.s_press")], ["key_down", t("mac.s_down")], ["key_up", t("mac.s_up")]], it.kind, set("kind")),
        keyBtn(), ...(it.kind === "key_press" ? hold() : [])];
    case "mouse_click": case "mouse_down": case "mouse_up":
      return [selInput([["mouse_click", t("mac.s_click")], ["mouse_down", t("mac.s_down")], ["mouse_up", t("mac.s_up")]], it.kind, set("kind")),
        selInput(MOUSE_BITS.map((b) => [b, t("mbit." + b)]), it.value, (v) => { it.value = +v; }), ...(it.kind === "mouse_click" ? hold() : [])];
    case "mouse":
      return [el("span", { text: it.value ? t("step.mouse", { b: MOUSE_BITS.filter((b) => it.value & b).map((b) => t("mbit." + b)).join("+") }) : t("step.mouse_up") })];
    case "wheel":
      return [el("b", { text: t("mac.s_wheel") }), selInput([[1, t("mac.up")], [-1, t("mac.down")]], it.value > 0 ? 1 : -1, (v) => { it.value = +v * Math.abs(it.value || 1); }),
        el("span", { class: "muted", text: "×" }), numInput(Math.abs(it.value) || 1, 1, 127, (v) => { it.value = Math.sign(it.value || 1) * v; })];
    case "media":
      return [el("b", { text: t("mac.s_media") }), selInput(consumerList(), it.value, (v) => { it.value = +v; })];
    case "consumer":
      return [el("span", { text: it.value ? t("desc.consumer", { c: tl("consumer", it.value, "0x" + it.value.toString(16)) }) : t("mac.s_media_up") })];
    case "loop_start":
      return [el("b", { text: t("mac.s_loop") }), numInput(it.extra || 2, 1, 1000, set("extra")), el("span", { class: "muted", text: t("mac.times") })];
    case "loop_end":
      return [el("b", { class: "muted", text: t("mac.s_loop_end") })];
    case "macro": {
      const others = S.lib.map((m) => m.name).filter((n) => n !== ($("#m-name").value || "").trim());
      return [el("b", { text: t("mac.s_macro") }), selInput(others.map((n) => [n, n]), it.text, set("text"))];
    }
    case "text": {
      const inp = textInput(it.text, t("mac.text_ph"), set("text"));
      if (!isAscii(it.text)) inp.classList.add("bad");
      return [el("b", { text: t("mac.s_text") }), inp,
        el("button", { class: "icon", title: t("mac.text_edit"), text: "✎", onclick: async () => {
          const v = await textDialog(t("mac.a_text"), it.text, true);
          if (v != null) edit(() => { it.text = v; });
        } })];
    }
    default:
      return [el("span", { text: `${it.kind} ${it.value}` })];
  }
}

function swEditor(a, row) {
  const set = (k) => (v) => { a[k] = v; };
  switch (a.kind) {
    case "launch": {
      const inp = textInput(a.text, t("mac.launch_ph"), set("text"));
      return [el("b", { text: t("mac.s_launch") }), inp,
        el("button", { class: "icon", title: t("mac.pick_program"), text: "⋯", onclick: (e) => pickProgram(e.currentTarget, (p) => edit(() => { a.text = p; })) })];
    }
    case "run":
      return [el("b", { text: t("mac.s_run") }), textInput(a.text, t("mac.run_ph"), set("text"))];
    case "type":
      return [el("b", { text: t("mac.s_type") }), textInput(a.text, t("mac.text_ph"), set("text")),
        el("button", { class: "icon", title: t("mac.text_edit"), text: "✎", onclick: async () => {
          const v = await textDialog(t("mac.a_type"), a.text, false);
          if (v != null) edit(() => { a.text = v; });
        } })];
    case "delay":
      return [el("b", { text: t("mac.s_delay") }), numInput(a.value, 1, 600000, set("value")), el("span", { class: "muted", text: t("mac.ms") })];
    default:
      void row;
      return [el("span", { text: `${a.kind} ${a.text}` })];
  }
}

function stepRow(list, i, depth) {
  const hw = list === "hw";
  const arr = hw ? M.items : M.soft, sel = hw ? M.selHw : M.selSw, it = arr[i];
  const li = el("li", { class: "step" + (sel.has(i) ? " sel" : "") + (it.on ? "" : " off") + (it.kind.startsWith("loop") ? " loop" : ""), "data-i": i });
  li.style.paddingLeft = 6 + depth * 18 + "px";
  const cb = el("input", { type: "checkbox", class: "pick-step" });
  cb.checked = sel.has(i);
  cb.addEventListener("click", (e) => { e.stopPropagation(); selectStep(list, i, e.shiftKey, true); });
  const handle = el("span", { class: "grip", title: t("mac.drag"), text: "⋮⋮" });
  // reorder with the pointer (HTML5 drag and drop is taken over by the WebView's file drop on Windows)
  handle.addEventListener("pointerdown", (e) => startDrag(e, list, i, li));
  li.addEventListener("click", (e) => { if (!e.target.closest("input,select,button,textarea")) selectStep(list, i, e.shiftKey, e.ctrlKey || e.metaKey); });
  const onoff = el("button", { class: "icon tog" + (it.on ? " on" : ""), title: it.on ? t("mac.step_off") : t("mac.step_on"), text: it.on ? "◉" : "○",
    onclick: () => edit(() => { it.on = !it.on; }) });
  const dup = el("button", { class: "icon hov", title: t("mac.dup"), text: "⧉", onclick: () => edit(() => { arr.splice(i + 1, 0, JSON.parse(JSON.stringify(it))); }) });
  const del = el("button", { class: "icon hov", title: t("mac.del_step"), text: "✕", onclick: () => edit(() => { arr.splice(i, 1); sel.clear(); }) });
  li.append(cb, handle, el("span", { class: "n", text: String(i + 1) }), el("span", { class: "ic", text: stepIcon(it.kind) }),
    el("span", { class: "ed" }, ...(hw ? hwEditor(it) : swEditor(it, li))), el("span", { class: "grow" }), dup, del, onoff);
  return li;
}

function drawSteps() {
  const ol = $("#steps");
  ol.innerHTML = "";
  let depth = 0;
  M.items.forEach((it, i) => {
    if (it.kind === "loop_end" && it.on) depth = Math.max(0, depth - 1);
    ol.append(stepRow("hw", i, depth));
    if (it.kind === "loop_start" && it.on) depth++;
  });
  if (!M.items.length) ol.append(el("li", { class: "empty muted", text: t("mac.empty_hint") }));
  const sw = $("#sw-steps");
  sw.innerHTML = "";
  M.soft.forEach((_, i) => sw.append(stepRow("sw", i, 0)));
  $("#sw-box").classList.toggle("hidden", !M.soft.length);
  $("#m-all").checked = M.items.length + M.soft.length > 0 && M.selHw.size + M.selSw.size === M.items.length + M.soft.length;
  $("#btn-undo").disabled = !M.undo.length;
  $("#btn-redo").disabled = !M.redo.length;
  const anySel = M.selHw.size + M.selSw.size > 0;
  ["#btn-up", "#btn-down", "#btn-sdup", "#btn-sdel"].forEach((s) => { $(s).disabled = !anySel; });
  scheduleCheck();
}

function selectStep(list, i, range, toggle) {
  const sel = list === "hw" ? M.selHw : M.selSw;
  const other = list === "hw" ? M.selSw : M.selHw;
  if (!toggle && !range) { M.selHw.clear(); M.selSw.clear(); sel.add(i); }
  else if (range && M.anchor && M.anchor.list === list) {
    const [a, b] = [Math.min(M.anchor.i, i), Math.max(M.anchor.i, i)];
    for (let k = a; k <= b; k++) sel.add(k);
  } else if (sel.has(i)) sel.delete(i); else sel.add(i);
  if (!toggle && !range) other.clear();
  M.anchor = { list, i };
  drawSteps();
}

/** Pointer drag by ⋮⋮: a line shows where the step(s) will land; drop moves them there. */
function startDrag(e, list, from, li) {
  if (e.button !== 0) return;
  e.preventDefault();
  const ol = li.parentElement;
  const rows = () => $$(".step", ol);
  let to = null, moved = false;
  const y0 = e.clientY;
  const move = (ev) => {
    if (!moved && Math.abs(ev.clientY - y0) < 4) return;
    if (!moved) { moved = true; li.classList.add("dragging"); document.body.classList.add("dragging-step"); }
    const r = ol.getBoundingClientRect();
    if (ev.clientY < r.top + 20) ol.scrollTop -= 8; else if (ev.clientY > r.bottom - 20) ol.scrollTop += 8;
    rows().forEach((x) => x.classList.remove("over", "over-end"));
    const all = rows();
    const hit = all.find((x) => { const b = x.getBoundingClientRect(); return ev.clientY < b.top + b.height / 2; });
    if (hit) { to = +hit.dataset.i; hit.classList.add("over"); } else if (all.length) { to = all.length; all[all.length - 1].classList.add("over-end"); }
  };
  const up = () => {
    document.removeEventListener("pointermove", move);
    document.removeEventListener("pointerup", up);
    document.body.classList.remove("dragging-step");
    li.classList.remove("dragging");
    rows().forEach((x) => x.classList.remove("over", "over-end"));
    if (moved && to !== null) moveTo(list, from, to);
  };
  document.addEventListener("pointermove", move);
  document.addEventListener("pointerup", up);
}

/** Drag: move the dragged step (or every selected one, if it is selected) before `to`. */
function moveTo(list, from, to) {
  const arr = list === "hw" ? M.items : M.soft, sel = list === "hw" ? M.selHw : M.selSw;
  const idx = sel.has(from) ? [...sel].sort((a, b) => a - b) : [from];
  // dropping onto itself or right below itself changes nothing
  if (idx.includes(to) || (idx.length === 1 && to === idx[0] + 1)) return;
  edit(() => {
    const moving = idx.map((k) => arr[k]);
    const before = idx.filter((k) => k < to).length;
    for (const k of [...idx].reverse()) arr.splice(k, 1);
    const at = to - before;
    arr.splice(at, 0, ...moving);
    sel.clear(); moving.forEach((_, k) => sel.add(at + k));
  });
}

/** ↑/↓ for every selected step, keeping their order. */
function moveSel(d) {
  edit(() => {
    for (const [arr, sel] of [[M.items, M.selHw], [M.soft, M.selSw]]) {
      const idx = [...sel].sort((a, b) => (d < 0 ? a - b : b - a));
      const next = new Set();
      for (const i of idx) {
        const j = i + d;
        if (j >= 0 && j < arr.length && !next.has(j)) { [arr[i], arr[j]] = [arr[j], arr[i]]; next.add(j); } else next.add(i);
      }
      sel.clear(); next.forEach((k) => sel.add(k));
    }
  });
}

function dupSel() {
  edit(() => {
    for (const [arr, sel] of [[M.items, M.selHw], [M.soft, M.selSw]]) {
      const idx = [...sel].sort((a, b) => a - b);
      if (!idx.length) continue;
      const copies = idx.map((k) => JSON.parse(JSON.stringify(arr[k])));
      const at = idx[idx.length - 1] + 1;
      arr.splice(at, 0, ...copies);
      sel.clear(); copies.forEach((_, k) => sel.add(at + k));
    }
  });
}

function delSel() {
  if (!M.selHw.size && !M.selSw.size) return;
  edit(() => {
    for (const [arr, sel] of [[M.items, M.selHw], [M.soft, M.selSw]]) {
      [...sel].sort((a, b) => b - a).forEach((k) => arr.splice(k, 1));
      sel.clear();
    }
  });
}

/** Insert after the last selected hardware step (else at the end) and select the new ones. */
function insertHw(...its) {
  edit(() => {
    const at = M.selHw.size ? Math.max(...M.selHw) + 1 : M.items.length;
    M.items.splice(at, 0, ...its);
    M.selHw.clear(); M.selSw.clear(); its.forEach((_, k) => M.selHw.add(at + k));
  });
  const row = $(`#steps li[data-i="${[...M.selHw][0]}"]`);
  if (row) row.scrollIntoView({ block: "nearest" });
}
function insertSw(a) {
  edit(() => {
    const at = M.selSw.size ? Math.max(...M.selSw) + 1 : M.soft.length;
    M.soft.splice(at, 0, a);
    M.selHw.clear(); M.selSw.clear(); M.selSw.add(at);
  });
  const row = $(`#sw-steps li[data-i="${[...M.selSw][0]}"]`);
  if (row) { row.scrollIntoView({ block: "nearest" }); const i = row.querySelector(".ed input"); if (i && (!i.value || i.value === "https://")) i.focus(); }
}

async function addFromMenu(what, anchor) {
  switch (what) {
    case "delay": return insertHw(item("delay", 50));
    case "key": { const k = await keyDialog(0x04); if (k != null) insertHw(item("key_press", k, 20)); return; }
    case "mouse":
      return popover(anchor, (box, close) => {
        [["mouse_click", "mac.s_click"], ["mouse_down", "mac.s_down"], ["mouse_up", "mac.s_up"]].forEach(([k, n]) =>
          MOUSE_BITS.forEach((b) => box.append(el("button", { class: "mi", text: `${t(n)}: ${t("mbit." + b)}`, onclick: () => { close(); insertHw(item(k, b, k === "mouse_click" ? 20 : 0)); } }))));
      }, "tall");
    case "wheel": return insertHw(item("wheel", 1));
    case "media": return insertHw(item("media", S.tables.consumer[0][0], 20));
    case "text": { const v = await textDialog(t("mac.a_text"), "", true); if (v) insertHw(item("text", 0, 0, v)); return; }
    case "macro": {
      const name = ($("#m-name").value || "").trim();
      const others = S.lib.filter((m) => m.name !== name);
      if (!others.length) return status(t("mac.no_other"), true);
      return popover(anchor, (box, close) => others.forEach((m) => box.append(el("button", { class: "mi", text: m.name, onclick: () => { close(); insertHw(item("macro", 0, 0, m.name)); } }))));
    }
    case "loop": {
      if (M.selHw.size) {
        // wrap the selected steps
        const a = Math.min(...M.selHw), b = Math.max(...M.selHw);
        edit(() => { M.items.splice(b + 1, 0, item("loop_end")); M.items.splice(a, 0, item("loop_start", 0, 2)); M.selHw.clear(); M.selHw.add(a); });
      } else insertHw(item("loop_start", 0, 2), item("loop_end"));
      return;
    }
    case "launch": return insertSw(soft("launch"));
    case "web": return insertSw(soft("launch", "https://"));
    case "run": return insertSw(soft("run"));
    case "type": { const v = await textDialog(t("mac.a_type"), "", false); if (v) insertSw(soft("type", v)); return; }
    case "sdelay": return insertSw(soft("delay", "", 500));
  }
}

// ---- live check (Rust compiles: size, time, trigger key)
function scheduleCheck() {
  clearTimeout(M.checkT);
  M.checkT = setTimeout(async () => {
    const name = ($("#m-name").value || "").trim() || "macro";
    const c = await quiet(api.call("check_macro", { name, items: M.items, software: M.soft }));
    if (!c) return;
    M.check = c;
    const info = $("#steps-info");
    info.classList.toggle("err", !c.ok);
    info.textContent = c.ok ? t("mac.info2", { n: c.steps, size: c.size, max: MAX_MACRO_BYTES, s: secs(c.ms) }) : macroErr(c.error);
    $("#m-time").textContent = c.ok ? t("mac.total", { s: secs(c.ms) }) : "";
    $("#sw-badge").textContent = c.trigger ? t("mac.badge_sw", { k: c.trigger }) : t("mac.badge_sw0");
  }, 120);
}
const secs = (ms) => (ms >= 1000 ? (ms / 1000).toFixed(ms % 1000 ? 2 : 0) + " " + t("mac.sec") : ms + " " + t("mac.ms"));

function drawSoftStatus() {
  const st = M.status && M.status.status;
  const box = $("#sw-status");
  if (!box) return;
  box.classList.remove("err");
  if (!st) { box.textContent = ""; return; }
  if (!st.supported) { box.textContent = t("mac.sw_unsupported"); box.classList.add("err"); }
  else if (!st.active) { box.textContent = t("mac.sw_inactive", { why: st.note }); box.classList.add("err"); }
  else box.textContent = t("mac.sw_active");
}

// ---- recorder (only inside its own pad — no global keyboard hooks)
const REC = { on: false, last: 0, opt: { mode: "real", fixed: 30, mouse: true } };
try { Object.assign(REC.opt, JSON.parse(localStorage.getItem("openrzr.rec") || "{}")); } catch (_) { /* no storage */ }
function recPush(it) {
  const now = performance.now();
  if (REC.last && REC.opt.mode !== "none") {
    const ms = REC.opt.mode === "real" ? Math.round(now - REC.last) : REC.opt.fixed;
    if (ms > 0) M.items.push(item("delay", ms));
  }
  REC.last = now;
  M.items.push(it);
  changed();
  const ol = $("#steps"); ol.scrollTop = ol.scrollHeight;
}
function recUi() {
  $("#btn-rec").textContent = REC.on ? t("mac.stop") : t("mac.record");
  $("#btn-rec").classList.toggle("rec", REC.on);
  $("#rec-pad").classList.toggle("rec", REC.on);
  $("#rec-pad").classList.toggle("hidden", !REC.on);
}
function toggleRec() {
  REC.on = !REC.on;
  recUi();
  const pad = $("#rec-pad");
  if (REC.on) {
    pushUndo();
    REC.last = 0;
    pad.textContent = t(REC.opt.mouse ? "mac.pad_rec" : "mac.pad_rec_keys");
    pad.focus();
    ulog("macro recording: start");
  } else {
    closeDangling();
    ulog(`macro recording: stop, ${M.items.length} steps`);
    drawSteps();
  }
}
function closeDangling() {
  const down = [];
  for (const s of M.items) {
    if (s.kind === "key_down" && !down.includes(s.value)) down.push(s.value);
    else if (s.kind === "key_up") { const i = down.indexOf(s.value); if (i >= 0) down.splice(i, 1); }
  }
  down.reverse().forEach((k) => M.items.push(item("key_up", k)));
}
function recMenu(anchor) {
  popover(anchor, (box) => {
    const radio = (mode, label) => {
      const r = el("input", { type: "radio", name: "recmode" });
      r.checked = REC.opt.mode === mode;
      r.addEventListener("change", () => { REC.opt.mode = mode; saveRecOpt(); });
      return el("label", { class: "check mi" }, r, " ", label);
    };
    const fixed = el("input", { type: "number", class: "num", min: 1, max: 5000, value: REC.opt.fixed });
    fixed.addEventListener("change", () => { REC.opt.fixed = Math.max(1, +fixed.value || 30); saveRecOpt(); });
    const mouse = el("input", { type: "checkbox" });
    mouse.checked = REC.opt.mouse;
    mouse.addEventListener("change", () => { REC.opt.mouse = mouse.checked; saveRecOpt(); });
    box.append(el("div", { class: "pop-title", text: t("mac.rec_delays") }), radio("real", t("mac.rec_real")),
      el("div", { class: "row mi" }, radio("fixed", t("mac.rec_fixed")), fixed, el("span", { class: "muted", text: t("mac.ms") })),
      radio("none", t("mac.rec_none")), el("hr"), el("label", { class: "check mi" }, mouse, " ", t("mac.rec_mouse")));
  }, "recopt");
}
function saveRecOpt() { try { localStorage.setItem("openrzr.rec", JSON.stringify(REC.opt)); } catch (_) { /* no storage */ } }

function bindRecorder() {
  const pad = $("#rec-pad");
  const keys = new Set();
  pad.addEventListener("keydown", (e) => {
    if (!REC.on) return;
    e.preventDefault();
    if (e.repeat) return;
    const c = HID.fromEvent(e);
    if (c != null && !keys.has(c)) { keys.add(c); recPush(item("key_down", c)); }
  });
  pad.addEventListener("keyup", (e) => {
    if (!REC.on) return;
    e.preventDefault();
    const c = HID.fromEvent(e);
    if (c != null) { keys.delete(c); recPush(item("key_up", c)); }
  });
  pad.addEventListener("mousedown", (e) => {
    if (!REC.on) return;
    e.preventDefault(); pad.focus();
    if (REC.opt.mouse && HID.mouseBit[e.button]) recPush(item("mouse_down", HID.mouseBit[e.button]));
  });
  pad.addEventListener("mouseup", (e) => {
    if (REC.on && REC.opt.mouse && HID.mouseBit[e.button]) recPush(item("mouse_up", HID.mouseBit[e.button]));
  });
  pad.addEventListener("wheel", (e) => {
    if (!REC.on) return;
    e.preventDefault();
    if (REC.opt.mouse) recPush(item("wheel", e.deltaY < 0 ? 1 : -1));
  }, { passive: false });
  pad.addEventListener("contextmenu", (e) => { if (REC.on) e.preventDefault(); });
  pad.addEventListener("auxclick", (e) => { if (REC.on) e.preventDefault(); });
}

// ---- library
async function newMacro() {
  if (!(await leaveEditor())) return;
  S.libSel = null; S.mouseSel = null; drawLib();
  $$("#mouse-macros li").forEach((x) => x.classList.remove("sel"));
  const used = new Set([...S.onMouse.map((m) => m.id), ...S.lib.map((m) => m.id)]);
  let i = 1; while (used.has(i)) i++;
  let n = 1; while (S.lib.some((m) => m.name === t("mac.new_name", { n }))) n++;
  $("#m-id").value = i; $("#m-name").value = t("mac.new_name", { n });
  loadDoc([], []);
  $("#m-name").select();
}

function libMenu(anchor) {
  popover(anchor, (box, close) => {
    const m = S.lib.find((x) => x.name === S.libSel);
    const it = (key, fn, dis) => box.append(el("button", { class: "mi", text: t(key), disabled: dis, onclick: () => { close(); fn(); } }));
    it("mac.new", newMacro);
    it("prof.rename", async () => {
      const name = await promptBox(t("mac.rename_title"), t("mac.name"), m.name);
      if (!name || name === m.name) return;
      await run("rename_macro_lib", { old: m.name, name }, t("mac.renamed"));
      S.libSel = name; $("#m-name").value = name;
      await refreshMacros();
    }, !m);
    it("prof.duplicate", async () => {
      let n = 2; while (S.lib.some((x) => x.name === `${m.name} (${n})`)) n++;
      const used = new Set([...S.onMouse.map((x) => x.id), ...S.lib.map((x) => x.id)]);
      let id = 1; while (used.has(id)) id++;
      await run("save_macro_lib", { name: `${m.name} (${n})`, id, items: m.items, software: m.software }, t("mac.saved_lib", { name: `${m.name} (${n})` }));
      await refreshMacros();
    }, !m);
    box.append(el("hr"));
    it("common.delete", async () => {
      if (!(await confirmBox(t("mac.del_title"), t("mac.del_text", { name: m.name }), t("common.delete")))) return;
      await run("delete_macro_lib", { name: m.name });
      S.libSel = null; setDirty(false);
      await refreshMacros();
    }, !m);
  });
}

function macroName() {
  const id = +$("#m-id").value, name = $("#m-name").value.trim() || t("mac.default_name", { id });
  $("#m-name").value = name;
  return { id, name };
}

async function saveLib() {
  const { id, name } = macroName();
  const err = await run("save_macro_lib", { name, id, items: M.items, software: M.soft });
  if (err) status(t("mac.saved_with_error", { name, err: macroErr(err) }), true); else status(t("mac.saved_lib", { name }));
  S.libSel = name; setDirty(false);
  await refreshMacros();
}

async function writeMacro() {
  if (!needMouse()) return;
  if (!M.items.length && !M.soft.length) return status(t("mac.empty"), true);
  const { id, name } = macroName();
  const w = await run("write_macro", { id, name, items: M.items, software: M.soft });
  status(w.trigger ? t("mac.written_sw", { id, n: w.bytes, k: w.trigger }) : t("mac.written", { id, n: w.bytes }));
  S.mouseSel = id; S.libSel = name; setDirty(false);
  await refreshMacros();
}

async function deleteMouseMacro() {
  if (!needMouse() || S.mouseSel == null) return;
  if (!(await confirmBox(t("mac.erase_title"), t("mac.erase_text", { id: S.mouseSel }), t("common.delete")))) return;
  await run("delete_macro", { id: S.mouseSel }, t("mac.erased"));
  S.mouseSel = null;
  await refreshMacros();
}

function bindMacroPage() {
  $("#btn-mnew").addEventListener("click", newMacro);
  $("#btn-mlibmenu").addEventListener("click", (e) => libMenu(e.currentTarget));
  $("#btn-mrefresh").addEventListener("click", refreshMacros);
  $("#btn-mdel").addEventListener("click", deleteMouseMacro);
  $("#btn-rec").addEventListener("click", toggleRec);
  $("#btn-rec-opt").addEventListener("click", (e) => recMenu(e.currentTarget));
  $$(".mac-add button.add").forEach((b) => b.addEventListener("click", () => addFromMenu(b.dataset.add, b)));
  $("#btn-undo").addEventListener("click", undo);
  $("#btn-redo").addEventListener("click", redo);
  $("#btn-up").addEventListener("click", () => moveSel(-1));
  $("#btn-down").addEventListener("click", () => moveSel(1));
  $("#btn-sdup").addEventListener("click", dupSel);
  $("#btn-sdel").addEventListener("click", delSel);
  $("#btn-msave").addEventListener("click", () => quiet(saveLib()));
  $("#btn-mwrite").addEventListener("click", () => quiet(writeMacro()));
  $("#btn-swtest").addEventListener("click", () => quiet(run("test_program_actions", { software: M.soft }, t("mac.sw_tested"))));
  $("#m-all").addEventListener("change", (e) => {
    M.selHw.clear(); M.selSw.clear();
    if (e.target.checked) { M.items.forEach((_, i) => M.selHw.add(i)); M.soft.forEach((_, i) => M.selSw.add(i)); }
    drawSteps();
  });
  $("#m-name").addEventListener("input", () => setDirty(true));
  $("#m-id").addEventListener("input", () => setDirty(true));
  document.addEventListener("keydown", (e) => {
    if ($("#page-macros").classList.contains("hidden") || !$("#modal").classList.contains("hidden") || REC.on) return;
    if (e.target.closest("input,select,textarea")) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.code === "KeyZ" && !e.shiftKey) { e.preventDefault(); undo(); }
    else if (mod && (e.code === "KeyY" || (e.code === "KeyZ" && e.shiftKey))) { e.preventDefault(); redo(); }
    else if (mod && e.code === "KeyD") { e.preventDefault(); dupSel(); }
    else if (e.key === "Delete") { e.preventDefault(); delSel(); }
  });
  bindRecorder();
  recUi();
}

// ------------------------------------------------------------------ lighting
/** Effects of a zone: what the connected mouse reports, else the general table. */
function zoneEffects(zone) {
  return (S.conn && S.conn.zone_effects && S.conn.zone_effects[zone]) || S.tables.zone_effects[zone] || [];
}

function zoneCard(zone) {
  const T = S.tables;
  const card = el("div", { class: "card zone", "data-zone": zone });
  const eff = el("select", { class: "eff" });
  fillSelect(eff, zoneEffects(zone).map((e) => [e, t("effect." + e)]));
  const c1 = el("input", { type: "color", class: "c1", value: "#00ff80" });
  const c2 = el("input", { type: "color", class: "c2", value: "#0040ff" });
  const leds = [0, 1, 2].map((i) => el("input", { type: "color", class: "led", value: ["#ff0000", "#00ff00", "#0000ff"][i] }));
  const speed = el("select", { class: "speed" });
  fillSelect(speed, [1, 2, 3, 4].map((n) => [n, t("light.speed." + n)]), 2);
  const br = el("input", { type: "range", class: "br grow", min: 0, max: 100, value: 100 });
  const brv = el("span", { class: "val", text: "100%" });
  br.addEventListener("input", () => { brv.textContent = br.value + "%"; });
  const prev = el("div", { class: "preview" });
  const rColors = el("div", { class: "row" }, c1, c2);
  const rLeds = el("div", { class: "row" }, ...leds.flatMap((l, i) => [el("span", { class: "muted", text: i + 1 }), l]));
  const lc = el("label", { text: t("light.colors") }), ll = el("label", { text: t("light.leds") }), ls = el("label", { text: t("light.afterglow") });
  const grid = el("div", { class: "grid" },
    el("label", { text: t("light.effect") }), eff, lc, rColors, ll, rLeds, ls, speed,
    el("label", { text: t("light.brightness") }), el("div", { class: "row" }, br, brv));
  const ui = () => {
    const e = eff.value;
    const n = { static: 1, breath_single: 1, reactive: 1, breath_dual: 2 }[e] || 0;
    lc.classList.toggle("hidden", !n); rColors.classList.toggle("hidden", !n); c2.classList.toggle("hidden", n < 2);
    ll.classList.toggle("hidden", e !== "custom"); rLeds.classList.toggle("hidden", e !== "custom");
    ls.classList.toggle("hidden", e !== "reactive"); speed.classList.toggle("hidden", e !== "reactive");
    const rainbow = "linear-gradient(90deg,#f00,#0f0,#00f,#f0f)";
    prev.style.background = e === "off" ? "#000" : e === "custom" ? `linear-gradient(90deg,${leds.map((l) => l.value).join(",")})`
      : ["spectrum", "wave", "breath_random"].includes(e) ? rainbow
      : e === "breath_dual" ? `linear-gradient(90deg,${c1.value},${c2.value})` : c1.value;
  };
  [eff, c1, c2, ...leds].forEach((x) => x.addEventListener("input", ui));
  eff.addEventListener("change", ui);
  ui();
  card.append(el("div", { class: "row" }, el("div", { class: "card-title", text: t("zone." + zone) }), el("span", { class: "grow" }), prev),
    grid, el("div", { class: "row" }, el("span", { class: "grow" }),
      el("button", { class: "primary", text: t("common.apply"), onclick: () => applyZone(zone) })));
  card._get = () => ({ effect: eff.value, color1: hexRgb(c1.value), color2: hexRgb(c2.value), speed: +speed.value,
    brightness: +br.value, leds: leds.map((l) => hexRgb(l.value)) });
  card._set = (s) => {
    if (s.effect && [...eff.options].some((o) => o.value === s.effect)) eff.value = s.effect;
    if (s.color1) c1.value = rgbHex(s.color1);
    if (s.color2) c2.value = rgbHex(s.color2);
    if (s.leds) s.leds.forEach((c, i) => { if (leds[i]) leds[i].value = rgbHex(c); });
    if (s.speed) speed.value = String(s.speed);
    if (s.brightness != null) { br.value = s.brightness; brv.textContent = s.brightness + "%"; }
    ui();
  };
  card._setBrightness = (v) => { br.value = v; brv.textContent = v + "%"; };
  return card;
}

function buildZones() {
  const box = $("#zones");
  const saved = {};
  $$("#zones .zone").forEach((c) => { saved[c.dataset.zone] = c._get(); });
  box.innerHTML = "";
  for (const [z] of S.tables.zones) {
    if (S.conn && !S.conn.lighting.includes(z)) continue;
    if (!S.conn && !["scroll", "logo", "side"].includes(z)) continue;
    const c = zoneCard(z);
    if (saved[z]) c._set(saved[z]);
    if (S.conn) c.classList.toggle("hidden", !S.conn.lighting.includes(z));
    box.append(c);
  }
}

async function loadLighting() {
  if (!S.conn) return;
  $$("#zones .zone").forEach((c) => c.classList.toggle("hidden", !S.conn.lighting.includes(c.dataset.zone)));
  const r = await run("get_lighting");
  for (const [z, st] of Object.entries(r.saved || {})) { const c = $(`#zones [data-zone="${z}"]`); if (c) c._set(st); }
  for (const [z, v] of Object.entries(r.brightness || {})) { const c = $(`#zones [data-zone="${z}"]`); if (c) c._setBrightness(v); }
}

async function sendLighting(targets) {
  if (!needMouse() || !targets.length) return;
  const zones = targets.map(([z]) => t("zone." + z)).join(", ");
  await run("set_lighting", { targets }, t("light.done", { zones, effect: t("effect." + targets[0][1].effect) }));
}

async function applyZone(zone) {
  const st = $(`#zones [data-zone="${zone}"]`)._get();
  if (!$("#light-sync").checked) return sendLighting([[zone, st]]);
  const list = [];
  for (const z of S.conn.lighting) {
    if (!zoneEffects(z).includes(st.effect)) continue;
    list.push([z, st]);
    if (z !== zone) $(`#zones [data-zone="${z}"]`)._set(st);
  }
  await sendLighting(list);
}

async function applyAllZones() {
  if (!needMouse()) return;
  await sendLighting(S.conn.lighting.map((z) => [z, $(`#zones [data-zone="${z}"]`)._get()]));
}

async function lightsOff() {
  if (!needMouse()) return;
  await sendLighting(S.conn.lighting.map((z) => {
    const c = $(`#zones [data-zone="${z}"]`);
    const st = { ...c._get(), effect: "off" };
    c._set(st);
    return [z, st];
  }));
}

// ------------------------------------------------------------------ power
function powerLabels() {
  $("#idle-v").textContent = t("power.min", { n: $("#idle").value });
  $("#lowbatt-v").textContent = $("#lowbatt").value + "%";
}
function loadPower(c) {
  if (c.idle) $("#idle").value = Math.max(1, Math.min(15, Math.round(c.idle / 60)));
  if (c.low) $("#lowbatt").value = Math.max(5, Math.min(100, Math.round(c.low / 5) * 5));
  powerLabels();
}
async function applyPower() {
  if (!needMouse()) return;
  await run("set_power", { idle: has("idle") ? +$("#idle").value * 60 : 0, low: has("low_battery") ? +$("#lowbatt").value : 0 }, t("power.saved"));
}

// ------------------------------------------------------------------ diagnostics / log
async function pollLog() {
  clearTimeout(pollLog.t);
  try {
    const r = await api.call("logs_since", { since: S.logSeq });
    if (r.lines.length) {
      S.logSeq = r.seq;
      const pre = $("#log"), filt = $("#log-filter").value;
      const frag = document.createDocumentFragment();
      for (const l of r.lines) {
        const cls = l.includes(" ERROR ") ? "e" : l.includes(" WARN ") ? "w" : l.includes(" DEBUG ") ? "d" : l.includes(" ui ") ? "u" : "";
        const sp = el("span", { class: cls, text: l + "\n" });
        if (filt && !l.includes(filt)) sp.classList.add("hidden");
        frag.append(sp);
      }
      pre.append(frag);
      while (pre.childNodes.length > 4000) pre.removeChild(pre.firstChild);
      if ($("#log-follow").checked) pre.scrollTop = pre.scrollHeight;
    }
  } catch (e) { /* ignore */ }
  if (!$("#page-diag").classList.contains("hidden")) pollLog.t = setTimeout(pollLog, 1000);
}
function applyLogFilter() {
  const f = $("#log-filter").value;
  $$("#log span").forEach((s) => s.classList.toggle("hidden", !!f && !s.textContent.includes(f)));
}
async function sendRaw() {
  if (!needMouse()) return;
  const cmd = $("#raw-cmd").value.trim(), args = $("#raw-args").value, size = +$("#raw-size").value || null;
  if (!/^(0x)?[0-9a-fA-F]{1,4}$/.test(cmd)) return status(t("diag.bad_cmd"), true);
  await run("raw_command", { cmd, args, size }, (r) => t("diag.raw_ok", { r }));
}

// ------------------------------------------------------------------ window style
const CLASSIC_TABS = { "tab.macros": "nav.macros", "tab.settings": "nav.settings", "tab.diag": "nav.diag" };

/** "synapse": tabs on top like the original configurator; "classic": the first open-rzr layout with a sidebar. */
function applyStyle(style) {
  const classic = style === "classic";
  const changed = S.style !== undefined && S.style !== style;
  S.style = style;
  document.body.classList.toggle("classic", classic);
  let side = $("#sidebar");
  if (classic) {
    if (!side) { side = el("aside", { id: "sidebar" }); $("#app").prepend(side); }
    side.append($(".brand"), $("#nav"), $("#tabs"));
  } else if (side) {
    $("#top").prepend($(".brand"));
    $(".brand").after($("#tabs"));
    $("#prof-box").after($("#nav"));
    side.remove();
  }
  // labels: "Customize / Info" vs "Buttons / Device"
  $$("#nav button").forEach((b) => {
    if (!b.dataset.syn) b.dataset.syn = b.dataset.i18n;
    b.dataset.i18n = classic ? b.dataset.classic : b.dataset.syn;
  });
  $$("#tabs [data-i18n]").forEach((e) => {
    if (!e.dataset.syn) e.dataset.syn = e.dataset.i18n;
    e.dataset.i18n = classic && CLASSIC_TABS[e.dataset.syn] ? CLASSIC_TABS[e.dataset.syn] : e.dataset.syn;
  });
  i18n.apply();
  // the classic look showed the buttons as a table
  if (changed || S.view === undefined) S.view = classic ? "list" : "figure";
  $$('input[name="ui-style"]').forEach((r) => { r.checked = r.value === style; });
  drawCustomize();
}

async function setStyle(style) {
  applyStyle(style);
  await run("set_ui_style", { style }, t("set.style_saved"));
}

// ------------------------------------------------------------------ settings & language
async function loadSettings() {
  S.settings = await run("get_settings");
  fillSelect($("#set-lang"), i18n.languages(), i18n.lang);
  $("#set-autostart").checked = S.settings.autostart;
  $("#set-minimized").checked = S.settings.start_minimized || !S.settings.autostart;
  $("#set-minimized").disabled = !S.settings.autostart;
  $("#set-close-tray").checked = S.settings.close_to_tray;
  $("#log-level").value = S.settings.log_level;
  $$('input[name="ui-style"]').forEach((r) => { r.checked = r.value === (S.style || "synapse"); });
  await quiet(loadOpenRazer());
  await quiet(loadRazer());
  drawAbout();
}

// ------------------------------------------------------------------ OpenRazer compatibility
async function loadOpenRazer() {
  const o = await api.call("openrazer_status");
  S.openrazer = o;
  $("#or-card").classList.toggle("hidden", !o.supported_os);
  if (!o.supported_os) return o;
  $("#or-mode").value = o.mode || "auto";
  const kv = $("#or-kv");
  kv.innerHTML = "";
  const bound = o.bound.map(([pid, dir]) => `${pid.toString(16).padStart(4, "0")} · ${dir}`).join("\n");
  for (const [k, v] of [[t("or.driver"), o.driver_loaded ? t("or.loaded") : t("or.not_loaded")],
    [t("or.daemon"), o.daemon_running ? t("or.running") : t("or.not_running")],
    [t("or.bound"), bound || "—"]]) kv.append(el("dt", { text: k }), el("dd", { text: v, style: "white-space:pre-wrap" }));
  $("#btn-or-stop").disabled = !o.daemon_running;
  applyPoly(o);
  return o;
}

/** Polychromatic (separate program) runs: it manages the lighting, our Lighting tab steps aside. */
function applyPoly(o) {
  const on = !!o && o.polychromatic_running;
  $("#poly-state").textContent = !o || !o.polychromatic_installed ? t("poly.not_installed") : on ? t("poly.running") : t("poly.installed");
  $$(".btn-poly-open").forEach((b) => { b.disabled = !o || !o.polychromatic_installed; });
  if (on && !S.polyWas) ulog("Polychromatic is running: lighting is left to it");
  S.polyWas = on;
  $("#poly-notice").classList.toggle("hidden", !on);
  $("#page-lighting").classList.toggle("poly", on && !S.polyOverride);
}

async function checkPoly() {
  if (!S.info || S.info.os !== "linux" || S.busy) return;
  const o = await quiet(api.call("openrazer_status"));
  if (o) { S.openrazer = o; applyPoly(o); }
}

/** The daemon talks to the mouse too: warn when we are not going through its driver. */
async function checkOpenRazer() {
  const o = await loadOpenRazer();
  const conflict = o && o.supported_os && o.daemon_running && S.conn && !S.conn.openrazer;
  $("#or-notice").classList.toggle("hidden", !conflict);
  // Polychromatic needs the daemon: never suggest stopping it then
  const poly = !!o && o.polychromatic_running;
  $("#or-notice-text").dataset.i18n = poly ? "or.daemon_conflict_poly" : "or.daemon_conflict";
  $("#or-notice-text").textContent = t($("#or-notice-text").dataset.i18n);
  $("#btn-or-stop2").classList.toggle("hidden", poly);
  if (conflict) ulog("OpenRazer daemon is running while the mouse is used directly", "WARN");
}

async function setOpenRazerMode() {
  await run("set_openrazer_mode", { mode: $("#or-mode").value }, t("or.mode_saved"));
  await rescan();
}

async function stopOpenRazer() {
  if (S.openrazer && S.openrazer.polychromatic_running && !(await confirmBox(t("or.stop"), t("or.stop_poly"), t("or.stop")))) return;
  await run("stop_openrazer_daemon", {}, t("or.stopped"));
  await rescan();
  await quiet(loadOpenRazer());
  status(t("or.stopped"));
}

// ------------------------------------------------------------------ about
function drawAbout() {
  if (!S.info) return;
  $("#about-version").textContent = "open-rzr " + S.info.version;
  $("#about-db").textContent = t("about.db", { n: S.info.db_models, rev: S.info.openrazer_db });
}

async function showDevices() {
  const list = await run("supported_devices");
  await modal((box, close) => {
    box.classList.add("wide");
    const q = el("input", { class: "grow", placeholder: t("about.search") });
    const body = el("tbody");
    const feats = (fs) => fs.map((f) => t("feat." + f)).join(", ");
    const draw = () => {
      const s = q.value.trim().toLowerCase();
      body.innerHTML = "";
      for (const d of list) {
        const pid = d.pid.toString(16).padStart(4, "0");
        if (s && !d.name.toLowerCase().includes(s) && !pid.includes(s)) continue;
        body.append(el("tr", {},
          el("td", { class: "mono", text: pid }),
          el("td", {}, d.name, el("span", { class: "badge" + (d.verified ? " ok" : ""), title: d.probe ? t("about.probe_note") : "",
            text: d.verified ? t("about.tested") : d.source === "family" ? t("about.family") : d.probe ? "OpenRazer + " + t("about.probe") : "OpenRazer" })),
          el("td", { text: feats(d.direct) || "—" }),
          el("td", { class: "muted", text: feats(d.openrazer) || "—" })));
      }
    };
    q.addEventListener("input", draw);
    box.append(el("h2", { text: t("about.devices_title", { n: list.length }) }),
      el("div", { class: "muted small", text: t("about.devices_note") }),
      el("div", { class: "row" }, q),
      el("div", { class: "devlist" }, el("table", {},
        el("thead", {}, el("tr", {}, el("th", { text: "PID" }), el("th", { text: t("about.model") }),
          el("th", { text: t("about.direct") }), el("th", { text: t("about.via_or") }))), body)),
      el("div", { class: "buttons" }, el("button", { class: "primary", text: t("common.ok"), onclick: () => close(null) })));
    draw();
  });
}

async function saveAutostart() {
  const on = $("#set-autostart").checked, min = $("#set-minimized").checked;
  const st = await run("set_autostart", { enabled: on, minimized: min }, t("set.saved"));
  $("#set-autostart").checked = st.enabled;
  $("#set-minimized").disabled = !st.enabled;
}

/** Re-render everything that is built from strings in JS (static HTML is handled by i18n.apply). */
function relabel() {
  i18n.apply();
  $("#log-path").textContent = t("diag.log_file", { path: S.info.log_file });
  recUi();
  powerLabels();
  paintStageDots();
  buildZones();
  drawAbout();
  drawCustomize();
  drawAutoChip();
  showProfileDpi();
  if (S.openrazer) quiet(loadOpenRazer());
  if (S.razer) quiet(checkRazer());
  if (S.conn) {
    loadDevice(S.conn); showCurDpi(S.conn.dpi); drawMouseMacros();
    $("#tab-device-name").textContent = shortName(deviceName(S.conn.pid || 0, S.conn.name));
    $$("#dev-select option").forEach((o, i) => { const g = S.groups[i]; if (g) o.textContent = deviceName(g.pid, g.label); });
  }
  else { showCurDpi(null); const o = $("#dev-select").options[0]; if (o && !S.groups.length) o.textContent = t("top.none"); }
  drawLib(); drawSteps(); drawSoftStatus();
}

async function setLanguage(lang, save = true) {
  i18n.set(lang);
  if (save) await quiet(api.call("set_language", { language: lang }));
  relabel();
  if (currentPage() === "help") quiet(openHelp());
}

/** First run: use the system language, then ask once whether to keep it. */
async function firstRunLanguage() {
  const detected = i18n.detect();
  await setLanguage(detected, false);
  const other = detected === "en" ? "ru" : "en";
  const keep = await modal((box, close) => {
    box.append(el("h2", { text: t("first.title") }),
      el("div", { text: t("first.text", { lang: window.I18N[detected]["lang.name"] }) }),
      el("div", { class: "buttons" },
        el("button", { text: t("first.switch", { other: window.I18N[other]["lang.name"] }), onclick: () => close(false) }),
        el("button", { class: "primary", text: t("first.keep"), autofocus: true, onclick: () => close(true) })));
  });
  await setLanguage(keep === false ? other : detected, true);
  ulog(`first run: language ${i18n.lang} (detected ${detected})`);
}

// ------------------------------------------------------------------ init
function bind() {
  $$("#nav button").forEach((b) => b.addEventListener("click", () => showPage(b.dataset.page)));
  $("#btn-rescan").addEventListener("click", rescan);
  $("#dev-select").addEventListener("change", (e) => { if (S.groups.length) { ulog("mouse selected: " + e.target.selectedOptions[0].text); connect(+e.target.value); } });
  $("#btn-batt").addEventListener("click", refreshBattery);
  $("#btn-dpi").addEventListener("click", applyDpi);
  $("#btn-poll").addEventListener("click", () => needMouse() && run("set_polling", { hz: +$("#poll").value }, t("perf.poll_saved", { hz: $("#poll").value })));
  $("#btn-all").addEventListener("change", drawList);
  $("#btn-layer").addEventListener("click", (e) => layerMenu(e.currentTarget));
  $$("#layer-seg button").forEach((b) => b.addEventListener("click", () => { S.hs = b.dataset.hs === "1"; ulog("layer: " + (S.hs ? "Hypershift" : "Standard"), "DEBUG"); drawCustomize(); }));
  $("#btn-view").addEventListener("click", () => { S.view = S.view === "list" ? "figure" : "list"; drawCustomize(); });
  $$("#plate-box .plate").forEach((b) => b.addEventListener("click", () => {
    if (!S.conn) return;
    S.figView = b.dataset.view;
    ulog("view: " + S.figView, "DEBUG");
    drawCustomize();
  }));
  $("#btn-detect").addEventListener("click", detectButton);
  $("#prof-select").addEventListener("change", (e) => { closePopover(); openProfile(e.target.value); });
  $("#btn-prof-menu").addEventListener("click", (e) => { if (S.profiles) profileMenu(e.currentTarget); });
  $("#btn-write").addEventListener("click", (e) => { if (S.profiles) quiet(writeToMouse(e.currentTarget)); });
  $("#btn-onboard").addEventListener("click", (e) => { if (S.profiles) onboardMenu(e.currentTarget); });
  $$("#tabs button").forEach((b) => b.addEventListener("click", () => showPage(b.dataset.tab === "device" ? (S.devPage || "buttons") : b.dataset.tab)));
  $("#btn-backup").addEventListener("click", doBackup);
  $("#btn-restore").addEventListener("click", doRestore);
  bindMacroPage();
  bindHelp();
  $("#btn-light-all").addEventListener("click", applyAllZones);
  $("#btn-light-off").addEventListener("click", lightsOff);
  $("#idle").addEventListener("input", powerLabels);
  $("#lowbatt").addEventListener("input", powerLabels);
  $("#btn-power").addEventListener("click", applyPower);
  $("#log-filter").addEventListener("change", applyLogFilter);
  $("#btn-logdir").addEventListener("click", () => run("open_log_folder"));
  $("#btn-report").addEventListener("click", () => run("save_report", {}, (p) => t("diag.report_saved", { path: p })));
  $("#btn-raw").addEventListener("click", sendRaw);
  // settings
  $("#set-lang").addEventListener("change", () => setLanguage($("#set-lang").value));
  $("#set-autostart").addEventListener("change", () => { $("#set-minimized").disabled = !$("#set-autostart").checked; saveAutostart(); });
  $("#set-minimized").addEventListener("change", saveAutostart);
  $("#btn-hidlist").addEventListener("click", () => quiet(showHidList()));
  $("#btn-drivers").addEventListener("click", () => quiet(showDrivers()));
  $("#btn-wireless").addEventListener("click", () => quiet(showWireless()));
  $("#set-autowrite").addEventListener("change", async (e) => {
    if (!S.profiles) { e.target.checked = false; return status(t("wr.need_mouse"), true); }
    S.profiles = await run("set_autowrite", { on: e.target.checked }, t("set.saved"));
    drawProfileSelect();
  });
  $("#set-close-tray").addEventListener("change", () => run("set_close_to_tray", { enabled: $("#set-close-tray").checked }, t("set.saved")));
  $("#log-level").addEventListener("change", () => run("set_log_level", { level: $("#log-level").value }, t("set.saved")));
  $("#btn-quit").addEventListener("click", () => api.call("quit_app"));
  $$('input[name="ui-style"]').forEach((r) => r.addEventListener("change", () => r.checked && setStyle(r.value)));
  $("#or-mode").addEventListener("change", setOpenRazerMode);
  $("#btn-or-refresh").addEventListener("click", () => run("openrazer_status").then(loadOpenRazer));
  $("#btn-or-stop").addEventListener("click", stopOpenRazer);
  $("#btn-or-stop2").addEventListener("click", stopOpenRazer);
  $$("[data-goto]").forEach((b) => b.addEventListener("click", () => showPage(b.dataset.goto)));
  $("#btn-devices").addEventListener("click", showDevices);
  $$(".btn-poly-open").forEach((b) => b.addEventListener("click", () => run("open_polychromatic", {}, t("poly.started"))));
  $("#btn-poly-here").addEventListener("click", () => {
    S.polyOverride = true;
    ulog("lighting: managed here although Polychromatic runs");
    $("#page-lighting").classList.remove("poly");
    status(t("poly.override"));
  });
  $("#btn-rz-stop").addEventListener("click", stopRazer);
  $("#btn-rz-stop2").addEventListener("click", stopRazer);
  $("#btn-rz-start").addEventListener("click", startRazer);
  $("#btn-rz-refresh").addEventListener("click", () => run("razer_software").then(checkRazer));
  $("#rz-autostart").addEventListener("change", () => { if (S.razer) loadRazer(); });
  window.addEventListener("error", (e) => ulog(`UI error: ${e.message} (${e.filename}:${e.lineno})`, "ERROR"));
  window.addEventListener("unhandledrejection", (e) => ulog(`unhandled rejection: ${e.reason}`, "WARN"));
}

async function init() {
  bind();
  S.info = await api.call("app_info");
  S.tables = await api.call("tables");
  S.settings = await api.call("get_settings");
  applyStyle(S.settings.ui_style || "synapse");
  buildStages(5);
  if (S.settings.language) await setLanguage(S.settings.language, false);
  else await firstRunLanguage();
  ulog("window opened");
  api.on("device-change", onDeviceChange);
  api.on("profile-switched", onProfileSwitched);
  api.on("close-requested", () => quiet(onCloseRequested()));
  let mem = {};
  try { mem = JSON.parse((await quiet(api.call("ui_state_get"))) || "{}"); } catch (_) { mem = {}; }
  S.devIndex = mem.dev || 0;
  S.uiRestored = true;
  showPage(mem.page && $("#page-" + mem.page) ? mem.page : "buttons");
  await quiet(rescan());
  if (!S.conn) await quiet(checkRazer()); // Synapse holding the mouse is a common reason it does not answer
  setInterval(refreshBattery, 60000);
  setInterval(checkPlate, 5000);
  watchRazer();
  setInterval(checkPoly, 10000); // Linux: Polychromatic started or closed // swap the plate — the list follows (Auto mode, Buttons tab open)
  const page = new URLSearchParams(location.search).get("page");
  if (page) showPage(page);
}

init().catch((e) => status(t("err.ui") + ": " + e, true));
