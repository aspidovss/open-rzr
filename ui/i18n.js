// Tiny i18n: t("key", {n: 3}) with {name} interpolation and plural forms via Intl.PluralRules.
// Static text in HTML: data-i18n="key" (text), data-i18n-ph="key" (placeholder), data-i18n-title="key".
(function () {
  let lang = "en";
  const dict = () => (window.I18N[lang] || window.I18N.en);
  function t(key, p = {}) {
    let v = dict()[key];
    if (v === undefined) v = window.I18N.en[key];
    if (v === undefined) return key;
    if (typeof v === "object") {
      const form = new Intl.PluralRules(lang).select(Number(p.n) || 0);
      v = v[form] || v.other;
    }
    return v.replace(/\{(\w+)\}/g, (m, k) => (p[k] !== undefined ? p[k] : m));
  }
  function apply(root = document) {
    root.querySelectorAll("[data-i18n]").forEach((e) => { e.textContent = t(e.dataset.i18n); });
    root.querySelectorAll("[data-i18n-ph]").forEach((e) => { e.placeholder = t(e.dataset.i18nPh); });
    root.querySelectorAll("[data-i18n-title]").forEach((e) => { e.title = t(e.dataset.i18nTitle); });
    document.documentElement.lang = lang;
  }
  /** Best guess from the OS/WebView language list: "ru" for Russian, else "en". */
  function detect() {
    const list = (navigator.languages && navigator.languages.length ? navigator.languages : [navigator.language || "en"]);
    for (const l of list) {
      const code = String(l).toLowerCase().split("-")[0];
      if (window.I18N[code]) return code;
    }
    return "en";
  }
  window.i18n = {
    t, apply, detect,
    set(l) { lang = window.I18N[l] ? l : "en"; apply(); },
    get lang() { return lang; },
    languages() { return Object.keys(window.I18N).map((k) => [k, window.I18N[k]["lang.name"]]); },
  };
  window.t = t;
})();
