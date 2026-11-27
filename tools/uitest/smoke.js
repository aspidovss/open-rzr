// Smoke test of the window against the mock (no mouse, no Tauri): every page opens, the main
// dialogs work, undo, close-to-tray and the raw command behave; no JavaScript errors anywhere.
//
//   npm --prefix tools/uitest install        (once: Playwright)
//   npx --prefix tools/uitest playwright install chromium
//   node tools/uitest/build.js && node tools/uitest/smoke.js [--shots]
//
// Exit code 1 on the first failed check. With --shots, screenshots go to tools/uitest/shots/.
const path = require("path");
const fs = require("fs");
const { chromium } = require("playwright");

const http = require("http");

// The help pages are fetched by the window, which a file:// page may not do: serve the repository
// over http on a free local port instead.
const root = path.resolve(__dirname, "..", "..");
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".md": "text/markdown", ".png": "image/png", ".svg": "image/svg+xml", ".json": "application/json" };
const server = http.createServer((req, res) => {
  const rel = decodeURIComponent(req.url.split("?")[0]).replace(/^\/+/, "");
  const file = path.resolve(root, rel === "favicon.ico" ? "app/icons/icon.ico" : rel);
  if (!file.startsWith(root) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); return res.end(); }
  res.writeHead(200, { "Content-Type": TYPES[path.extname(file)] || "application/octet-stream" });
  fs.createReadStream(file).pipe(res);
});
let page = "";
const shots = process.argv.includes("--shots");
let failed = 0;

function check(name, ok, detail = "") {
  console.log(`${ok ? "ok  " : "FAIL"}  ${name}${detail ? "  — " + detail : ""}`);
  if (!ok) failed++;
}

async function open(browser, query, ready = "#fig .lbl") {
  const errors = [];
  const p = await (await browser.newContext({ viewport: { width: 1200, height: 860 } })).newPage();
  p.on("pageerror", (e) => errors.push(e.message));
  p.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  await p.goto(page + "?" + query);
  await p.waitForFunction((selector) => document.querySelectorAll(selector).length > 5, ready, { timeout: 15000 });
  return { p, errors };
}

async function shot(p, name) {
  if (!shots) return;
  fs.mkdirSync(path.join(__dirname, "shots"), { recursive: true });
  await p.screenshot({ path: path.join(__dirname, "shots", name + ".png") });
}

