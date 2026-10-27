/* Inkwell — renderer-side app logic. Plain ES2020, no bundler. */
"use strict";

const { invoke, listen, window: appWindow } = window.__inkwell;

const $ = (id) => document.getElementById(id);

const LS = {
  theme: "inkwell.theme",
  recents: "inkwell.recents",
  thread: "inkwell.thread",
  session: "inkwell.session",
  rail: "inkwell.rail",
  chat: "inkwell.chat",
};

const MAX_DOC_BYTES = 8 * 1024 * 1024;

const ICONS = {
  rail: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M9 4v16"/></svg>',
  source: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="m8 8-4 4 4 4"/><path d="m16 8 4 4-4 4"/></svg>',
  sun: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>',
  moon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z"/></svg>',
  settings: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><path d="M4 7h9M17 7h3M4 17h4M12 17h8"/><circle cx="15" cy="7" r="2"/><circle cx="10" cy="17" r="2"/></svg>',
  newChat: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14M5 12h14"/></svg>',
};

const state = {
  settings: null,
  info: null,
  doc: null,
  outline: [],
  raw: "",
  view: "rendered",
  thread: [],
  streaming: null,
  selection: "",
  models: [],
  keyCleared: false,
  renderTimer: 0,
  activeHeading: "",
};

const isLocalEndpoint = (url = "") =>
  /(^|\/\/)(localhost|127\.0\.0\.1|\[::1\]|0\.0\.0\.0)/i.test(url);

const activeProvider = () =>
  state.settings?.providers.find((p) => p.id === state.settings.activeProvider) || null;

const providerReady = (provider) =>
  !!provider && !!provider.model && (provider.hasKey || isLocalEndpoint(provider.baseUrl));

/* ---------------------------------------------------------------- utilities */

function toast(message, kind = "") {
  const el = document.createElement("div");
  el.className = `toast ${kind}`.trim();
  el.textContent = message;
  $("toasts").appendChild(el);
  setTimeout(
    () => {
      el.classList.add("out");
      setTimeout(() => el.remove(), 240);
    },
    kind === "error" ? 7000 : 3000
  );
}

