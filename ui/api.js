// Calls into the Rust side. Inside the app: Tauri IPC. Opened in a plain browser
// (development/testing only): the same commands as HTTP POST /api/<command>.
window.api = (function () {
  const tauri = window.__TAURI__ && window.__TAURI__.core;
  async function call(cmd, args) {
    args = args || {};
    if (tauri) return tauri.invoke(cmd, args);
    const r = await fetch("/api/" + cmd, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(args) });
    const text = await r.text();
    if (!r.ok) throw text;
    return text ? JSON.parse(text) : null;
  }
  /** Events from Rust (e.g. "device-change"); no-op outside the app. */
  function on(name, fn) {
    const ev = window.__TAURI__ && window.__TAURI__.event;
    if (ev && ev.listen) return ev.listen(name, (e) => fn(e.payload));
    return Promise.resolve(() => {});
  }
  return { call, on, inApp: !!tauri };
})();
