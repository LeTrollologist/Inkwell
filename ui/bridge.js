/* Inkwell <-> Rust bridge (wry IPC). Loaded before app.js. */
"use strict";

(() => {
  const pending = new Map();
  let sequence = 0;

  window.__inkwell_event = (name, payload) => {
    const listeners = (window.__inkwellListeners && window.__inkwellListeners[name]) || [];
    for (const listener of listeners) {
      try {
        listener({ payload });
      } catch (err) {
        console.error("inkwell: event handler failed", err);
      }
    }
  };

  window.__inkwell_reply = (message) => {
    if (!message || !message.id) return;
    const entry = pending.get(message.id);
    if (!entry) return;
    pending.delete(message.id);
    if (message.ok) entry.resolve(message.result);
    else entry.reject(new Error(message.error || "Command failed"));
  };

  const invoke = (cmd, args = {}) => {
    if (!window.ipc || typeof window.ipc.postMessage !== "function") {
      return Promise.reject(new Error("The native bridge is unavailable."));
    }
    const id = `r${++sequence}`;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      try {
        window.ipc.postMessage(JSON.stringify({ id, cmd, args }));
      } catch (err) {
        pending.delete(id);
        reject(err);
      }
    });
  };

  const listen = (name, handler) => {
    window.__inkwellListeners = window.__inkwellListeners || {};
    const listeners = window.__inkwellListeners[name] || [];
    listeners.push(handler);
    window.__inkwellListeners[name] = listeners;
    return Promise.resolve(() => {
      window.__inkwellListeners[name] = listeners.filter((entry) => entry !== handler);
    });
  };

  window.__inkwell = {
    invoke,
    listen,
    window: {
      minimize: () => invoke("win_minimize"),
      toggleMaximize: () => invoke("win_toggle_maximize"),
      close: () => invoke("win_close"),
      drag: () => invoke("win_drag"),
      resize: (direction) => invoke("win_resize", { direction }),
      isMaximized: () => invoke("win_is_maximized"),
    },
  };
})();