function formatBytes(bytes) {
  if (!bytes) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function statState(text) {
  $("statState").textContent = text;
}

function applyTheme(theme) {
  const next = theme === "light" ? "light" : "dark";
  document.documentElement.dataset.theme = next;
  localStorage.setItem(LS.theme, next);
  $("themeBtn").innerHTML = next === "dark" ? ICONS.sun : ICONS.moon;
  $("themeBtn").title = next === "dark" ? "Switch to light theme" : "Switch to dark theme";
}

function loadTheme() {
  applyTheme(localStorage.getItem(LS.theme) || "dark");
}

function syncProviderUI() {
  const provider = activeProvider();
  const label = provider
    ? `${provider.label} · ${provider.model || "no model"}`
    : "No provider configured";
  $("providerChipText").textContent = label;
  $("railProviderLabel").textContent = label;
  $("statProvider").textContent = label;
  const ready = providerReady(provider);
  $("railDot").classList.toggle("on", ready);
  $("chatDot").classList.toggle("on", ready);
  $("chatDot").title = ready ? "Provider ready" : "Provider needs an API key";
  if (state.doc) $("docChip").hidden = false;
}

/* ---------------------------------------------------------------- settings */

async function loadSettings() {
  try {
    state.settings = await invoke("get_settings");
  } catch (err) {
    toast(`Could not load settings: ${err}`, "error");
    state.settings = null;
  }
  syncProviderUI();
  syncCredentialStatus();
  if (state.settings) {
    $("contextToggle").setAttribute("aria-pressed", String(!!state.settings.includeDocument));
  }
}

async function loadInfo() {
  try {
    state.info = await invoke("app_info");
    const backend = state.settings?.credentials?.backend;
    $("storageNote").textContent = state.info.settingsFile
      ? `Settings are stored at ${state.info.settingsFile}${
          backend ? `. API keys are kept in ${backend}.` : "."
        }`
      : "";
  } catch {
    /* non-fatal */
  }
}

function syncCredentialStatus() {
  const status = state.settings?.credentials;
  const warning = $("credentialWarning");
  const text = $("credentialWarningText");
  if (!status || status.available) {
    warning.hidden = true;
    text.textContent = "";
    return;
  }
  warning.hidden = false;
  const detail = status.detail || "Inkwell could not open a system credential store.";
  text.textContent = `${detail} Keys entered now are kept in memory only and are lost when Inkwell quits.`;
}
/* ------------------------------------------------------------------ markdown */

async function renderMarkdown(markdown) {
  return invoke("render_markdown", { markdown });
}

function addHeadingAnchors(root) {
  root.querySelectorAll("h1[id], h2[id], h3[id], h4[id]").forEach((heading) => {
    if (heading.querySelector(".heading-anchor")) return;
    const anchor = document.createElement("a");
    anchor.className = "heading-anchor";
    anchor.href = `#${heading.id}`;
    anchor.textContent = "#";
    anchor.title = "Link to this section";
    heading.appendChild(anchor);
  });
}

function enhanceCodeBlocks(root) {
  root.querySelectorAll("pre").forEach((pre) => {
    if (pre.parentElement?.classList.contains("md-code")) return;
    const code = pre.querySelector("code");
    const langClass = [...(code?.classList || [])].find((name) => name.startsWith("language-"));
    const lang = langClass ? langClass.replace("language-", "") : "";

    if (window.hljs && code && !code.dataset.highlighted) {
      try {
        hljs.highlightElement(code);
        code.dataset.highlighted = "yes";
      } catch {
        /* highlighting is best-effort */
      }
    }

    const wrap = document.createElement("div");
    wrap.className = "md-code";
    pre.parentNode.insertBefore(wrap, pre);
    wrap.appendChild(pre);

    const toolbar = document.createElement("div");
    toolbar.className = "code-toolbar";
    if (lang) {
      const tag = document.createElement("span");
      tag.className = "code-lang";
      tag.textContent = lang;
      toolbar.appendChild(tag);
    }
    const copy = document.createElement("button");
    copy.type = "button";
    copy.className = "copy-code";
    copy.textContent = "Copy";
    copy.dataset.copy = "code";
    toolbar.appendChild(copy);
    wrap.appendChild(toolbar);
  });
}

function enhanceTables(root) {
  root.querySelectorAll("table").forEach((table) => {
    if (table.parentElement?.classList.contains("table-wrap")) return;
    const wrap = document.createElement("div");
    wrap.className = "table-wrap";
    table.parentNode.insertBefore(wrap, table);
    wrap.appendChild(table);
  });
}

function enhanceMarkdown(root, { compact = false } = {}) {
  enhanceTables(root);
  enhanceCodeBlocks(root);
  if (!compact) addHeadingAnchors(root);
}

/* ------------------------------------------------------------- document view */

function setView(view) {
  state.view = view === "source" ? "source" : "rendered";
  $("app").dataset.view = state.view;
  const hasDoc = !!state.doc;
  $("doc").hidden = !hasDoc || state.view !== "rendered";
  $("source").hidden = !hasDoc || state.view !== "source";
  $("empty").hidden = hasDoc;
  $("sourceBtn").classList.toggle("active", state.view === "source");
  $("sourceBtn").title =
    state.view === "source"
      ? "Back to rendered view (Ctrl+U)"
      : "View Markdown source (Ctrl+U)";
}

function showEmpty() {
  state.doc = null;
  $("doc").hidden = true;
  $("source").hidden = true;
  $("empty").hidden = false;
  $("docTitle").textContent = "No document";
  $("docMeta").textContent = "";
  $("statFile").textContent = "No document";
  $("statCounts").textContent = "—";
  $("docChip").hidden = true;
  $("toc").innerHTML = '<p class="rail-hint">Open a document to see its outline.</p>';
  $("tocCount").textContent = "";
  setView("rendered");
}

async function setDocument(name, content, options = {}) {
  if (typeof content !== "string") return;
  if (content.length > MAX_DOC_BYTES) {
    toast("That document is too large to open (8 MB limit).", "error");
    return;
  }

  let rendered;
  try {
    rendered = await renderMarkdown(content);
  } catch (err) {
    toast(`Could not render this document: ${err}`, "error");
    return;
  }

  const doc = {
    name: name || "untitled.md",
    content,
    path: options.path || null,
    size: options.size ?? new Blob([content]).size,
    words: rendered.words,
    title: rendered.title || name || "untitled.md",
    outline: rendered.outline || [],
  };

  state.doc = doc;
  state.outline = doc.outline;
  state.raw = content;
  state.selection = "";
  renderSelectionChip();

  $("doc").innerHTML = rendered.html;
  enhanceMarkdown($("doc"));
  $("source").textContent = content;

  $("docTitle").textContent = doc.title;
  $("docMeta").textContent = `${doc.name} · ${formatBytes(doc.size)}`;
  $("statFile").textContent = doc.name;
  $("statCounts").textContent = `${doc.words.toLocaleString()} words · ${Math.max(
    1,
    Math.round(doc.words / 220)
  )} min read`;
  $("docChip").hidden = false;

  buildToc();
  setView("rendered");
  $("reader").scrollTop = 0;
  requestAnimationFrame(() => {
    $("reader").scrollTop = 0;
    updateProgress();
  });
  statState("Ready");

  if (rendered.truncated) toast("Large document — only the first 8 MB were rendered.", "error");
  if (options.remember !== false) rememberDocument(doc);
  saveSession(doc);
}

function buildToc() {
  const toc = $("toc");
  toc.textContent = "";
  const outline = state.outline.filter((item) => item.text.trim().length > 0);
  $("tocCount").textContent = outline.length || "";

  if (!outline.length) {
    toc.innerHTML = '<p class="rail-hint">This document has no headings.</p>';
    return;
  }

  outline.forEach((item) => {
    const link = document.createElement("a");
    link.href = `#${item.id}`;
    link.textContent = item.text;
    link.dataset.heading = item.id;
    link.style.paddingLeft = `${8 + (Math.min(item.level, 4) - 1) * 11}px`;
    link.addEventListener("click", (event) => {
      event.preventDefault();
      scrollToHeading(item.id);
    });
    toc.appendChild(link);
  });

  observeHeadings();
}

function scrollToHeading(id) {
  if (state.view !== "rendered") setView("rendered");
  const target = document.getElementById(id);
  if (!target) return;
  target.scrollIntoView({ behavior: "smooth", block: "start" });
  try {
    history.replaceState(null, "", `#${id}`);
  } catch {
    /* hash updates are best-effort */
  }
}

let headingObserver = null;

function observeHeadings() {
  headingObserver?.disconnect();
  const headings = [...$("doc").querySelectorAll("h1[id], h2[id], h3[id], h4[id], h5[id]")];
  if (!headings.length) return;

  headingObserver = new IntersectionObserver(
    (entries) => {
      const visible = entries
        .filter((entry) => entry.isIntersecting)
        .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
      if (!visible.length) return;
      setActiveHeading(visible[0].target.id);
    },
    { root: $("reader"), rootMargin: "-8% 0px -72% 0px", threshold: 0 }
  );
  headings.forEach((heading) => headingObserver.observe(heading));
}

function setActiveHeading(id) {
  if (state.activeHeading === id) return;
  state.activeHeading = id;
  $("toc").querySelectorAll("a").forEach((link) => {
    link.classList.toggle("active", link.dataset.heading === id);
  });
}

function updateProgress() {
  const reader = $("reader");
  const max = reader.scrollHeight - reader.clientHeight;
  const ratio = max > 4 ? Math.min(1, Math.max(0, reader.scrollTop / max)) : 0;
  $("progressBar").style.width = `${(ratio * 100).toFixed(2)}%`;
}

/* -------------------------------------------------------------------- recents */

function readRecents() {
  try {
    const parsed = JSON.parse(localStorage.getItem(LS.recents) || "[]");
    return Array.isArray(parsed) ? parsed.filter((item) => item && item.name && item.content) : [];
  } catch {
    return [];
  }
}

function rememberDocument(doc) {
  if (doc.content.length > 600_000) return;
  const entries = readRecents().filter((entry) => entry.name !== doc.name);
  entries.unshift({
    name: doc.name,
    title: doc.title,
    content: doc.content,
    words: doc.words,
    size: doc.size,
    openedAt: Date.now(),
  });
  let kept = entries.slice(0, 6);
  while (kept.reduce((total, entry) => total + entry.content.length, 0) > 1_200_000) {
    kept = kept.slice(0, -1);
  }
  try {
    localStorage.setItem(LS.recents, JSON.stringify(kept));
  } catch {
    /* storage full — keep the current session working */
  }
  renderRecents();
}

function timeAgo(timestamp) {
  if (!timestamp) return "earlier";
  const seconds = Math.round((Date.now() - timestamp) / 1000);
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.round(seconds / 60)} min ago`;
  if (seconds < 86400) return `${Math.round(seconds / 3600)} h ago`;
  return `${Math.round(seconds / 86400)} d ago`;
}

function renderRecents() {
  const list = $("recents");
  list.textContent = "";
  const entries = readRecents();
  if (!entries.length) {
    const hint = document.createElement("li");
    hint.className = "rail-hint";
    hint.textContent = "Documents you open show up here.";
    list.appendChild(hint);
    return;
  }
  entries.forEach((entry) => {
    const li = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "recent-item";
    button.title = `Reopen ${entry.name}`;

    const name = document.createElement("span");
    name.className = "recent-name";
    name.textContent = entry.title || entry.name;
    const meta = document.createElement("span");
    meta.className = "recent-meta";
    meta.textContent = `${(entry.words || 0).toLocaleString()} words · ${timeAgo(
      entry.openedAt
    )}`;

    button.append(name, meta);
    button.addEventListener("click", () => {
      setDocument(entry.name, entry.content, { size: entry.size });
      toast(`Reopened ${entry.name}`);
    });
    li.appendChild(button);
    list.appendChild(li);
  });
}

function saveSession(doc) {
  try {
    if (doc.content.length > 700_000) {
      localStorage.removeItem(LS.session);
      return;
    }
    localStorage.setItem(LS.session, JSON.stringify({ name: doc.name, content: doc.content }));
  } catch {
    /* ignore */
  }
}

async function restoreSession() {
  try {
    const raw = localStorage.getItem(LS.session);
    if (!raw) return false;
    const parsed = JSON.parse(raw);
    if (!parsed?.content) return false;
    await setDocument(parsed.name || "untitled.md", parsed.content, { remember: false });
    return true;
  } catch {
    return false;
  }
}

/* -------------------------------------------------------------------- files */

async function openFile(file) {
  if (!file) return;
  if (file.size > MAX_DOC_BYTES) {
    toast("That file is larger than 8 MB.", "error");
    return;
  }
  try {
    const text = await file.text();
    await setDocument(file.name, text, { size: file.size });
    closeAllModals();
  } catch (err) {
    toast(`Could not read ${file.name}: ${err}`, "error");
  }
}

function pickFile() {
  $("fileInput").value = "";
  $("fileInput").click();
}

/* -------------------------------------------------------------------- links */

async function copyText(text, message = "Copied to clipboard") {
  try {
    await navigator.clipboard.writeText(text);
    toast(message);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    try {
      document.execCommand("copy");
      toast(message);
    } catch {
      toast("Could not copy to the clipboard", "error");
    }
    area.remove();
  }
}

function handleContentClick(event) {
  const copyButton = event.target.closest("[data-copy]");
  if (copyButton) {
    const wrap = copyButton.closest(".md-code");
    const code = wrap?.querySelector("pre code, pre");
    if (code) copyText(code.innerText, "Code copied");
    return;
  }

  const anchor = event.target.closest("a");
  if (!anchor) return;
  const href = anchor.getAttribute("href") || "";

  if (href.startsWith("#")) {
    event.preventDefault();
    const id = href.slice(1);
    if (anchor.classList.contains("heading-anchor")) {
      copyText(`#${id}`, "Section link copied");
    }
    scrollToHeading(id);
    return;
  }

  if (anchor.classList.contains("footnote-ref")) return;
  event.preventDefault();
  if (/^(https?:|mailto:)/i.test(href)) {
    invoke("open_external", { url: href }).catch((err) => toast(String(err), "error"));
  }
}