(async () => {
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  page = `http://127.0.0.1:${server.address().port}/ui/test.html`;
  const browser = await chromium.launch({ channel: process.env.PLAYWRIGHT_CHANNEL || undefined });
  for (const lang of ["en", "ru"]) {
    const { p, errors } = await open(browser, "lang=" + lang);

    for (const width of [1140, 920]) {
      await p.setViewportSize({ width, height: 780 });
      const layout = await p.evaluate(() => {
        const profile = document.querySelector("#prof-box").getBoundingClientRect();
        const nav = document.querySelector("#nav");
        const tabs = nav.getBoundingClientRect();
        const bar = document.querySelector("#devbar").getBoundingClientRect();
        return {
          singleRow: profile.right <= tabs.left && profile.top < tabs.bottom && tabs.top < profile.bottom
            && tabs.right <= bar.right,
          allTabsVisible: nav.scrollWidth === nav.clientWidth,
        };
      });
      check(`${lang}: profile and device tabs stay on one row at ${width}px`, layout.singleRow);
      if (width === 1140) check(`${lang}: all device tabs fit at the default width`, layout.allTabsVisible);
    }
    await p.setViewportSize({ width: 1200, height: 860 });

    // every tab, and every page of the mouse tab
    const tabs = await p.$$eval("#tabs button[data-tab]", (b) => b.map((x) => x.dataset.tab));
    for (const tab of tabs) {
      await p.click(`#tabs button[data-tab="${tab}"]`);
      await p.waitForTimeout(150);
      const shown = await p.$$eval(".page:not(.hidden)", (v) => v.map((x) => x.id));
      check(`${lang}: tab ${tab}`, shown.length === 1, shown.join(","));
      await shot(p, `${lang}-${tab}`);
    }
    const pages = await p.$$eval("#nav button[data-page]", (b) => b.map((x) => x.dataset.page));
    for (const pg of pages) {
      await p.evaluate((x) => showPage(x), pg);
      await p.waitForTimeout(150);
      check(`${lang}: page ${pg}`, await p.isVisible("#page-" + pg));
      await shot(p, `${lang}-page-${pg}`);
    }

    // diagnostics dialogs
    await p.click('#tabs button[data-tab="diag"]');
    for (const id of ["#btn-hidlist", "#btn-drivers", "#btn-devfiles"]) {
      await p.click(id);
      await p.waitForSelector("#modal:not(.hidden)", { timeout: 5000 });
      await p.waitForTimeout(150);
      check(`${lang}: dialog ${id}`, await p.isVisible("#modal:not(.hidden)"));
      await shot(p, `${lang}-dialog-${id.slice(5)}`);
      await p.keyboard.press("Escape");
      await p.evaluate(() => { const m = document.querySelector("#modal"); if (m && !m.classList.contains("hidden")) m.querySelector(".buttons button.primary, .buttons button:last-child")?.click(); });
      await p.waitForTimeout(150);
    }

    // Wireless view: known commands, a scan, the raw GATT channel; back to the log
    await p.click("#btn-wireless"); await p.waitForSelector("#diag-wl-view:not(.hidden) .card.wl");
    check(`${lang}: wireless view replaces the log`, !(await p.isVisible("#log")));
    await p.click('#diag-wl-view button:has-text("' + (lang === "en" ? "Read what is known" : "Прочитать известное") + '")');
    await p.waitForFunction(() => document.querySelectorAll("#diag-wl-view tr.ok").length >= 4, null, { timeout: 5000 });
    check(`${lang}: known commands decoded`, /800×800/.test(await p.textContent("#diag-wl-view")));
    await p.evaluate(() => { WL.scanCls = "00-02"; });
    await p.evaluate(() => { window.__realConfirm = confirmBox; window.confirmBox = async () => true; });
    await p.click('#diag-wl-view button:has-text("' + (lang === "en" ? "Start" : "Начать") + '")');
    await p.waitForFunction(() => window.__ST__.bleCalls >= 12, null, { timeout: 5000 });
    await p.evaluate(() => { window.confirmBox = window.__realConfirm; });
    await p.waitForTimeout(200);
    check(`${lang}: scan runs class by class`, await p.evaluate(() => WL.ble.length === 9 + 96));
    check(`${lang}: "not supported" hidden`, (await p.$$eval("#diag-wl-view .card.wl:first-of-type tr", (r) => r.length)) < 20);
    await shot(p, `${lang}-wireless`);
    // write self-test: 3 stages, the user confirms the key F
    await p.click('#diag-wl-view button:has-text("' + (lang === "en" ? "Run the self-test" : "Запустить самопроверку") + '")');
    // the test button is chosen from the mouse's buttons: side button 12 first; take side button 1
    await p.waitForSelector("#wl-test-btn", { timeout: 5000 });
    check(`${lang}: test button offered, side 12 first`, (await p.$$eval("#wl-test-btn option", (o) => o.map((x) => +x.value))).join(",") === "75,64,5,3");
    await p.selectOption("#wl-test-btn", "64");
    await p.click('#modal:not(.hidden) button.primary');
    await p.waitForSelector("#modal:not(.hidden) .mono", { timeout: 5000 });
    check(`${lang}: press dialog names the chosen button`, (await p.textContent("#modal h2")).includes(await p.evaluate(() => buttonName(0x40))));
    // variant 1 (active profile 2): "=", variant 2 (live profile 1): "7" (found, stops), then the check after restore: "="
    for (const key of ["=", "7", "="]) {
      await p.waitForSelector("#modal:not(.hidden) .mono", { timeout: 5000 });
      await p.waitForTimeout(100);
      await p.keyboard.press(key === "=" ? "Equal" : "Digit7"); await p.waitForTimeout(450);
    }
    // stage 5: agree, "switch the mouse off and on", continue, press the button
    const ok5 = lang === "en" ? "Check" : "Проверить", go5 = lang === "en" ? "Continue" : "Продолжить";
    await p.click(`#modal:not(.hidden) button:has-text("${ok5}")`); await p.waitForTimeout(200);
    await p.click(`#modal:not(.hidden) button:has-text("${go5}")`); await p.waitForTimeout(200);
    await p.waitForSelector("#modal:not(.hidden) .mono", { timeout: 5000 }); await p.waitForTimeout(100);
    await p.keyboard.press(lang === "en" ? "Digit7" : "Digit6"); await p.waitForTimeout(450);
    check(`${lang}: self-test ran all steps`, (await p.evaluate(() => (window.__ST__.selftest || []).join(","))) === "buttons,sleep,select:64,targets,target_write:2f,target_write:2orig,target_write:1f,target_write:1orig,target_write:1hs_f,target_write:1hs_orig,target_write:1f,read_target,target_write:1orig",
      await p.evaluate(() => (window.__ST__.selftest || []).join(",")));
    check(`${lang}: self-test verdict matches the key pressed`, await p.evaluate((passed) => WL.test.stages[4].ok === passed && wirelessText().includes(t(passed ? "wl.t_all_ok" : "wl.t_some_failed")), lang === "en"));
    check(`${lang}: Hypershift and off/on stages reported`, await p.evaluate((passed) => WL.test.stages[3].note === t("wl.t_hs_ok") && WL.test.stages[4].note === t(passed ? "wl.t_persist_yes" : "wl.t_persist_read"), lang === "en"), await p.evaluate(() => WL.test.stages[3].note + " | " + WL.test.stages[4].note));
    check(`${lang}: self-test names the profile that works`, await p.evaluate(() => WL.test.stages[2].note.startsWith(t("wl.t_found", { p: "1" })) && !WL.test.stages[2].note.includes(t("wl.t_not_restored"))), await p.evaluate(() => WL.test.stages[2].note));
    await shot(p, `${lang}-selftest`);
    await p.click("#diag-views button[data-v=log]"); await p.waitForTimeout(100);
    check(`${lang}: back to the log`, await p.isVisible("#log"));

    // raw command: size in hex
    await p.fill("#raw-cmd", "028c"); await p.fill("#raw-args", "01 34 00");
    await p.fill("#raw-size", "zz"); await p.click("#btn-raw"); await p.waitForTimeout(100);
    check(`${lang}: raw size rejects "zz"`, (await p.textContent("#status")).startsWith("✗"));
    await p.fill("#raw-size", "0a"); await p.click("#btn-raw"); await p.waitForTimeout(150);
    check(`${lang}: raw size 0a = 10`, (await p.evaluate(() => window.__ST__.lastRaw && window.__ST__.lastRaw.size)) === 10);

    // undo of a button change
    await p.evaluate(() => showPage("buttons")); await p.waitForTimeout(100);
    const before = await p.evaluate(() => JSON.stringify(S.profile.buttons["0x41"].normal));
    await p.evaluate(() => undoable(0x41, () => setButton(0x41, { ...defaultAction(), kind: "disabled" })));
    await p.waitForTimeout(150);
    check(`${lang}: "off" offers undo`, await p.isVisible("#status .undo"));
    await p.click("#status .undo"); await p.waitForTimeout(200);
    check(`${lang}: undo restores the binding`, (await p.evaluate(() => JSON.stringify(S.profile.buttons["0x41"].normal))) === before);

    // undo of a profile reset
    await p.evaluate(() => setButton(0x40, { ...defaultAction(), kind: "keyboard", key: 4 }));
    await p.evaluate(() => { window.__realConfirm = confirmBox; window.confirmBox = async () => true; });
    await p.evaluate(() => resetProfile()); await p.waitForTimeout(200);
    await p.evaluate(() => { window.confirmBox = window.__realConfirm; });
    await p.click("#status .undo"); await p.waitForTimeout(200);
    check(`${lang}: undo of a profile reset`, (await p.evaluate(() => S.profile.buttons["0x40"].normal.key)) === 4);

    // close to tray: clean editor closes, dirty editor asks
    await p.evaluate(() => window.__EMIT__("close-requested", null)); await p.waitForTimeout(100);
    check(`${lang}: close to tray`, await p.evaluate(() => !!window.__ST__.hidden));
    await p.evaluate(() => { window.__ST__.hidden = false; setDirty(true); window.__EMIT__("close-requested", null); });
    await p.waitForTimeout(150);
    check(`${lang}: unsaved macro asks before closing`, await p.isVisible("#modal:not(.hidden)"));
    await p.click("#dialog .buttons button >> nth=0"); await p.waitForTimeout(100);
    check(`${lang}: cancel keeps the window`, await p.evaluate(() => !window.__ST__.hidden && !!window.__ST__.cancelled));

    check(`${lang}: no JavaScript errors`, errors.length === 0, errors.slice(0, 3).join(" | "));
    await p.context().close();
  }
  // first run: user agreement, then the update question; update check with a mocked GitHub
  for (const lang of ["en", "ru"]) {
    const errors = [];
    const p = await (await browser.newContext({ viewport: { width: 1200, height: 860 } })).newPage();
    p.on("pageerror", (e) => errors.push(e.message));
    p.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
    await p.route("https://api.github.com/**", (r) => r.fulfill({ contentType: "application/json",
      body: JSON.stringify({ tag_name: "v9.9.9", assets: [{ name: "open-rzr-9.9.9-windows-x64-setup.exe", size: 500 }], html_url: "https://github.com/aspidovss/open-rzr/releases/tag/v9.9.9", draft: false, prerelease: false }) }));
    await p.goto(page + "?firstrun&lang=" + lang);
    await p.waitForSelector("#modal:not(.hidden) pre.eula", { timeout: 10000 });
    await p.waitForFunction(() => document.querySelector("pre.eula").textContent.length > 500);
    const text = await p.textContent("pre.eula");
    check(`${lang}: agreement shown first`, /open-rzr/.test(text) && /orzr@aspidov\.ru/.test(text));
    await p.click(`#dialog .seg button[data-l="${lang === "en" ? "ru" : "en"}"]`); await p.waitForTimeout(200);
    check(`${lang}: agreement in the other language`, (await p.textContent("pre.eula")) !== text);
    await p.keyboard.press("Escape"); await p.waitForTimeout(100);
    check(`${lang}: Escape does not skip the agreement`, await p.isVisible("pre.eula"));
    await shot(p, `${lang}-first-agreement`);
    await p.click("#dialog .buttons button.primary"); await p.waitForTimeout(200);
    check(`${lang}: accepted`, await p.evaluate(() => !!window.__ST__.agreed));
    check(`${lang}: then the update question`, await p.isVisible("#modal:not(.hidden)") && !(await p.isVisible("pre.eula")));
    await shot(p, `${lang}-first-updates`);
    await p.click("#dialog .buttons button.primary"); await p.waitForTimeout(300);
    check(`${lang}: updates turned on`, await p.evaluate(() => window.__ST__.updates === true));
    await p.waitForFunction(() => document.querySelectorAll("#fig .lbl").length > 5, null, { timeout: 15000 });
    await p.waitForTimeout(500);
    check(`${lang}: new version banner`, await p.isVisible("#update-banner"), await p.textContent("#update-text"));
    await shot(p, `${lang}-update-banner`);
    await p.click("#btn-banner-download");
    await p.waitForFunction(() => document.querySelector("#update-text").textContent.includes("Downloads/"));
    check(`${lang}: release downloaded only on request`, await p.evaluate(() => window.__ST__.download.version === "9.9.9" && window.__ST__.download.size === 500));
    await p.click("#btn-update-notes"); await p.waitForTimeout(100);
    check(`${lang}: "What's new" opens the release page`, (await p.evaluate(() => window.__ST__.opened)) === "https://github.com/aspidovss/open-rzr/releases/tag/v9.9.9");
    await p.click("#btn-update-hide"); await p.waitForTimeout(100);
    check(`${lang}: hide this version`, !(await p.isVisible("#update-banner")) && (await p.evaluate(() => window.__ST__.skipped)) === "9.9.9");
    await p.click('#tabs button[data-tab="settings"]'); await p.waitForTimeout(100);
    check(`${lang}: setting is on`, await p.isChecked("#set-updates"));
    await p.click("#btn-update-check"); await p.waitForTimeout(400);
    check(`${lang}: download available in Settings`, await p.isVisible("#btn-update-download"));
    check(`${lang}: manual check`, /9\.9\.9/.test(await p.textContent("#update-state")));
    await p.click("#btn-eula"); await p.waitForTimeout(300);
    check(`${lang}: agreement can be read again`, await p.isVisible("pre.eula"));
    await p.click("#dialog .buttons button.primary"); await p.waitForTimeout(100);
    check(`${lang}: first run without JavaScript errors`, errors.length === 0, errors.slice(0, 3).join(" | "));
    await p.context().close();
  }
  // "Remove from this computer": portable (undo + delete data) and installed (uninstaller)
  for (const lang of ["en", "ru"]) {
    const { p, errors } = await open(browser, "lang=" + lang);
    await p.click('#tabs button[data-tab="settings"]'); await p.waitForTimeout(100);
    await p.click("#btn-remove"); await p.waitForSelector("#modal:not(.hidden) #rm-data");
    await shot(p, `${lang}-remove`);
    await p.click("#dialog .buttons button >> nth=0"); await p.waitForTimeout(100);
    check(`${lang}: remove can be cancelled`, !(await p.isVisible("#modal:not(.hidden)")) && !(await p.evaluate(() => window.__ST__.removed)));
    await p.click("#btn-remove"); await p.waitForSelector("#modal:not(.hidden) #rm-data");
    await p.check("#rm-data");
    await p.click("#dialog .buttons button.danger"); await p.waitForTimeout(200);
    check(`${lang}: remove undoes changes (Razer services back)`, (await p.evaluate(() => window.__ST__.removed && window.__ST__.removed.startRazer)) === true);
    await shot(p, `${lang}-remove-done`);
    await p.keyboard.press("Escape"); await p.waitForTimeout(100);
    check(`${lang}: final window stays until a choice`, await p.isVisible("#modal:not(.hidden)"));
    await p.click("#dialog .buttons button >> nth=0"); await p.waitForTimeout(150);
    const fin = await p.evaluate(() => window.__ST__.finish);
    check(`${lang}: quit, delete data, open the folder`, !!fin && fin.deleteData === true && fin.openFolder === true, JSON.stringify(fin));
    check(`${lang}: remove without JavaScript errors`, errors.length === 0, errors.slice(0, 3).join(" | "));
    await p.context().close();
  }
  {
    const { p } = await open(browser, "lang=en&installed");
    await p.click('#tabs button[data-tab="settings"]'); await p.waitForTimeout(100);
    await p.click("#btn-remove"); await p.waitForSelector("#modal:not(.hidden)");
    check("installed: no own removal dialog", !(await p.isVisible("#rm-data")));
    await p.click("#dialog .buttons button.primary"); await p.waitForTimeout(150);
    check("installed: runs the uninstaller", await p.evaluate(() => !!window.__ST__.uninstaller));
    await p.context().close();
  }
  // software profile in slot 5: a side button run by open-rzr
  for (const lang of ["en", "ru"]) {
    const { p, errors } = await open(browser, "lang=" + lang);
    check(`${lang}: active slot shown`, /1/.test(await p.textContent("#active-chip")) && await p.isVisible("#active-chip"));
    await p.evaluate(() => { newSoftProfile(); }); await p.waitForSelector("#modal:not(.hidden) select");
    await shot(p, `${lang}-soft-new`);
    await p.click("#dialog .buttons button.primary"); await p.waitForTimeout(400);
    check(`${lang}: software profile in slot 5`, await p.evaluate(() => S.profile.software && S.profiles.slots[4] === S.profile.id));
    await p.evaluate(() => { window.__ST__.activeSlot = 5; }); await p.evaluate(() => checkActive()); await p.waitForTimeout(100);
    check(`${lang}: active slot follows the mouse`, /5/.test(await p.textContent("#active-chip")));
    await p.click("#btn-onboard"); await p.waitForTimeout(150);
    check(`${lang}: active slot marked in the memory list`, await p.isVisible(".onb .slot.active"));
    await shot(p, `${lang}-onboard-active`);
    await p.keyboard.press("Escape"); await p.mouse.click(5, 400); await p.waitForTimeout(100);
    await p.evaluate(() => { S.figView = "12"; showPage("buttons"); });
    await p.evaluate(() => { editButton(0x40); }); await p.waitForSelector("#modal:not(.hidden) select");
    await p.selectOption("#dialog > select", "software"); await p.waitForTimeout(100);
    await p.click("#dialog .soft-row .capture"); await p.keyboard.press("Control+KeyC");
    await p.click('#dialog button:has-text("' + (lang === "en" ? "+ Action" : "+ Действие") + '")');
    await p.fill("#dialog .soft-row:nth-of-type(2) input", "hello");
    await shot(p, `${lang}-soft-editor`);
    await p.click("#dialog .buttons button.primary"); await p.waitForTimeout(300);
    const soft = await p.evaluate(() => S.profile.soft["0x40"]);
    check(`${lang}: button 0x40 run by open-rzr`, !!soft && soft.trigger === 0x68 && soft.actions.length === 2 && soft.actions[0].value === ((0x01 << 8) | 0x06), JSON.stringify(soft));
    check(`${lang}: shown with ⚡`, /⚡/.test(await p.evaluate(() => describe(bindingOf(0x40)))));
    await shot(p, `${lang}-soft-buttons`);
    check(`${lang}: software profile without JavaScript errors`, errors.length === 0, errors.slice(0, 3).join(" | "));
    await p.context().close();
  }
  // a mouse over Bluetooth: read-only
  for (const lang of ["en", "ru"]) {
    const errors = [];
    const p = await (await browser.newContext({ viewport: { width: 1200, height: 860 } })).newPage();
    p.on("pageerror", (e) => errors.push(e.message));
    p.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
    await p.goto(page + "?lang=" + lang + "&ble");
    await p.waitForFunction(() => typeof S === "object" && !!S.conn, null, { timeout: 15000 });
    await p.evaluate(() => showPage("device")); await p.waitForTimeout(200);
    await p.waitForFunction(() => document.querySelectorAll("#ble-bind tr").length >= 5, null, { timeout: 5000 });
    check(`${lang}: modular Bluetooth mouse has no fixed side buttons on top`, await p.evaluate(() => modularBody() && !hasKeypad() && !document.querySelector('#fig .mouse-part[data-id="4"], #fig .mouse-part[data-id="5"]')));
    check(`${lang}: Bluetooth bindings shown`, await p.isVisible("#ble-bind-card"));
    check(`${lang}: read-only mode`, await p.evaluate(() => document.body.classList.contains("read-only")));
    await shot(p, `${lang}-ble-device`);
    await p.evaluate(() => showPage("perf")); await p.waitForTimeout(150);
    check(`${lang}: read-only notice on Performance`, await p.isVisible("#page-perf .ro-notice"));
    check(`${lang}: no polling rate over Bluetooth`, !(await p.isVisible("#poll-card")));
    check(`${lang}: DPI stays read-only over Bluetooth`, await p.evaluate(() => getComputedStyle(document.querySelector("#page-perf [data-feat=dpi_stages] input, #page-perf [data-feat=dpi_stages] button")).pointerEvents === "none"));
    await shot(p, `${lang}-ble-perf`);
    // the sleep timer can be changed; the low-battery level is left alone
    await p.evaluate(() => showPage("power")); await p.waitForTimeout(150);
    check(`${lang}: sleep timer writable over Bluetooth`, await p.evaluate(() => getComputedStyle($("#idle")).pointerEvents !== "none" && getComputedStyle($("#btn-power")).pointerEvents !== "none"));
    await p.evaluate(() => { $("#idle").value = 6; }); await p.click("#btn-power"); await p.waitForTimeout(200);
    check(`${lang}: power over Bluetooth writes only the sleep timer`, JSON.stringify(await p.evaluate(() => window.__ST__.power)) === "[360,0]", JSON.stringify(await p.evaluate(() => window.__ST__.power)));
    await shot(p, `${lang}-ble-power`);
    // buttons: profiles and slots over Bluetooth, without creating or deleting slots
    await p.evaluate(() => showPage("buttons")); await p.waitForTimeout(200);
    check(`${lang}: profile box over Bluetooth`, await p.isVisible("#prof-box"));
    await p.click("#btn-onboard"); await p.waitForTimeout(150);
    check(`${lang}: no slot create/delete over Bluetooth`, (await p.$$(".onb .slot.missing")).length === 3 && !(await p.$(".onb .slot button.small")) && !(await p.$(".onb .slot button.icon")));
    await p.keyboard.press("Escape"); await p.waitForTimeout(100);
    check(`${lang}: Bluetooth without JavaScript errors`, errors.length === 0, errors.slice(0, 3).join(" | "));
    await p.context().close();
  }
  // Diagnostics (log) and Help fill the window: no window scroll bar, the log and the text scroll
  {
    const { p, errors } = await open(browser, "lang=ru");
    for (const style of ["synapse", "classic"]) {
      await p.evaluate((s) => applyStyle(s), style);
      for (const [w, h] of [[1180, 760], [900, 600], [1400, 950]]) {
        await p.setViewportSize({ width: w, height: h });
        await p.evaluate(() => { showPage("diag"); showDiagView("log"); $("#log").textContent = Array.from({ length: 400 }, (_, i) => "line " + i).join("\n"); });
        await p.waitForTimeout(150);
        const d = await p.evaluate(() => { const pg = $("#pages"), lg = $("#log"), r = $("#btn-raw").getBoundingClientRect();
          return { win: pg.scrollHeight - pg.clientHeight, log: lg.scrollHeight > lg.clientHeight, raw: r.bottom <= pg.getBoundingClientRect().bottom + 1, logh: lg.clientHeight }; });
        check(`diag ${style} ${w}x${h}: no window scroll, the log scrolls, the command row is visible`, d.win <= 1 && d.log && d.raw && d.logh >= 150, JSON.stringify(d));
        await p.evaluate(() => { showPage("help"); });
        await p.waitForTimeout(250);
        const hp = await p.evaluate(() => { const pg = $("#pages"), b = $("#help-body");
          return { win: pg.scrollHeight - pg.clientHeight, text: b.scrollHeight > b.clientHeight, bottom: b.getBoundingClientRect().bottom <= pg.getBoundingClientRect().bottom + 1 }; });
        check(`help ${style} ${w}x${h}: no window scroll, the text scrolls`, hp.win <= 1 && hp.text && hp.bottom, JSON.stringify(hp));
        if (style === "synapse" && w === 1180) { await shot(p, "ru-help-fill"); await p.evaluate(() => showPage("diag")); await p.waitForTimeout(100); await shot(p, "ru-diag-fill"); }
      }
    }
    // the wireless view and other pages scroll the window as before
    await p.setViewportSize({ width: 1180, height: 760 });
    await p.evaluate(() => { applyStyle("synapse"); showPage("diag"); showDiagView("wl"); });
    await p.waitForTimeout(150);
    check("wireless view: window scroll as before", !(await p.evaluate(() => $("#pages").classList.contains("fill"))));
    await p.evaluate(() => showPage("settings")); await p.waitForTimeout(100);
    check("settings: window scroll as before", !(await p.evaluate(() => $("#pages").classList.contains("fill"))));
    check("fill layout without JavaScript errors", errors.length === 0, errors.slice(0, 3).join(" | "));
    await p.context().close();
  }
  // Linux: an access rule of an older version is offered for update once; Settings shows its state
  {
    const { p, errors } = await open(browser, "lang=ru&os=linux&udevold&rz");
    await p.waitForSelector("#modal:not(.hidden)", { timeout: 5000 });
    check("outdated udev rule: update offered", (await p.textContent("#modal")).includes("Обновить правило доступа"));
    await p.click('#modal:not(.hidden) button:has-text("Обновить")'); await p.waitForTimeout(300);
    check("outdated udev rule: installed", await p.evaluate(() => !!window.__ST__.udevInstalled));
    await p.evaluate(() => showPage("settings")); await p.waitForTimeout(200);
    check("Settings: udev card on Linux, up to date", await p.isVisible("#udev-card") && (await p.textContent("#udev-state")).includes("актуальное"));
    check("Linux: no Synapse controls or status request", !(await p.isVisible("#rz-card"))
      && await p.evaluate(() => !window.__ST__.calls.includes("razer_software")));
    await shot(p, "ru-udev-card");
    check("udev without JavaScript errors", errors.length === 0, errors.slice(0, 3).join(" | "));
    await p.context().close();
  }
  {
    const { p, errors } = await open(browser, "lang=en&rz");
    const dialog = p.locator("#modal:not(.hidden)");
    await p.click('#nav button[data-page="device"]');
    await p.click("#btn-rz-stop2");
    check("Windows: offers both Synapse stop options", await dialog.getByRole("button", { name: "Stop once" }).isEnabled()
      && await dialog.getByRole("button", { name: "Stop and disable startup" }).isEnabled());
    await dialog.getByRole("button", { name: "Cancel" }).click();
    check("Cancel leaves Synapse running", await p.evaluate(() => !window.__ST__.calls.includes("stop_razer_software")));
    await p.click("#btn-rz-stop2");
    await dialog.getByRole("button", { name: "Stop once" }).click();
    await p.waitForFunction(() => window.__ST__.rz.autostart === false);
    await p.waitForSelector("#rz-notice", { state: "hidden" });
    await p.click('#tabs button[data-tab="settings"]');
    await p.click("#btn-rz-stop");
    await dialog.getByRole("button", { name: "Stop and disable startup" }).click();
    await p.waitForFunction(() => window.__ST__.rz.autostart === true);
    check("Windows: one-time stop and startup change send different options", await p.evaluate(
      () => window.__ST__.calls.filter((cmd) => cmd === "stop_razer_software").length === 2));
    check("Settings: no udev card on Windows", !(await p.isVisible("#udev-card")));
    check("Synapse stop dialog without JavaScript errors", errors.length === 0, errors.join(" | "));
    await p.context().close();
  }
  // on-board slots: create a missing one; the list of all buttons beside the picture (Settings)
  {
    const { p, errors } = await open(browser, "lang=ru&slots2");
    await p.click("#btn-onboard"); await p.waitForTimeout(150);
    check("missing slots offer Create", (await p.$$(".onb .slot.missing")).length === 3);
    await shot(p, "ru-onboard-missing");
    await p.evaluate(() => { window.__realConfirm = confirmBox; window.confirmBox = async () => true; });
    await p.click(".onb .slot.missing button >> nth=2"); await p.waitForTimeout(300);
    check("slot 5 created", await p.evaluate(() => S.conn.profiles.includes(5)) && (await p.$$(".onb .slot.missing")).length === 2);
    await p.click(".onb .slot:not(.missing) button.icon >> nth=-1"); await p.waitForTimeout(300);
    await p.evaluate(() => { window.confirmBox = window.__realConfirm; });
    check("slot 5 deleted", await p.evaluate(() => !S.conn.profiles.includes(5)));
    await p.keyboard.press("Escape");
    await p.click('#tabs button[data-tab="settings"]'); await p.waitForTimeout(100);
    await p.check("#set-side-list"); await p.waitForTimeout(100);
    await p.evaluate(() => showPage("buttons")); await p.waitForTimeout(100);
    await p.click("#btn-view"); await p.waitForTimeout(150);
    check("all buttons beside the picture", await p.isVisible("#cust-side .side-item") && await p.isVisible("#fig"));
    check("no layers icon, no ▶ in the write button", !(await p.$("#prof-box svg.pi")) && !/▶/.test(await p.textContent("#btn-write")));
    await shot(p, "ru-side-list");
    await p.selectOption("#cust-side select", "plate"); await p.waitForTimeout(100);
    check("side list filter: side plate", (await p.$$("#cust-side .side-item")).length === 12);
    await p.click('button.plate[data-view="12"]'); await p.waitForTimeout(150);
    check("side plate view draws the keypad (with the list open)", (await p.$$("#fig .side-key-n")).length === 12 && await p.isVisible("#cust-side .side-item"));
    await p.click("#btn-view"); await p.waitForTimeout(100);
    await p.click('button.plate[data-view="6"]'); await p.waitForTimeout(150);
    check("side plate view draws the keypad (list closed)", (await p.$$("#fig .side-key-n")).length === 6 && !(await p.isVisible("#cust-side .side-item")));
    check("slots and list without JavaScript errors", errors.length === 0, errors.slice(0, 3).join(" | "));
    await p.context().close();
  }
  // optional updates (only the fourth number changes): off by default, shown once turned on
  {
    const { p, errors } = await open(browser, "lang=ru");
    await p.route("https://api.github.com/**", (r) => r.fulfill({ contentType: "application/json",
      body: JSON.stringify({ tag_name: "v0.8.1.2", html_url: "https://github.com/aspidovss/open-rzr/releases/tag/v0.8.1.2", draft: false, prerelease: false }) }));
    await p.click('#tabs button[data-tab="settings"]'); await p.waitForTimeout(100);
    check("optional updates off by default", !(await p.isChecked("#set-updates-optional")));
    await p.click("#btn-update-check"); await p.waitForTimeout(400);
    check("optional update not offered", /0\.8\.1\.2/.test(await p.textContent("#update-state")) && !(await p.isVisible("#update-banner")), await p.textContent("#update-state"));
    await p.check("#set-updates-optional"); await p.waitForTimeout(100);
    await p.click("#btn-update-check"); await p.waitForTimeout(400);
    check("optional update offered when turned on", await p.isVisible("#update-banner"));
    check("version compare: four numbers", await p.evaluate(() => newerVersion("0.8.2", "0.8.1.9") && !newerVersion("0.8.1", "0.8.1.0") && optionalOnly("v0.8.1.3", "0.8.1.1") && !optionalOnly("0.9.0.0", "0.8.1.1")));
    await shot(p, "ru-updates-optional");
    check("optional updates without JavaScript errors", errors.length === 0, errors.slice(0, 3).join(" | "));
    await p.context().close();
  }
  {
    const { p, errors } = await open(browser, "lang=en&keyboard", "#keyboard-preview .keyboard-key");
    await p.click('#nav button[data-page="lighting"]');
    check("keyboard lighting has a key layout", await p.isVisible("#keyboard-preview"));
    await p.click("#btn-light-off");
    await p.waitForFunction(() => window.__ST__.saved.backlight?.effect === "off");
    check("keyboard preview reflects off", await p.isVisible("#keyboard-preview.keyboard-dark"));
    await p.selectOption('#zones [data-zone="backlight"] .eff', "custom");
    await p.click('#keyboard-preview [data-cell="67"]'); // A: row 3, column 1
    await p.click("#btn-light-all");
    await p.waitForFunction(() => window.__ST__.saved.backlight?.effect === "custom");
    check("keyboard sends separate colors in matrix order", await p.evaluate(() => {
      const colors = window.__ST__.saved.backlight.leds;
      return colors.length === 132 && colors[67].join() === "0,255,128" && colors[68].join() === "0,0,0";
    }));
    await p.click('#tabs button[data-tab="macros"]');
    check("keyboard macro library has no device writes", await p.isVisible("#btn-msave") && !(await p.isVisible("#btn-mwrite")));
    check("keyboard without JavaScript errors", errors.length === 0, errors.slice(0, 3).join(" | "));
    await p.context().close();
  }
  // declining the agreement closes the program
  {
    const p = await (await browser.newContext()).newPage();
    await p.goto(page + "?firstrun&lang=en");
    await p.waitForSelector("#modal:not(.hidden) pre.eula", { timeout: 10000 });
    await p.click("#dialog .buttons button >> nth=0"); await p.waitForTimeout(150);
    await p.click("#dialog .buttons button.primary"); await p.waitForTimeout(150);
    check("decline closes the program", await p.evaluate(() => !!window.__ST__.quit));
    await p.context().close();
  }
  await browser.close();
  server.close();
  console.log(failed ? `smoke: ${failed} FAILED` : "smoke: all OK");
  process.exit(failed ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
