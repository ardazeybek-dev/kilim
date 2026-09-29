import init, { Doc } from "./pkg/kilim.js";

await init();

const $ = (sel, root = document) => root.querySelector(sel);
const PEERS = [
  { name: "Ada", color: "#a78bfa" },
  { name: "Bora", color: "#34d399" },
  { name: "Cem", color: "#fbbf24" },
];
const SITE_COLORS = {};
const net = { latency: 600, jitter: 400, dup: 0.1, inflight: 0, delivered: 0, dups: 0 };
let peers = [];
let inspected = 0;

// ---------- simulated peers ----------

function createPeers() {
  const root = $("#peers");
  root.innerHTML = "";
  peers = PEERS.map((p, i) => {
    const site = i + 1;
    SITE_COLORS[site] = p.color;
    const el = $("#peerTpl").content.firstElementChild.cloneNode(true);
    el.style.setProperty("--peer", p.color);
    $(".name", el).textContent = p.name;
    root.append(el);
    const peer = { ...p, site, doc: new Doc(site), el, online: true, editor: $(".editor", el) };
    peer.editor.placeholder = `${p.name} types here…`;
    peer.editor.addEventListener("input", () => broadcast(peer, peer.doc.applyText(peer.editor.value)));
    $(".online", el).addEventListener("change", (e) => setOnline(peer, e.target.checked));
    return peer;
  });
  const seed = peers[0].doc.insert(0, "Merhaba! Edit me from any side.");
  broadcast(peers[0], seed);
  render(peers[0]);
  renderTabs();
}

function broadcast(from, opsJson) {
  const ops = JSON.parse(opsJson);
  if (!ops.length) return refresh();
  for (const to of peers) {
    if (to === from) continue;
    // Each op travels on its own, so jitter really does reorder them.
    for (const op of ops) {
      send(from, to, op);
      if (Math.random() < net.dup) { net.dups++; send(from, to, op); }
    }
  }
  refresh();
}

function send(from, to, op) {
  if (!from.online || !to.online) return; // dropped; resynced on reconnect
  net.inflight++;
  const delay = net.latency + Math.random() * net.jitter;
  setTimeout(() => {
    net.inflight--;
    if (from.online && to.online) {
      receive(to, JSON.stringify([op]));
      net.delivered++;
    }
    refresh();
  }, delay);
}

function receive(peer, opsJson) {
  const ed = peer.editor;
  const focused = document.activeElement === ed;
  // Remember the cursor as CRDT anchors, not indices, so remote edits don't move it.
  const a = focused ? peer.doc.anchor(ed.selectionStart) : null;
  const b = focused ? peer.doc.anchor(ed.selectionEnd) : null;
  peer.doc.applyRemote(opsJson);
  render(peer);
  if (focused) ed.setSelectionRange(peer.doc.resolve(a), peer.doc.resolve(b));
}

function setOnline(peer, online) {
  peer.online = online;
  peer.el.classList.toggle("offline", !online);
  if (online) {
    // Anti-entropy: swap version vectors with every online peer, send only what's missing.
    for (const other of peers) {
      if (other === peer || !other.online) continue;
      receive(peer, other.doc.opsSince(peer.doc.version()));
      receive(other, peer.doc.opsSince(other.doc.version()));
    }
  }
  refresh();
}

function render(peer) {
  if (peer.editor.value !== peer.doc.text()) peer.editor.value = peer.doc.text();
}

// ---------- status & inspector ----------

function refresh() {
  for (const p of peers) {
    $(".pending", p.el).textContent = p.doc.pendingCount();
    $(".version", p.el).textContent = prettyVersion(p.doc.version());
  }
  const texts = new Set(peers.map((p) => p.doc.text()));
  const status = $("#status");
  const synced = texts.size === 1 && net.inflight === 0;
  status.textContent = synced ? "converged ✓" : peers.some((p) => !p.online) ? "partitioned" : "syncing…";
  status.className = `badge ${synced ? "ok" : "busy"}`;
  $("#inflight").textContent = net.inflight;
  $("#delivered").textContent = net.delivered;
  $("#dups").textContent = net.dups;
  renderItems();
}

function prettyVersion(json) {
  const v = JSON.parse(json);
  const names = { 1: "Ada", 2: "Bora", 3: "Cem" };
  const parts = Object.entries(v).map(([s, n]) => `${names[s] ?? s}:${n}`);
  return parts.length ? parts.join("  ") : "—";
}