/* --------------------------------------------------------------------- chat */

let chatEmptyEl = null;

function chatScrollToBottom(force = false) {
  const messages = $("messages");
  const distance = messages.scrollHeight - messages.scrollTop - messages.clientHeight;
  if (force || distance < 160) messages.scrollTop = messages.scrollHeight;
}

function ensureChatVisible() {
  $("app").dataset.chat = "open";
  localStorage.setItem(LS.chat, "open");
}

function createUserNode(text) {
  const root = document.createElement("div");
  root.className = "msg msg-user";
  const bubble = document.createElement("div");
  bubble.className = "bubble";
  bubble.textContent = text;
  root.appendChild(bubble);
  return root;
}

function createAssistantNode({ reasoning = "", content = "" } = {}) {
  const root = document.createElement("div");
  root.className = "msg msg-assistant";

  const reasoningBox = document.createElement("details");
  reasoningBox.className = "reasoning";
  reasoningBox.hidden = !reasoning;
  const summary = document.createElement("summary");
  summary.textContent = "Reasoning";
  const reasoningBody = document.createElement("div");
  reasoningBody.className = "reasoning-body";
  reasoningBody.textContent = reasoning;
  reasoningBox.append(summary, reasoningBody);

  const body = document.createElement("div");
  body.className = "bubble markdown compact";
  body.textContent = content;

  const meta = document.createElement("div");
  meta.className = "msg-meta";
  const who = document.createElement("span");
  who.textContent = "Inkwell";
  const actions = document.createElement("div");
  actions.className = "msg-actions";
  const copy = document.createElement("button");
  copy.type = "button";
  copy.className = "msg-action";
  copy.dataset.copyMessage = "true";
  copy.textContent = "Copy";
  actions.appendChild(copy);
  meta.append(who, actions);

  root.append(reasoningBox, body, meta);
  return { root, reasoningBox, reasoningBody, body, meta };
}

