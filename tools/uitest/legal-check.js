// The user agreement exists in several places that must agree:
//   ui/legal/eula.{en,ru}.txt            — the texts (window, first run)
//   packaging/windows/agreement.txt       — the installer's licence page: both texts, generated
//   app/src/commands/settings.rs          — AGREEMENT_EDITION
//   packaging/windows/installer-hooks.nsh — OPENRZR_AGREEMENT (written to agreement.txt on install)
//
//   node tools/uitest/legal-check.js           check (exit code 1 on a mismatch)
//   node tools/uitest/legal-check.js --write   regenerate packaging/windows/agreement.txt
const fs = require("fs");
const path = require("path");
const root = path.resolve(__dirname, "..", "..");
const read = (f) => fs.readFileSync(path.join(root, f), "utf8").replace(/\r\n/g, "\n");
let failed = 0;
const check = (name, ok, detail = "") => { console.log(`${ok ? "ok  " : "FAIL"}  ${name}${detail ? "  — " + detail : ""}`); if (!ok) failed++; };

const en = read("ui/legal/eula.en.txt").trimEnd();
const ru = read("ui/legal/eula.ru.txt").trimEnd();
// NSIS shows the file in a plain text box: CRLF line ends, no byte order mark (tauri adds one).
const want = (en + "\n\n" + "=".repeat(60) + "\n\n" + ru + "\n").replace(/\n/g, "\r\n");
const out = path.join(root, "packaging/windows/agreement.txt");
if (process.argv.includes("--write")) {
  fs.writeFileSync(out, want, "utf8");
  console.log("written", path.relative(root, out));
}

const num = (re, text) => { const m = re.exec(text); return m ? Number(m[1]) : NaN; };
const edEn = num(/^Version (\d+)\./m, en);
const edRu = num(/^Редакция (\d+)\./m, ru);
const edRs = num(/AGREEMENT_EDITION: u32 = (\d+);/, read("app/src/commands/settings.rs"));
const edNsh = num(/!define OPENRZR_AGREEMENT (\d+)/, read("packaging/windows/installer-hooks.nsh"));
check("edition: en text = ru text", edEn === edRu, `${edEn} / ${edRu}`);
check("edition: text = settings.rs", edEn === edRs, `${edEn} / ${edRs}`);
check("edition: text = installer hooks", edEn === edNsh, `${edEn} / ${edNsh}`);

const have = fs.existsSync(out) ? fs.readFileSync(out, "utf8").replace(/\r\n/g, "\n") : "";
check("installer licence file is up to date", have === want.replace(/\r\n/g, "\n"), "run: node tools/uitest/legal-check.js --write");
check("installer licence file has no BOM", !have.startsWith("﻿"));
console.log(failed ? `legal: ${failed} FAILED` : "legal: OK");
process.exit(failed ? 1 : 0);
