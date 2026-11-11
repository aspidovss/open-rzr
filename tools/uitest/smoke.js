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
  const file = path.resolve(root, rel);
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

async function open(browser, query) {
  const errors = [];
  const p = await (await browser.newContext({ viewport: { width: 1200, height: 860 } })).newPage();
  p.on("pageerror", (e) => errors.push(e.message));
  p.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  await p.goto(page + "?" + query);
  await p.waitForFunction(() => document.querySelectorAll("#fig .lbl").length > 5, null, { timeout: 15000 });
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
  const browser = await chromium.launch();
  for (const lang of ["en", "ru"]) {
    const { p, errors } = await open(browser, "lang=" + lang);

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
    for (const id of ["#btn-hidlist", "#btn-drivers", "#btn-devfiles", "#btn-wireless"]) {
      await p.click(id);
      await p.waitForSelector("#modal:not(.hidden)", { timeout: 5000 });
      await p.waitForTimeout(150);
      check(`${lang}: dialog ${id}`, await p.isVisible("#modal:not(.hidden)"));
      await shot(p, `${lang}-dialog-${id.slice(5)}`);
      await p.keyboard.press("Escape");
      await p.evaluate(() => { const m = document.querySelector("#modal"); if (m && !m.classList.contains("hidden")) m.querySelector(".buttons button.primary, .buttons button:last-child")?.click(); });
      await p.waitForTimeout(150);
    }

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
      body: JSON.stringify({ tag_name: "v9.9.9", html_url: "https://github.com/aspidovss/open-rzr/releases/tag/v9.9.9", draft: false, prerelease: false }) }));
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
    await p.click("#btn-update-notes"); await p.waitForTimeout(100);
    check(`${lang}: "What's new" opens the release page`, (await p.evaluate(() => window.__ST__.opened)) === "https://github.com/aspidovss/open-rzr/releases/tag/v9.9.9");
    await p.click("#btn-update-hide"); await p.waitForTimeout(100);
    check(`${lang}: hide this version`, !(await p.isVisible("#update-banner")) && (await p.evaluate(() => window.__ST__.skipped)) === "9.9.9");
    await p.click('#tabs button[data-tab="settings"]'); await p.waitForTimeout(100);
    check(`${lang}: setting is on`, await p.isChecked("#set-updates"));
    await p.click("#btn-update-check"); await p.waitForTimeout(400);
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