async function renderThread() {
  const messages = $("messages");
  messages.textContent = "";
  if (!state.thread.length) {
    if (chatEmptyEl) {
      chatEmptyEl.hidden = false;
      messages.appendChild(chatEmptyEl);
    }
    return;
  }

  for (const message of state.thread) {
    if (message.role === "user") {
      messages.appendChild(createUserNode(message.content));
    } else {
      const node = createAssistantNode(message);
      messages.appendChild(node.root);
      try {
        const rendered = await renderMarkdown(message.content || "");
        node.body.innerHTML = rendered.html;
        enhanceMarkdown(node.body, { compact: true });
      } catch {
        node.body.textContent = message.content;
      }
    }
  }
  chatScrollToBottom(true);
}

function saveThread() {
  try {
    const payload = JSON.stringify(state.thread.slice(-40));
    if (payload.length > 700_000) return;
    localStorage.setItem(LS.thread, payload);
  } catch {
    /* ignore quota errors */
  }
}

function restoreThread() {
  try {
    const parsed = JSON.parse(localStorage.getItem(LS.thread) || "[]");
    if (!Array.isArray(parsed)) return;
    state.thread = parsed
      .filter((message) => message && (message.role === "user" || message.role === "assistant"))
      .map((message) => ({
        role: message.role,
        content: typeof message.content === "string" ? message.content : "",
        reasoning: typeof message.reasoning === "string" ? message.reasoning : "",
      }));
  } catch {
    state.thread = [];
  }
}

function setStreamingUI(active) {
  $("sendBtn").hidden = active;
  $("stopBtn").hidden = !active;
  $("sendBtn").disabled = active;
  statState(active ? "Streaming…" : "Ready");
}

function cleanupStreamTimer() {
  if (state.renderTimer) {
    clearTimeout(state.renderTimer);
    state.renderTimer = 0;
  }
}

function scheduleStreamRender() {
  if (state.renderTimer) return;
  state.renderTimer = setTimeout(async () => {
    state.renderTimer = 0;
    const stream = state.streaming;
    if (!stream) return;
    const seq = (stream.seq || 0) + 1;
    stream.seq = seq;
    try {
      const rendered = await renderMarkdown(stream.content);
      if (state.streaming !== stream || stream.seq !== seq) return;
      stream.body.innerHTML = rendered.html;
    } catch {
      /* keep the previous frame */
    }
    chatScrollToBottom();
  }, 180);
}

async function finishStreaming({ reason = "stop", error = "" } = {}) {
  const stream = state.streaming;
  if (!stream) return;
  state.streaming = null;
  cleanupStreamTimer();

  stream.body.classList.remove("stream-caret");
  try {
    const rendered = await renderMarkdown(stream.content);
    stream.body.innerHTML = rendered.html;
    enhanceMarkdown(stream.body, { compact: true });
  } catch {
    stream.body.textContent = stream.content;
  }

  if (error) {
    const box = document.createElement("div");
    box.className = "chat-error";
    box.textContent = error;
    stream.root.insertBefore(box, stream.root.querySelector(".msg-meta"));
  }

  if (stream.content.trim() || stream.reasoning.trim()) {
    state.thread.push({
      role: "assistant",
      content: stream.content,
      reasoning: stream.reasoning,
    });
    saveThread();
  }

  setStreamingUI(false);
  statState(error ? "Error" : reason === "cancelled" ? "Stopped" : "Ready");
  chatScrollToBottom();
  $("prompt").focus();
}

async function sendMessage(override) {
  if (state.streaming) {
    toast("Inkwell is still answering — stop it first.", "error");
    return;
  }

  const prompt = $("prompt");
  const text = (typeof override === "string" ? override : prompt.value).trim();
  if (!text) return;

  const provider = activeProvider();
  if (!provider) {
    openSettings();
    toast("Choose a provider and add your API key first.", "error");
    return;
  }
  if (!providerReady(provider)) {
    openSettings(true);
    toast(
      isLocalEndpoint(provider.baseUrl)
        ? "Set a model for this local endpoint in Settings."
        : `${provider.label} needs an API key. Add one in Settings.`,
      "error"
    );
    return;
  }

  ensureChatVisible();
  if (typeof override !== "string") {
    prompt.value = "";
    autoGrow();
  }

  const node = createAssistantNode();
  $("messages").appendChild(createUserNode(text));
  $("messages").appendChild(node.root);
  if (chatEmptyEl) chatEmptyEl.hidden = true;

  const id =
    window.crypto && crypto.randomUUID
      ? crypto.randomUUID()
      : `req-${Date.now()}-${Math.random().toString(16).slice(2)}`;

  state.streaming = {
    id,
    content: "",
    reasoning: "",
    body: node.body,
    root: node.root,
    reasoningBox: node.reasoningBox,
    reasoningBody: node.reasoningBody,
    seq: 0,
  };
  node.body.classList.add("stream-caret");

  state.thread.push({ role: "user", content: text });
  saveThread();

  const includeDocument =
    $("contextToggle").getAttribute("aria-pressed") === "true" && !!state.doc;
  const selection = state.selection || "";
  state.selection = "";
  renderSelectionChip();

  setStreamingUI(true);
  statState("Thinking…");
  chatScrollToBottom(true);

  const messages = state.thread
    .slice(-20)
    .filter((message) => message.role === "user" || message.role === "assistant")
    .map((message) => ({ role: message.role, content: message.content }));

  try {
    await invoke("chat", {
      req: {
        id,
        messages,
        document: includeDocument ? state.doc.content : null,
        selection: selection || null,
        temperature: state.settings.temperature,
        maxTokens: state.settings.maxTokens,
        systemPrompt: state.settings.systemPrompt,
      },
    });
  } catch (err) {
    await finishStreaming({ error: String(err) });
  }
}