function renderTabs() {
  const tabs = $("#inspectTabs");
  tabs.innerHTML = "";
  peers.forEach((p, i) => {
    const b = document.createElement("button");
    b.type = "button";
    b.role = "tab";
    b.textContent = p.name;
    b.style.setProperty("--peer", p.color);
    b.setAttribute("aria-selected", i === inspected);
    b.onclick = () => { inspected = i; renderTabs(); renderItems(); };
    tabs.append(b);
  });
}

let itemsQueued = false;
function renderItems() {
  if (itemsQueued) return;
  itemsQueued = true;
  requestAnimationFrame(() => {
    itemsQueued = false;
    const items = JSON.parse(peers[inspected].doc.items());
    const box = $("#items");
    box.replaceChildren(...items.map((it) => {
      const s = document.createElement("span");
      s.className = `chip${it.deleted ? " dead" : ""}`;
      s.textContent = it.ch === " " ? "␣" : it.ch === "\n" ? "↵" : it.ch;
      s.style.setProperty("--site", SITE_COLORS[it.id.site] ?? "#94a3b8");
      s.title = `id ${it.id.site}:${it.id.seq} · lamport ${it.lamport}${it.deleted ? " · tombstone" : ""}`;
      return s;
    }));
  });
}

// ---------- controls ----------

function bindRange(id, out, fmt, apply) {
  const input = $(`#${id}`);
  const update = () => { apply(Number(input.value)); $(`#${out}`).textContent = fmt(input.value); };
  input.addEventListener("input", update);
  update();
}
bindRange("latency", "latencyOut", (v) => `${v} ms`, (v) => (net.latency = v));
bindRange("jitter", "jitterOut", (v) => `${v} ms`, (v) => (net.jitter = v));
bindRange("dup", "dupOut", (v) => `${v} %`, (v) => (net.dup = v / 100));

$("#chaos").addEventListener("click", () => {
  const words = ["kilim ", "weave ", "ip ", "desen ", "🧶", "loom ", "renk "];
  let n = 0;
  const timer = setInterval(() => {
    const p = peers[Math.floor(Math.random() * peers.length)];
    const len = p.doc.text().length;
    const ops = len > 8 && Math.random() < 0.3
      ? p.doc.delete(Math.floor(Math.random() * (len - 2)), 2)
      : p.doc.insert(Math.floor(Math.random() * (len + 1)), words[Math.floor(Math.random() * words.length)]);
    render(p);
    broadcast(p, ops);
    if (++n >= 24) clearInterval(timer);
  }, 90);
});

$("#reset").addEventListener("click", () => {
  Object.assign(net, { inflight: 0, delivered: 0, dups: 0 });
  createPeers();
});

createPeers();

// ---------- real cross-tab sync ----------

const STORAGE_KEY = "kilim-demo-ops";
const tabDoc = new Doc((Math.random() * 2 ** 32) >>> 0);
const tabEditor = $("#tabDoc");
const channel = "BroadcastChannel" in window ? new BroadcastChannel("kilim-demo") : null;

try {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved) tabDoc.applyRemote(saved);
} catch { /* storage unavailable: start empty */ }

function tabRender(remote) {
  const focused = document.activeElement === tabEditor;
  const a = remote && focused ? tabDoc.anchor(tabEditor.selectionStart) : null;
  const b = remote && focused ? tabDoc.anchor(tabEditor.selectionEnd) : null;
  return (applyChange) => {
    applyChange();
    if (tabEditor.value !== tabDoc.text()) tabEditor.value = tabDoc.text();
    if (a !== null) tabEditor.setSelectionRange(tabDoc.resolve(a), tabDoc.resolve(b));
    $("#tabVersion").textContent = tabDoc.version();
    try { localStorage.setItem(STORAGE_KEY, tabDoc.opsSince("{}")); } catch { /* ignore */ }
  };
}

$("#tabSite").textContent = tabDoc.site;
tabRender(false)(() => {});

tabEditor.addEventListener("input", () => {
  const ops = tabDoc.applyText(tabEditor.value);
  tabRender(false)(() => {});
  channel?.postMessage({ type: "ops", ops });
});

if (channel) {
  channel.onmessage = ({ data }) => {
    if (data.type === "ops") tabRender(true)(() => tabDoc.applyRemote(data.ops));
    if (data.type === "hello" || data.type === "hello-back") {
      channel.postMessage({ type: "ops", ops: tabDoc.opsSince(data.version) });
      if (data.type === "hello") channel.postMessage({ type: "hello-back", version: tabDoc.version() });
    }
  };
  channel.postMessage({ type: "hello", version: tabDoc.version() });
} else {
  tabEditor.placeholder = "BroadcastChannel is not available in this browser.";
}
