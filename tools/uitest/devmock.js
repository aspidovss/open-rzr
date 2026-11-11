// Generated argument names of the Rust commands are loaded first (sigs.js, written by build.js).
// Test-only stand-in for Tauri IPC. Enforces the exact argument names of the Rust commands
// and returns data shaped like the serde output of the Rust structs.
(function () {
  const KEYS = [];
  for (let i = 0; i < 26; i++) KEYS.push([4 + i, String.fromCharCode(65 + i)]);
  for (let i = 1; i <= 9; i++) KEYS.push([0x1d + i, String(i)]);
  KEYS.push([0x27, "0"], [0x2d, "-"], [0x2e, "="], [0x35, "`"], [0x3e, "F5"], [0xe0, "LCtrl"], [0xe1, "LShift"]);
  const name = (c) => (KEYS.find(([k]) => k === c) || [0, "0x" + c.toString(16)])[1];
  const A = (o) => Object.assign({ kind: "disabled", button: 1, modifiers: 0, key: 0, interval: 100, code: 0, macro_id: 0,
    macro_mode: "once", repeat: 1, stage: 1, dpi_x: 400, dpi_y: 400, profile: 1, raw: [] }, o);
  const btn = {};
  const defs = [[1, "Левая кнопка", A({ kind: "mouse", button: 1 })], [2, "Правая кнопка", A({ kind: "mouse", button: 2 })],
    [0x0b, "Кнопка DPI + (верх)", A({ kind: "dpi", code: 1 })], [0x0e, "Кнопка профиля (снизу)", A({ kind: "profile", code: 4 })]];
  const side = [0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2d, 0x2e];
  for (const p of [0, 1, 2, 3, 4, 5]) for (const h of [false, true]) {
    for (const [id, , a] of defs) btn[`${p}/${id}/${h}`] = a;
    side.forEach((k, i) => { btn[`${p}/${0x40 + i}/${h}`] = A({ kind: "keyboard", key: k }); });
    side.slice(0, 6).forEach((k, i) => { btn[`${p}/${0x50 + i}/${h}`] = A({ kind: "keyboard", key: k }); });
  }
  const names = {}; defs.forEach(([id, n]) => { names[id] = n; });
  for (let i = 0; i < 12; i++) names[0x40 + i] = "12 кн.: " + (i + 1);
  for (let i = 0; i < 6; i++) names[0x50 + i] = "6 кн.: " + (i + 1);
  const st = { macros: {}, lib: {}, stages: { 1: { active: 2, stages: [[400, 400], [800, 800], [1600, 1600]] } }, dpi: [800, 800],
    log: [], seq: 0, lang: (window.__LANG__ ?? 'ru'), auto: { enabled: false, minimized: false }, closeTray: true, failBattery: false, devicesGone: false, names: {}, plate: 12, choice: 'auto', lastPlate: null, saved: {}, bright: { scroll: 100, logo: 100, side: 100 }, calls: [], polyRun: true, style: window.__STYLE__ || "synapse", db: window.__DB__ || false,
    rz: { services: window.__RZ__ ? [{ name: "Razer Synapse Service", display: "Razer Synapse Service", running: true, start: "auto" }, { name: "RzActionSvc", display: "Razer Chroma SDK Service", running: false, start: "demand" }] : [], processes: window.__RZ__ ? ["RazerAppEngine.exe"] : [], saved: false, denyUac: false },
    or: { os: true, driver: !!window.__ORD__, daemon: !!window.__ORD__, bridged: false, mode: "auto" } };
  const log = (t, m) => { st.seq++; st.log.push(`2026-11-07 00:00:00.000Z INFO  ${t.padEnd(6)} ${m}`); };
  const describe = (a) => {
    switch (a.kind) {
      case "keyboard": return "Клавиша: " + [a.modifiers & 1 ? "LCtrl" : "", a.modifiers & 2 ? "LShift" : "", name(a.key)].filter(Boolean).join("+");
      case "mouse": return "Мышь: " + ["", "ЛКМ", "ПКМ", "Средняя", "Назад", "Вперёд"][a.button];
      case "macro": return `Макрос #${a.macro_id} (${a.macro_mode})`;
      case "dpi": return a.code === 5 ? `Снайпер: ${a.dpi_x} DPI` : "DPI op " + a.code;
      case "profile": return a.code === 3 ? "Профиль " + a.profile : "Профиль op " + a.code;
      default: return a.kind;
    }
  };

  const clone = (o) => JSON.parse(JSON.stringify(o));
  const slotOf = (id) => st.slots.indexOf(id) + 1;
  function mkProfiles() {
    if (st.lib2) return;
    st.lib2 = {}; st.pn = 0; st.slots = [null, null, null, null, null]; st.auto2 = { enabled: false, fallback: "", target: 0 };
    const mk = (id, name, prof) => {
      const buttons = {};
      for (const k of Object.keys(names).map(Number)) buttons["0x" + k.toString(16).padStart(2, "0")] = { normal: btn[`${prof}/${k}/false`], hypershift: btn[`${prof}/${k}/true`] };
      for (const k of [3, 9, 10, 0x0c, 0x34, 0x35]) buttons["0x" + k.toString(16).padStart(2, "0")] = { normal: A({ kind: "mouse", button: { 3: 3, 9: 9, 10: 10, 12: 1, 52: 9, 53: 10 }[k] }), hypershift: null };
      buttons["0x0c"] = { normal: A({ kind: "dpi", code: 2 }), hypershift: null };
      st.lib2[id] = { format: "open-rzr-profile", version: 1, id, name, device: "Razer Naga Pro (wired)", pid: 0x8f, buttons, dpi_stages: clone(st.stages[1]), polling: 1000, lighting: {}, macros: {}, links: [] };
    };
    mk("win-pc-default", "WIN-PC-Default", 1); mk("profile-2", "Profile 2", 2);
    if (st.db === "probe") {
      // Basilisk-like: no DPI buttons behind the wheel, no plates; a clutch and a wheel-mode button
      for (const p of Object.values(st.lib2)) {
        const keep = ["0x01", "0x02", "0x03", "0x04", "0x05", "0x09", "0x0a", "0x34", "0x35"];
        for (const k of Object.keys(p.buttons)) if (!keep.includes(k)) delete p.buttons[k];
        p.buttons["0x04"] = { normal: A({ kind: "mouse", button: 4 }), hypershift: null };
        p.buttons["0x05"] = { normal: A({ kind: "mouse", button: 5 }), hypershift: null };
        p.buttons["0x0f"] = { normal: A({ kind: "dpi", code: 5, dpi_x: 400, dpi_y: 400 }), hypershift: null };
        p.buttons["0x6a"] = { normal: A({ kind: "scroll_mode" }), hypershift: null };
        p.device = "Razer Basilisk V3"; p.pid = 0x99;
      }
    }
    st.slots[0] = "win-pc-default"; st.slots[1] = "profile-2"; st.cur = "win-pc-default";
  }
  st.written = st.written || {};
  const fp = (p) => JSON.stringify([p.buttons, p.dpi_stages]);
  const pinfo = () => ({ list: Object.values(st.lib2).map((p) => { const slots = st.slots.map((x, i) => (x === p.id ? i + 1 : 0)).filter(Boolean);
      slots.forEach((n) => { if (st.written[n] === undefined) st.written[n] = fp(p); });
      return { id: p.id, name: p.name, links: p.links, slots, written: slots.length ? slots.every((n) => st.written[n] === fp(p)) : null }; }),
    current: st.cur, slots: st.slots, flash: [92160, 91912, 248], autoswitch: st.auto2, linking: true, autowrite: !!st.autowrite });
  // tiny stand-in for macro_doc::compile (loops unrolled, nested macros inlined, ASCII text)
  const mcompile = (name, items, software) => {
    const flat = [];
    let err = "";
    const run = (its, stack) => {
      let depth = 0;
      for (const it of its) { if (!it.on) continue; if (it.kind === "loop_start") depth++; if (it.kind === "loop_end") depth--; if (depth < 0) { err = "[loop_end|] \"End of loop\" without a loop"; return; } }
      if (depth) { err = "[loop_open|] a loop has no end"; return; }
      for (let i = 0; i < its.length && !err; i++) {
        const it = its[i];
        if (!it.on) continue;
        if (it.kind === "loop_start") {
          let d = 0, j = i;
          for (; j < its.length; j++) { if (!its[j].on) continue; if (its[j].kind === "loop_start") d++; if (its[j].kind === "loop_end" && --d === 0) break; }
          for (let k = 0; k < Math.max(1, it.extra); k++) run(its.slice(i + 1, j), stack);
          i = j; continue;
        }
        if (it.kind === "loop_end") continue;
        if (it.kind === "macro") { if (stack.includes(it.text)) { err = `[self|${it.text}] macro "${it.text}" contains itself`; return; } const m = st.lib[it.text]; if (!m) { err = `[missing|${it.text}] macro "${it.text}" not found`; return; } run(m.items, [...stack, it.text]); continue; }
        if (it.kind === "text") { if (!/^[\x20-\x7E\n\t]*$/.test(it.text)) { err = "[text|Привет] the mouse can type only Latin letters"; return; } for (const _ of it.text) flat.push({ kind: "key_down", value: 4 }, { kind: "delay", value: 10 }, { kind: "key_up", value: 4 }); continue; }
        if (["key_press", "mouse_click", "media"].includes(it.kind)) { flat.push({ kind: "key_down", value: it.value }, { kind: "delay", value: it.extra || 20 }, { kind: "key_up", value: it.value }); continue; }
        flat.push({ kind: it.kind === "mouse_down" || it.kind === "mouse_up" ? "mouse" : it.kind, value: it.value });
      }
    };
    run(items, [name]);
    const used = Object.entries(st.lib).filter(([n]) => n !== name).map(([, m]) => m.trigger).filter(Boolean);
    const trigger = software.some((a) => a.on) ? (st.lib[name] && st.lib[name].trigger) || ["F13", "F14", "F15"].find((k) => !used.includes(k)) : "";
    if (trigger) flat.push({ kind: "key_down", value: 0x68 }, { kind: "delay", value: 20 }, { kind: "key_up", value: 0x68 });
    const size = flat.reduce((a, s) => a + (s.kind === "delay" && s.value > 255 ? 3 : 2), 0);
    if (!err && size > 4096) err = `[too_big|${size}] the macro takes ${size} bytes`;
    const lines = items.map((i) => i.kind);
    if (err) return { ok: false, error: err, steps: 0, size: 0, ms: 0, trigger: "", lines, flat: [] };
    return { ok: true, error: "", steps: flat.length, size, ms: flat.filter((s) => s.kind === "delay").reduce((a, s) => a + s.value, 0), trigger: trigger || "", lines, flat };
  };
  const H = {
    app_info: () => ({ version: "0.5.0", hotplug: true, os: window.__OS__ || "windows", log_file: "C:\\Users\\us\\AppData\\Roaming\\open-rzr\\logs\\open-rzr.log", config_dir: "x", mock: true, log_level: "DEBUG", openrazer_db: "477f4d6 2026-10-05", db_models: 117 }),
    tables: () => ({
      kinds: [["keyboard", "Клавиша / сочетание"], ["keyboard_turbo", "Турбо-клавиша"], ["mouse", "Кнопка мыши"], ["double_click", "Двойной клик"], ["mouse_turbo", "Турбо-клик"], ["macro", "Макрос"], ["dpi", "Переключение DPI"], ["profile", "Переключение профиля"], ["consumer", "Медиа-клавиша"], ["hypershift", "Hypershift"], ["system", "Питание ПК"], ["scroll_mode", "Режим колеса"], ["disabled", "Отключена"], ["raw", "Сырые байты"]],
      dpi_ops: [[1, "DPI: следующая"], [2, "DPI: предыдущая"], [6, "по кругу"], [7, "назад"], [3, "стадия"], [5, "Снайпер"]],
      profile_ops: [[1, "след"], [2, "пред"], [4, "по кругу"], [5, "назад"], [3, "Конкретный профиль"]],
      system_ops: [[1, "Выкл"], [2, "Сон"], [4, "Пробуждение"]],
      macro_modes: [["once", "N раз"], ["hold", "пока удерживается"], ["toggle", "вкл/выкл"], ["sequence", "по шагам"]],
      mouse_fn: [[1, "ЛКМ"], [2, "ПКМ"], [3, "Средняя"], [4, "Назад"], [5, "Вперёд"]],
      consumer: [[0xcd, "Play/Pause"], [0xe9, "Громкость +"]], keys: KEYS,
      zones: [["scroll", "Scroll wheel"], ["logo", "Logo"], ["side", "Side panel"], ["left", "Left side"], ["right", "Right side"], ["backlight", "Backlight"], ["matrix", "Device lighting"]],
      effects: [["off", "Выключено"], ["static", "Статичный цвет"], ["spectrum", "Спектр"], ["wave", "Волна"], ["breath_single", "Дыхание 1"], ["breath_dual", "Дыхание 2"], ["breath_random", "Дыхание случ."], ["reactive", "Реакция"], ["custom", "Свои цвета"]],
      zone_effects: { scroll: ["off", "static", "spectrum", "wave", "breath_single", "breath_dual", "breath_random", "reactive"], logo: ["off", "static", "spectrum", "wave", "breath_single", "breath_dual", "breath_random", "reactive"], side: ["off", "static", "custom", "spectrum", "breath_single", "breath_dual", "breath_random", "reactive"], left: ["off", "static", "spectrum", "wave", "breath_single", "breath_dual", "breath_random", "reactive"], right: ["off", "static", "spectrum", "wave", "breath_single", "breath_dual", "breath_random", "reactive"], backlight: ["off", "static", "spectrum", "wave", "breath_single", "breath_dual", "breath_random", "reactive"], matrix: ["off", "static", "spectrum", "wave", "breath_single", "breath_dual", "breath_random", "reactive"] },
    }),
    list_devices: () => st.devicesGone ? [] : st.db ? [{ label: "Razer Basilisk V3", pid: 0x99, candidates: [{ path: "/dev/hidraw3", vid: 0x1532, pid: 0x99, interface: 0, name: "Razer Basilisk V3", backend: "hidraw", usage_page: 0, usage: 0 }] }] : [{ label: "Razer Naga Pro (кабель)", pid: 0x8f, candidates: [{ path: "\\\\?\\hid#vid_1532&pid_008f&mi_00", vid: 0x1532, pid: 0x8f, interface: 0, name: "Razer Naga Pro", backend: "hidapi", usage_page: 1, usage: 2 }] }],
    connect: ({ candidates }) => { if (!Array.isArray(candidates) || !candidates[0].path) throw "bad candidates"; log("device", "подключено");
      const ALLF = ["firmware", "serial", "dpi", "dpi_stages", "polling", "battery", "charging", "idle", "low_battery", "lighting", "buttons", "macros", "profiles", "side_plate"];
      const ZE = { scroll: ["off", "static", "spectrum", "wave", "breath_single", "breath_dual", "breath_random", "reactive"], logo: ["off", "static", "spectrum", "wave", "breath_single", "breath_dual", "breath_random", "reactive"], side: ["off", "static", "custom", "spectrum", "breath_single", "breath_dual", "breath_random", "reactive"] };
      if (st.db) return { name: "Razer Basilisk V3", pid: 0x99, path: candidates[0].path, verified: false, source: "openrazer", experimental: st.db === "probe" ? ["buttons", "macros", "profiles"] : [], features: ["firmware", "serial", "dpi", "dpi_stages", "polling", "lighting", ...(st.db === "probe" ? ["buttons", "macros", "profiles"] : [])], openrazer: st.or.bridged ? "/sys/bus/hid/drivers/razermouse/0003:1532:0099.0004" : null,
        zone_effects: { logo: ZE.logo, scroll: ["off", "static", "spectrum", "breath_single", "breath_dual", "breath_random", "reactive"] }, wireless: false, has_battery: false, max_dpi: 26000, max_stages: 5, lighting: ["scroll", "logo"], firmware: "v1.3", serial: "PM21", dpi: st.dpi, stages: st.stages[1], poll: 1000, battery: null, charging: false, idle: null, low: null, profiles: [1], side_plates: false, plate: null, plate_choice: "auto" };
      return { pid: 0x8f, verified: true, features: ALLF, openrazer: st.or.bridged ? "/sys/bus/hid/drivers/razermouse/0003:1532:008F.0005" : null, zone_effects: ZE, name: "Razer Naga Pro (кабель)", path: candidates[0].path, wireless: false, has_battery: true, max_dpi: 20000, max_stages: 5, lighting: ["scroll", "logo", "side"], firmware: "v1.5", serial: "PM2041H0001", dpi: st.dpi, stages: st.stages[1], poll: 1000, battery: 78, charging: false, idle: 300, low: 15, profiles: [1, 2, 3, 4, 5], side_plates: true, plate: st.plate, plate_choice: st.choice }; },
    battery: () => { if (st.failBattery) throw "[disconnected] HIDIOCGFEATURE: No such device"; return [77, true]; },
    get_settings: () => ({ language: st.lang, autostart: st.auto.enabled, start_minimized: st.auto.minimized, close_to_tray: st.closeTray, log_level: "DEBUG", ui_style: st.style }),
    set_ui_style: ({ style }) => { st.style = style; return null; },
    set_language: ({ language }) => { st.lang = language; return null; },
    set_autostart: ({ enabled, minimized }) => { st.auto = { enabled, minimized }; return st.auto; },
    set_close_to_tray: ({ enabled }) => { st.closeTray = enabled; return null; },
    default_action: ({ id }) => (id >= 0x40 && id <= 0x4b ? A({ kind: "keyboard", key: [0x1e,0x1f,0x20,0x21,0x22,0x23,0x24,0x25,0x26,0x27,0x2d,0x2e][id - 0x40] }) : null),
    quit_app: () => null,
    button_names: () => st.names,
    install_udev: () => null,
    set_dpi_stages: ({ stages, active, profile }) => { st.stages[profile] = { active, stages }; st.dpi = stages[active - 1]; return st.dpi; },
    get_dpi_stages: ({ profile }) => st.stages[profile] || { active: 1, stages: [[800, 800]] },
    set_polling: ({ hz }) => { if (![125, 500, 1000].includes(hz)) throw "hz"; return null; },
    set_power: ({ idle, low }) => { st.power = [idle, low]; return null; },
    scan_buttons: ({ profile, hypershift, all, plate }) => {
      const p = profile === 0 ? 0 : profile;
      const plateOf = (id) => (id >= 0x40 && id <= 0x4b ? 12 : id >= 0x50 && id <= 0x55 ? 6 : id === 4 || id === 5 ? 2 : null);
      st.lastPlate = plate;
      return Object.keys(names).map(Number).sort((a, b) => a - b)
        .filter((id) => !plate || plateOf(id) == null || plateOf(id) === plate)
        .map((id) => ({ id, name: names[id], action: btn[`${p}/${id}/${hypershift}`] }));
    },
    side_plate: () => st.plate,
    set_plate_choice: ({ choice }) => { st.choice = choice; return null; },
    set_button: ({ id, action, profile, hypershift }) => { if (typeof action.kind !== "string") throw "action"; btn[`${profile}/${id}/${hypershift}`] = A(action); log("device", "set_button 0x" + id.toString(16)); return A(action); },
    rename_button: ({ id, name }) => { st.names[id.toString(16).padStart(2, "0")] = name; return null; },
    copy_profile: ({ from, to }) => 60,
    describe_action: ({ action }) => describe(action),
    macros_on_mouse: () => ({ macros: Object.entries(st.macros).map(([id, steps]) => ({ id: +id, steps, text: [] })), flash: [92160, 90000, 0] }),
    check_macro: ({ name, items, software }) => mcompile(name, items, software),
    write_macro: ({ id, name, items, software }) => { const c = mcompile(name, items, software); if (!c.ok) throw "[invalid] " + c.error; st.macros[id] = c.flat; st.lib[name] = { id, items: clone(items), software: clone(software), trigger: c.trigger }; return { bytes: c.size, trigger: c.trigger }; },
    delete_macro: ({ id }) => { delete st.macros[id]; return null; },
    macro_library: () => Object.entries(st.lib).map(([name, m]) => ({ name, id: m.id, items: clone(m.items), software: clone(m.software || []), trigger: m.trigger || "" })),
    save_macro_lib: ({ name, id, items, software }) => { const c = mcompile(name, items, software); st.lib[name] = { id, items: clone(items), software: clone(software), trigger: c.trigger }; return c.ok ? "" : c.error; },
    rename_macro_lib: ({ old, name }) => { if (st.lib[name]) throw "[invalid] name is empty or taken"; st.lib[name] = st.lib[old]; delete st.lib[old]; return null; },
    delete_macro_lib: ({ name }) => { delete st.lib[name]; return null; },
    describe_steps: ({ steps }) => [steps.map((s) => s.kind), steps.length * 2],
    test_program_actions: ({ software }) => { st.tested = clone(software); return null; },
    program_actions_status: () => ({ status: { supported: true, active: true, note: "1 trigger keys" }, bound: Object.entries(st.lib).filter(([, m]) => m.trigger).map(([n, m]) => [m.trigger, n]) }),
    profile_layer: ({ id, op }) => { const p = st.lib2[id]; let n = 0; for (const b of Object.values(p.buttons)) { if (b.normal.kind === "hypershift") continue; b.hypershift = op === "copy" ? clone(b.normal) : A({ kind: "disabled" }); n++; } return n; },
    get_lighting: () => ({ saved: st.saved, brightness: st.bright }),
    set_lighting: ({ targets }) => { for (const [z, s] of targets) { if (!Array.isArray(s.color1) || s.leds.length !== 3) throw "zone state"; st.saved[z] = s; st.bright[z] = s.brightness; } return null; },
    backup: () => "C:\\Users\\us\\AppData\\Roaming\\open-rzr\\backups\\naga-1.json",
    list_backups: () => [{ name: "naga-1.json", path: "C:\\x\\naga-1.json", size: 20480 }],
    restore: ({ path }) => 120,
    open_backup_folder: () => null,
    logs_since: ({ since }) => ({ seq: st.seq, lines: st.log.slice(Math.max(0, st.log.length - (st.seq - since))) }),
    set_log_level: ({ level }) => null,
    open_log_folder: () => null,
    ui_log: ({ level, msg }) => { log("ui", msg); return null; },
    save_report: () => "C:\\x\\report.txt",
    raw_command: ({ cmd, args, size }) => { st.lastRaw = { cmd, args, size }; return "01"; },
    profiles: () => { mkProfiles(); return pinfo(); },
    profile_get: ({ id }) => clone(st.lib2[id]),
    profile_restore: ({ profile }) => { st.lib2[profile.id] = clone(profile); st.restored = (st.restored || 0) + 1; return clone(profile); },
    profile_select: ({ id }) => { st.cur = id; log("ui", "select " + id); return clone(st.lib2[id]); },
    profile_set_button: ({ id, button, hypershift, action }) => { const p = st.lib2[id]; const k = "0x" + button.toString(16).padStart(2, "0"); const b = p.buttons[k] || (p.buttons[k] = { normal: A(action), hypershift: null }); if (hypershift && b.normal.kind === "hypershift") throw "[invalid] this button holds Hypershift"; const wasMod = b.normal.kind === "hypershift"; if (hypershift) b.hypershift = A(action); else b.normal = A(action); if (!hypershift && action.kind === "hypershift") b.hypershift = A(action); else if (!hypershift && wasMod) b.hypershift = A({ kind: "disabled" }); st.lastWrite = { id, button, hypershift, slot: slotOf(id) }; return A(action); },
    profile_set_dpi: ({ id, stages, active }) => { st.lib2[id].dpi_stages = { active, stages }; st.dpi = stages[active - 1]; return st.dpi; },
    profile_create: ({ name, from }) => { const base = from === "current" ? clone(st.lib2[st.cur]) : clone(st.lib2["win-pc-default"]); const id = "p" + (++st.pn); st.lib2[id] = { ...base, id, name, links: [] }; return id; },
    profile_rename: ({ id, name }) => { st.lib2[id].name = name; return null; },
    profile_duplicate: ({ id }) => { const id2 = "p" + (++st.pn); st.lib2[id2] = { ...clone(st.lib2[id]), id: id2, name: st.lib2[id].name + " (copy)", links: [] }; return id2; },
    profile_delete: ({ id }) => { delete st.lib2[id]; st.slots = st.slots.map((x) => (x === id ? null : x)); if (st.cur === id) st.cur = ""; return null; },
    profile_reset: ({ id }) => { const p = st.lib2[id]; for (const [k, v] of Object.entries(p.buttons)) { const d = H.default_action({ id: parseInt(k, 16) }); if (d) { v.normal = d; v.hypershift = d; } } return clone(p); },
    profile_reload: ({ id }) => clone(st.lib2[id]),
    profile_export: ({ id }) => "C:\\Users\\us\\Downloads\\" + st.lib2[id].name.toLowerCase() + ".openrzr.json",
    profile_import: ({ text }) => { const p = JSON.parse(text); if (p.format !== "open-rzr-profile") throw "[invalid] not an open-rzr profile"; const id = "p" + (++st.pn); st.lib2[id] = { ...p, id }; return id; },
    open_folder_of: ({ path }) => null,
    slot_assign: ({ slot, id }) => { st.slots[slot - 1] = id; st.written[slot] = id ? fp(st.lib2[id]) : undefined; return pinfo(); },
    profile_write: ({ id }) => { st.slots.forEach((x, i) => { if (x === id) st.written[i + 1] = fp(st.lib2[id]); }); st.writes = (st.writes || 0) + 1; return pinfo(); },
    set_autowrite: ({ on }) => { st.autowrite = on; return pinfo(); },
    all_hid_devices: () => [
      { path: "\\\\?\\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&021532_PID&0092&Col06", vid: 0x1532, pid: 0x92, interface: -1, name: "Naga Pro", backend: "hidapi", usage_page: 0xff00, usage: 1, bluetooth: true },
      { path: "\\\\?\\HID#VID_1532&PID_008F&MI_00", vid: 0x1532, pid: 0x8f, interface: 0, name: "Razer Naga Pro", backend: "hidapi", usage_page: 1, usage: 2, bluetooth: false },
      { path: "\\\\?\\HID#VID_046D&PID_C52B", vid: 0x046d, pid: 0xc52b, interface: 2, name: "USB Receiver", backend: "hidapi", usage_page: 0xff00, usage: 1, bluetooth: false }],
    wireless_debug: ({ probe }) => ({ address: "eac88650b167", battery: 87, notes: [],
      hid: [1, 2, 3, 4, 5, 6].map((n) => ({ path: `\\\\?\\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&021532_PID&0092_REV&0001_eac88650b167&Col0${n}#9`, usage_page: n === 5 ? 0xff00 : n === 6 ? 1 : 0x0c, usage: n === 6 ? 6 : 1, input_len: 9, output_len: n === 5 ? 21 : 0, feature_len: n === 5 ? 21 : 0, input_ids: [n], output_ids: n === 5 ? [5] : [], feature_ids: n === 5 ? [5] : [], descriptor: "", error: "" })),
      probes: probe ? [{ path: "…&Col05#9", method: "feature id 0x05", ok: false, detail: "error 1: incorrect function (the collection has no such report)" }] : [],
      gatt: [{ uuid: "180f", name: "Battery", handle: 12, chars: [{ uuid: "2a19", name: "Battery Level", handle: 13, props: "read notify", value: "57" }] },
        { uuid: "52401523-f97c-7f90-0e7f-6c6f4e36db1c", name: "vendor-specific", handle: 40, chars: [{ uuid: "52401524-f97c-7f90-0e7f-6c6f4e36db1c", name: "vendor-specific", handle: 41, props: "write", value: "" }, { uuid: "52401525-f97c-7f90-0e7f-6c6f4e36db1c", name: "vendor-specific", handle: 43, props: "notify", value: "" }] }],
      gatt_error: "" }),
    wireless_gatt: ({ request, listenMs }) => ({ request: request === "fw" ? "00 1f 00 00 00 02 00 81 00 00" : request, error: "",
      steps: ["service interface \\\\?\\BTHLEDevice#{52401523-f97c-7f90-0e7f-6c6f4e36db1c}_Dev_VID&021532_PID&0092_REV&0001_eac88650b167", "char 52401524-… handle 60 value handle 61 write 1", "subscribe 52401525-…: OK", "subscribe 52401526-…: OK", "listen to 2 characteristics: OK", "write 90 bytes to 52401524-…: OK", "read 52401525-…: 00"],
      notifications: request ? [{ t_ms: 42, handle: 64, uuid: "52401525-f97c-7f90-0e7f-6c6f4e36db1c", hex: "02 1f 00 00 00 02 00 81 02 01", len: 20 }] : [] }),
    hide_to_tray: () => { st.hidden = true; return null; },
    close_cancelled: () => { st.cancelled = true; return null; },
    ui_state_get: () => st.ui || "",
    ui_state_set: ({ state }) => { st.ui = state; return null; },
    device_files: () => ({ folder: "C:\\Users\\us\\AppData\\Roaming\\open-rzr\\devices", files: [
      { file: "C:\\x\\devices\\viper.toml", pid: 0x007a, name: "Razer Viper (mine)", base: "openrazer", error: "", warnings: ["unknown key “colour” (ignored)"] },
      { file: "C:\\x\\devices\\bad.json", pid: 0, name: "", base: "", error: "line 3: expected `,` or `}`", warnings: [] }] }),
    razer_drivers: () => [
      { device: "Razer Naga Pro", instance: "HID\\VID_1532&PID_008F&MI_00\\8&1", provider: "Razer Inc", version: "10.0.15063.0", inf: "oem42.inf", service: "mouhid", filters: "", razer: true },
      { device: "USB Input Device", instance: "USB\\VID_1532&PID_008F&MI_00\\7&3", provider: "Microsoft", version: "10.0.26100.1", inf: "input.inf", service: "HidUsb", filters: "", razer: false }],
    turbo_media: ({ code, interval }) => { const id = 0xf000; st.macros[id] = [{ kind: "consumer", value: code }, { kind: "delay", value: 10 }, { kind: "consumer", value: 0 }, { kind: "delay", value: interval - 10 }]; return id; },
    profile_links: ({ id, links }) => { st.lib2[id].links = links; return null; },
    running_programs: () => [{ name: "chrome.exe", path: "C:\\Program Files\\Chromium\\Application\\chrome.exe", title: "Chromium" }, { name: "witcher3.exe", path: "D:\\Games\\The Witcher 3\\bin\\x64\\witcher3.exe", title: "The Witcher 3" }, { name: "audacity.exe", path: "C:\\Program Files\\Audacity\\audacity.exe", title: "Audacity" }],
    set_autoswitch: ({ enabled, fallback, target }) => { st.auto2 = { enabled, fallback, target }; return null; },
    razer_software: () => ({ supported_os: true, services: st.rz.services, processes: st.rz.processes, can_restore: st.rz.saved }),
    stop_razer_software: ({ autostart }) => { if (st.rz.denyUac) throw "[no_access] Administrator rights were not granted"; st.rz.services.forEach((x) => { x.running = false; }); st.rz.processes = []; st.rz.saved = true; st.rz.autostart = autostart; return H.razer_software(); },
    start_razer_software: () => { st.rz.services.forEach((x) => { x.running = true; }); st.rz.saved = false; return H.razer_software(); },
    openrazer_status: () => ({ supported_os: st.or.os, driver_loaded: st.or.driver, daemon_running: st.or.daemon, bound: st.or.bridged ? [[0x8f, "/sys/bus/hid/drivers/razermouse/0003:1532:008F.0005"]] : [], mode: st.or.mode, polychromatic_installed: !!window.__POLY__, polychromatic_running: !!window.__POLY__ && st.polyRun }),
    open_polychromatic: () => { st.polyRun = true; return null; },
    set_openrazer_mode: ({ mode }) => { if (!["auto", "on", "off"].includes(mode)) throw "[invalid] mode"; st.or.mode = mode; return null; },
    stop_openrazer_daemon: () => { st.or.daemon = false; return null; },
    supported_devices: () => [
      { pid: 0x8f, name: "Razer Naga Pro (wired)", verified: true, direct: ["firmware", "dpi", "buttons", "macros"], openrazer: [] },
      { pid: 0x99, name: "Razer Basilisk V3", verified: false, direct: ["firmware", "dpi", "polling", "lighting"], openrazer: [] },
      { pid: 0x96, name: "Razer Naga X", verified: false, direct: ["firmware", "dpi"], openrazer: ["serial", "dpi_stages"] }],
  };
  const listeners = {};
  window.__EMIT__ = (name, payload) => (listeners[name] || []).forEach((f) => f({ payload }));
  window.__TAURI__ = { event: { listen: async (name, f) => { (listeners[name] = listeners[name] || []).push(f); return () => {}; } }, core: { invoke: async (cmd, args) => {
    args = args || {};
    const want = (window.__SIGS__[cmd] || null);
    if (!want) throw `UNKNOWN COMMAND ${cmd}`;
    const got = Object.keys(args).sort().join(","), exp = [...want].sort().join(",");
    if (got !== exp) throw `CONTRACT ${cmd}: got {${got}} want {${exp}}`;
    st.calls.push(cmd);
    await new Promise((r) => setTimeout(r, 5));
    return H[cmd](args);
  } } };
  window.__ST__ = st;
})();