async function stopStreaming() {
  const stream = state.streaming;
  if (!stream) return;
  statState("Stopping…");
  try {
    await invoke("cancel_chat", { id: stream.id });
  } catch {
    /* ignore */
  }
  setTimeout(() => {
    if (state.streaming === stream) finishStreaming({ reason: "cancelled" });
  }, 1600);
}

function wireLlmEvents() {
  listen("llm:delta", (event) => {
    const { id, channel, text } = event.payload || {};
    const stream = state.streaming;
    if (!stream || stream.id !== id || !text) return;
    if (channel === "reasoning") {
      stream.reasoning += text;
      stream.reasoningBox.hidden = false;
      stream.reasoningBody.textContent = stream.reasoning;
    } else {
      stream.content += text;
      statState("Streaming…");
      scheduleStreamRender();
    }
    chatScrollToBottom();
  });

  listen("llm:done", (event) => {
    const { id, reason } = event.payload || {};
    if (!state.streaming || state.streaming.id !== id) return;
    finishStreaming({ reason: reason || "stop" });
  });

  listen("llm:error", (event) => {
    const { id, message } = event.payload || {};
    if (!state.streaming || state.streaming.id !== id) return;
    finishStreaming({ error: message || "The provider returned an error." });
  });
}

/* ------------------------------------------------------------- settings modal */

const pendingKeys = new Map();
const clearedKeys = new Set();

function openSettings(focusKey = false) {
  if (!state.settings) {
    toast("Settings are unavailable right now.", "error");
    return;
  }
  fillProviderSelect();
  loadProviderIntoForm(state.settings.activeProvider);
  $("tempInput").value = state.settings.temperature;
  $("tempValue").textContent = Number(state.settings.temperature || 0).toFixed(1);
  $("maxTokensInput").value = state.settings.maxTokens;
  $("systemPrompt").value = state.settings.systemPrompt || "";
  $("includeDocInput").checked = state.settings.includeDocument !== false;
  $("saveHint").textContent = "";
  syncCredentialStatus();
  $("settingsModal").hidden = false;
  setTimeout(() => (focusKey ? $("keyInput") : $("modelInput")).focus(), 40);
}

function fillProviderSelect() {
  const select = $("providerSelect");
  select.textContent = "";
  state.settings.providers.forEach((provider) => {
    const option = document.createElement("option");
    option.value = provider.id;
    option.textContent = provider.label;
    select.appendChild(option);
  });
  if (!state.settings.providers.some((p) => p.id === state.settings.activeProvider)) {
    state.settings.activeProvider = state.settings.providers[0]?.id || "";
  }
  select.value = state.settings.activeProvider;
}

let formProviderId = null;

function currentFormProvider() {
  return state.settings.providers.find((provider) => provider.id === formProviderId) || null;
}

function loadProviderIntoForm(id) {
  const provider = state.settings.providers.find((p) => p.id === id);
  if (!provider) return;
  formProviderId = provider.id;
  const status = state.settings?.credentials;
  const backend = status?.backend || "the system credential store";
  const available = status?.available !== false;

  $("providerSelect").value = provider.id;
  $("modelInput").value = provider.model || "";
  $("baseUrlInput").value = provider.baseUrl || "";
  $("keyInput").value = pendingKeys.get(provider.id) || "";
  $("keyInput").placeholder = provider.hasKey
    ? "Saved — type a new key to replace it"
    : isLocalEndpoint(provider.baseUrl)
      ? "Not required for local endpoints"
      : available
        ? "sk-…"
        : "Session only — no system keychain";
  $("clearKeyBtn").hidden = !provider.hasKey && !pendingKeys.get(provider.id);
  $("keyHint").textContent = provider.hasKey
    ? `Saved in ${backend}. It is never sent back to this window.`
    : available
      ? `Stored in ${backend}.`
      : "Session only — the system credential store is unavailable.";
}

function captureFormIntoSettings() {
  const provider = currentFormProvider();
  if (!provider) return;
  provider.model = $("modelInput").value.trim();
  provider.baseUrl = $("baseUrlInput").value.trim();
  const typed = $("keyInput").value.trim();
  if (typed) {
    pendingKeys.set(provider.id, typed);
    clearedKeys.delete(provider.id);
  }
  state.settings.activeProvider = $("providerSelect").value;
  state.settings.temperature = Number.parseFloat($("tempInput").value) || 0;
  state.settings.maxTokens = Number.parseInt($("maxTokensInput").value, 10) || 1500;
  state.settings.systemPrompt = $("systemPrompt").value;
  state.settings.includeDocument = $("includeDocInput").checked;
}

function buildSettingsPayload() {
  captureFormIntoSettings();
  if ($("providerSelect").value) {
    state.settings.activeProvider = $("providerSelect").value;
  }
  return {
    activeProvider: state.settings.activeProvider,
    providers: state.settings.providers.map((provider) => {
      const key = clearedKeys.has(provider.id)
        ? ""
        : pendingKeys.has(provider.id)
          ? pendingKeys.get(provider.id)
          : undefined;
      return {
        id: provider.id,
        label: provider.label,
        kind: provider.kind,
        baseUrl: provider.baseUrl,
        model: provider.model,
        apiKey: key,
      };
    }),
    temperature: state.settings.temperature,
    maxTokens: state.settings.maxTokens,
    systemPrompt: state.settings.systemPrompt,
    includeDocument: state.settings.includeDocument,
  };
}

function payloadFromState() {
  return {
    activeProvider: state.settings.activeProvider,
    providers: state.settings.providers.map((provider) => ({
      id: provider.id,
      label: provider.label,
      kind: provider.kind,
      baseUrl: provider.baseUrl,
      model: provider.model,
    })),
    temperature: state.settings.temperature,
    maxTokens: state.settings.maxTokens,
    systemPrompt: state.settings.systemPrompt,
    includeDocument: state.settings.includeDocument,
  };
}

async function persistSettings({ silent = true, fromForm = true } = {}) {
  const payload = fromForm ? buildSettingsPayload() : payloadFromState();
  const view = await invoke("save_settings", { payload });
  state.settings = view;
  pendingKeys.clear();
  clearedKeys.clear();
  syncProviderUI();
  syncCredentialStatus();
  $("contextToggle").setAttribute("aria-pressed", String(!!view.includeDocument));
  if (!silent) $("saveHint").textContent = "Saved.";
  return view;
}

async function saveSettingsFromForm() {
  const provider = currentFormProvider();
  if (!provider) return;
  if (!provider.model) {
    $("saveHint").textContent = "Add a model name first.";
    $("modelInput").focus();
    return;
  }
  if (!provider.baseUrl) {
    $("saveHint").textContent = "Add a base URL for this provider.";
    $("baseUrlInput").focus();
    return;
  }
  const button = $("settingsSave");
  button.disabled = true;
  button.textContent = "Saving…";
  try {
    await persistSettings({ silent: false });
    toast("Settings saved");
    $("settingsModal").hidden = true;
  } catch (err) {
    $("saveHint").textContent = String(err);
    toast(String(err), "error");
  } finally {
    button.disabled = false;
    button.textContent = "Save settings";
  }
}

async function fetchModels() {
  const provider = currentFormProvider();
  if (!provider) return;
  const button = $("fetchModels");
  button.disabled = true;
  const original = button.textContent;
  button.textContent = "Fetching…";
  try {
    await persistSettings({ silent: true });
    const models = await invoke("list_models", { providerId: provider.id });
    state.models = models;
    const list = $("modelList");
    list.textContent = "";
    models.forEach((model) => {
      const option = document.createElement("option");
      option.value = model;
      list.appendChild(option);
    });
    if (models.length && !$("modelInput").value.trim()) {
      $("modelInput").value = models[0];
    }
    toast(models.length ? `${models.length} models available` : "The provider returned no models");
  } catch (err) {
    toast(String(err), "error");
  } finally {
    button.disabled = false;
    button.textContent = original;
  }
}

/* -------------------------------------------------------------- paste modal */

function openPaste() {
  $("pasteModal").hidden = false;
  $("pasteArea").value = "";
  setTimeout(() => $("pasteArea").focus(), 40);
}

async function renderPasted() {
  const text = $("pasteArea").value;
  if (!text.trim()) {
    toast("Paste some Markdown first.", "error");
    return;
  }
  await setDocument("pasted.md", text, { remember: false });
  closeAllModals();
  $("pasteArea").value = "";
}

function closeAllModals() {
  $("settingsModal").hidden = true;
  $("pasteModal").hidden = true;
}

/* ---------------------------------------------------------------- selection */

function renderSelectionChip() {
  const chip = $("selectionChip");
  const text = $("selectionText");
  if (!state.selection) {
    chip.hidden = true;
    text.textContent = "";
    return;
  }
  const singleLine = state.selection.replace(/\s+/g, " ").trim();
  chip.hidden = false;
  text.textContent = singleLine.length > 160 ? `${singleLine.slice(0, 160)}…` : singleLine;
  text.title = singleLine.slice(0, 500);
}

function captureSelection() {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || !selection.rangeCount) return;
  const node = selection.anchorNode;
  if (!node || !$("doc").contains(node)) return;
  const text = selection.toString().trim();
  if (text.length < 3) return;
  state.selection = text.slice(0, 12000);
  renderSelectionChip();
}

/* ------------------------------------------------------------------ composer */

function autoGrow() {
  const prompt = $("prompt");
  prompt.style.height = "auto";
  prompt.style.height = `${Math.min(prompt.scrollHeight, 168)}px`;
}

function toggleChat(force) {
  const app = $("app");
  const open = force ?? app.dataset.chat === "closed";
  app.dataset.chat = open ? "open" : "closed";
  localStorage.setItem(LS.chat, app.dataset.chat);
  if (open) setTimeout(() => $("prompt").focus(), 60);
}

function toggleRail(force) {
  const app = $("app");
  const open = force ?? app.dataset.rail === "closed";
  app.dataset.rail = open ? "open" : "closed";
  localStorage.setItem(LS.rail, app.dataset.rail);
}

function toggleSource() {
  if (!state.doc) {
    toast("Open a document first.", "error");
    return;
  }
  setView(state.view === "source" ? "rendered" : "source");
}

function restoreLayout() {
  const railPref = localStorage.getItem(LS.rail);
  $("app").dataset.rail = railPref === "closed" ? "closed" : "open";

  // The assistant starts open; narrow windows overlay it on the reading pane,
  // so a wide-screen default is safe.
  const chatPref = localStorage.getItem(LS.chat);
  $("app").dataset.chat = chatPref === "closed" ? "closed" : "open";
}

/* -------------------------------------------------------------------- sample */

const SAMPLE_MD = `---
title: Welcome to Inkwell
author: Inkwell
---

# Read Markdown. Ask anything.

Inkwell is a **desktop Markdown reader** with a built-in assistant. Rendering happens
locally in Rust, so the page you are looking at never leaves your machine.

> Your document is only sent to the model provider you configure — with your own key.

## What you get

- **Typography that respects the text** — comfortable measure, tuned spacing, local syntax highlighting.
- **Outline with scroll sync** — jump anywhere from the sidebar.
- **Source view** — press \`Ctrl+U\` to see the raw Markdown.
- **Quoted questions** — select a passage, then ask about it.
- **Anything goes** — tables, task lists, footnotes, strikethrough, code fences.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| \`Ctrl+O\` | Open a Markdown file |
| \`Ctrl+J\` | Show or hide the assistant |
| \`Ctrl+B\` | Show or hide the sidebar |
| \`Ctrl+U\` | Toggle rendered / source |
| \`Ctrl+,\` | Assistant settings |
| \`Ctrl+Enter\` | Send the message |

## Code, highlighted locally

\`\`\`rust
use pulldown_cmark::{html, Options, Parser};

fn render(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}
\`\`\`

Inline code like \`cargo tauri dev\` is styled to match.

## A reading checklist

- [x] Drop a \`.md\` file anywhere in this window
- [x] Read it comfortably
- [ ] Paste an LLM answer with **Paste**
- [ ] Connect a model and ask a question

## Connect your own model

1. Open **Settings** with \`Ctrl+,\`.
2. Pick a provider — OpenAI, Anthropic, Gemini, Groq, Mistral, DeepSeek, OpenRouter, xAI…
3. Paste your API key, then hit **Fetch list** to load the model names.
4. Save. The dot next to *Assistant* turns on when the provider is ready.

### Local models are first-class

Point the base URL at \`http://localhost:11434/v1\` for Ollama or
\`http://localhost:1234/v1\` for LM Studio. No API key is required for localhost.

## Where your data goes

Inkwell has no servers. Markdown is rendered by the Rust core, and chat requests go
straight from your machine to the provider you chose. API keys are stored in your
app config folder and are never handed back to the page.

---

Made with Rust, Tauri and a little restraint.[^1]

[^1]: Footnotes work too — click the reference to jump here.
`;

/* -------------------------------------------------------------------- wiring */

function wireEvents() {
  // window chrome
  $("winMin").addEventListener("click", () => appWindow.minimize());
  $("winMax").addEventListener("click", () => appWindow.toggleMaximize());
  $("winClose").addEventListener("click", () => appWindow.close());

  const titlebar = document.querySelector(".titlebar");
  let dragStart = null;
  titlebar.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || event.target.closest("button")) return;
    dragStart = { x: event.screenX, y: event.screenY, dragging: false };
  });
  titlebar.addEventListener("mousemove", (event) => {
    if (!dragStart || dragStart.dragging) return;
    const moved =
      Math.abs(event.screenX - dragStart.x) + Math.abs(event.screenY - dragStart.y);
    if (moved > 3) {
      dragStart.dragging = true;
      appWindow.drag();
    }
  });
  window.addEventListener("mouseup", () => {
    dragStart = null;
  });
  titlebar.addEventListener("dblclick", (event) => {
    if (event.target.closest("button")) return;
    appWindow.toggleMaximize();
  });

  document.querySelectorAll("[data-resize]").forEach((handle) => {
    handle.addEventListener("mousedown", (event) => {
      event.preventDefault();
      event.stopPropagation();
      appWindow.resize(handle.dataset.resize);
    });
  });

  // titlebar actions
  $("railBtn").addEventListener("click", () => toggleRail());
  $("sourceBtn").addEventListener("click", toggleSource);
  $("themeBtn").addEventListener("click", () =>
    applyTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark")
  );
  $("settingsBtn").addEventListener("click", () => openSettings());
  $("chatToggle").addEventListener("click", () => toggleChat());
  $("closeChatBtn").addEventListener("click", () => toggleChat(false));
  $("providerChip").addEventListener("click", () => openSettings());
  $("statProvider").addEventListener("click", () => openSettings());
  $("railProvider").addEventListener("click", () => openSettings());

  // files
  $("openBtn").addEventListener("click", pickFile);
  $("emptyOpen").addEventListener("click", pickFile);
  $("fileInput").addEventListener("change", (event) => {
    const file = event.target.files?.[0];
    if (file) openFile(file);
  });
  $("sampleBtn").addEventListener("click", async () => {
    await setDocument("inkwell-welcome.md", SAMPLE_MD, { remember: false });
    toast("Sample document loaded");
  });
  $("pasteBtn").addEventListener("click", openPaste);
  $("pasteClose").addEventListener("click", closeAllModals);
  $("pasteCancel").addEventListener("click", closeAllModals);
  $("pasteRender").addEventListener("click", renderPasted);
  $("pasteArea").addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      renderPasted();
    }
  });
  $("clearRecents").addEventListener("click", () => {
    localStorage.removeItem(LS.recents);
    renderRecents();
    toast("Recent documents cleared");
  });

  // settings modal
  $("settingsClose").addEventListener("click", closeAllModals);
  $("settingsCancel").addEventListener("click", closeAllModals);
  $("settingsSave").addEventListener("click", saveSettingsFromForm);
  $("fetchModels").addEventListener("click", fetchModels);
  $("providerSelect").addEventListener("change", (event) => {
    if (formProviderId) captureFormIntoSettings();
    loadProviderIntoForm(event.target.value);
  });
  $("keyInput").addEventListener("input", () => {
    const provider = currentFormProvider();
    if (!provider) return;
    const value = $("keyInput").value.trim();
    if (value) {
      pendingKeys.set(provider.id, value);
      clearedKeys.delete(provider.id);
    } else {
      pendingKeys.delete(provider.id);
    }
  });
  $("clearKeyBtn").addEventListener("click", () => {
    const provider = currentFormProvider();
    if (!provider) return;
    pendingKeys.set(provider.id, "");
    clearedKeys.add(provider.id);
    $("keyInput").value = "";
    $("keyInput").placeholder = "Key will be removed when you save";
    $("clearKeyBtn").hidden = true;
  });
  $("tempInput").addEventListener("input", () => {
    $("tempValue").textContent = Number.parseFloat($("tempInput").value).toFixed(1);
  });
  $("settingsModal").addEventListener("click", (event) => {
    if (event.target === $("settingsModal")) closeAllModals();
  });
  $("pasteModal").addEventListener("click", (event) => {
    if (event.target === $("pasteModal")) closeAllModals();
  });
}

function wireInteractionEvents() {
  // assistant
  $("sendBtn").addEventListener("click", () => sendMessage());
  $("stopBtn").addEventListener("click", stopStreaming);
  $("newChatBtn").addEventListener("click", () => {
    if (state.streaming) {
      toast("Stop the current answer first.", "error");
      return;
    }
    state.thread = [];
    saveThread();
    renderThread();
  });
  $("prompt").addEventListener("input", autoGrow);
  $("prompt").addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      sendMessage();
    }
  });
  $("contextToggle").addEventListener("click", async () => {
    const pressed = $("contextToggle").getAttribute("aria-pressed") === "true";
    $("contextToggle").setAttribute("aria-pressed", String(!pressed));
    if (state.settings) {
      state.settings.includeDocument = !pressed;
      try {
        await persistSettings({ silent: true, fromForm: false });
      } catch (err) {
        toast(String(err), "error");
      }
    }
  });
  $("suggestions").addEventListener("click", (event) => {
    const button = event.target.closest(".suggestion");
    if (button) sendMessage(button.textContent);
  });
  $("messages").addEventListener("click", (event) => {
    const copy = event.target.closest("[data-copy-message]");
    if (!copy) return;
    const bubble = copy.closest(".msg-assistant")?.querySelector(".bubble");
    if (bubble) copyText(bubble.innerText, "Answer copied");
  });
  $("clearSelection").addEventListener("click", () => {
    state.selection = "";
    renderSelectionChip();
  });

  // reading pane
  $("reader").addEventListener("scroll", updateProgress, { passive: true });
  $("doc").addEventListener("click", handleContentClick);
  $("messages").addEventListener("click", handleContentClick);
  $("doc").addEventListener("mouseup", () => setTimeout(captureSelection, 0));

  // drag and drop
  let dragDepth = 0;
  window.addEventListener("dragenter", (event) => {
    if (!event.dataTransfer?.types?.includes("Files")) return;
    event.preventDefault();
    dragDepth += 1;
    $("dropOverlay").hidden = false;
  });
  window.addEventListener("dragover", (event) => {
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
  });
  window.addEventListener("dragleave", () => {
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) $("dropOverlay").hidden = true;
  });
  window.addEventListener("drop", (event) => {
    event.preventDefault();
    dragDepth = 0;
    $("dropOverlay").hidden = true;
    const file = event.dataTransfer?.files?.[0];
    if (file) openFile(file);
  });

  // keyboard
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      closeAllModals();
      return;
    }
    if (!(event.ctrlKey || event.metaKey)) return;
    const key = event.key.toLowerCase();
    if (key === "o") {
      event.preventDefault();
      pickFile();
    } else if (key === "j") {
      event.preventDefault();
      toggleChat();
    } else if (key === "b") {
      event.preventDefault();
      toggleRail();
    } else if (key === "u") {
      event.preventDefault();
      toggleSource();
    } else if (key === ",") {
      event.preventDefault();
      openSettings();
    } else if (key === "p") {
      event.preventDefault();
      window.print();
    } else if (event.key === "Enter" && document.activeElement !== $("prompt")) {
      event.preventDefault();
      if ($("app").dataset.chat === "open") sendMessage();
    }
  });
}

/* ---------------------------------------------------------------------- boot */

async function openInitialDocument() {
  try {
    const doc = await invoke("initial_document");
    if (doc?.content) {
      await setDocument(doc.name, doc.content, { path: doc.path, size: doc.size });
      return true;
    }
  } catch (err) {
    console.warn("initial_document failed", err);
  }
  return false;
}

async function boot() {
  $("railBtn").innerHTML = ICONS.rail;
  $("sourceBtn").innerHTML = ICONS.source;
  $("settingsBtn").innerHTML = ICONS.settings;
  $("newChatBtn").innerHTML = ICONS.newChat;

  chatEmptyEl = $("chatEmpty");
  restoreLayout();
  loadTheme();
  wireEvents();
  wireInteractionEvents();
  wireLlmEvents();

  try {
    await loadSettings();
    const credentialStatus = state.settings?.credentials;
    if (credentialStatus && !credentialStatus.available) {
      toast("System keychain unavailable — API keys will be session-only.", "error");
    }
    await loadInfo();
    restoreThread();
    await renderThread();
    renderRecents();

    const opened = await openInitialDocument();
    if (!opened) await restoreSession();
    if (!state.doc) showEmpty();

    syncProviderUI();
    if (!providerReady(activeProvider())) {
      statState("Connect a model to chat");
    }
  } catch (err) {
    console.error(err);
    toast(`Startup problem: ${err}`, "error");
  }
}

boot();

