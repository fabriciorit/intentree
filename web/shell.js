// The showcase shell: a demo page adds one line,
//
//   <script type="module" src="../../web/shell.js"></script>
//
// and gets a "See the architecture" button that opens the real intentree (compiled to
// WebAssembly) beside the simulator: views drawn from the demo's own system.intent, an editor
// that re-checks as you type, the `where` query an agent would run, and a guided tour.
//
// It reads `demo.json` next to the page and listens to `window.demo` (see web/README.md for the
// contract). No dependencies. If intentree.wasm is not built the simulator is unaffected.

import { loadIntentree } from "./intentree.js";

const WASM_URL = new URL("intentree.wasm", import.meta.url);
const CSS_URL = new URL("shell.css", import.meta.url);
const DESCRIPTION =
  "intentree helps people follow what LLM coding agents build: the design intent is written as controlled-English sentences, checked against the code and drawn as views.";
const REDUCED_MOTION = matchMedia("(prefers-reduced-motion: reduce)");

// ---------------------------------------------------------------- small helpers

function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k === "class") el.className = v;
    else if (k === "text") el.textContent = v;
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) {
    if (c == null || c === false) continue;
    el.append(c.nodeType ? c : document.createTextNode(c));
  }
  return el;
}

const debounce = (fn, ms) => {
  let t;
  return (...a) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
};

async function fetchText(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  return res.text();
}

function ready() {
  return document.readyState === "loading"
    ? new Promise((r) => document.addEventListener("DOMContentLoaded", r, { once: true }))
    : Promise.resolve();
}

// ---------------------------------------------------------------- the project (files + wasm)

function createProject(demo, dirUrl) {
  const systemPath = demo.system || "system.intent";
  const paths = [...new Set([...(demo.sources || []), systemPath])];
  const s = {
    systemPath,
    original: "",
    files: {}, // everything fetched, with the original system file
    edited: "", // the text in the editor
    good: null, // files of the last version that checked clean
    loaded: null, // the files object currently inside the wasm instance
    it: null,
    error: null, // why intentree is not usable (wasm missing, fetch failed)
  };

  s.ready = (async () => {
    const filesP = Promise.all(
      paths.map(async (p) => {
        try {
          return [p, await fetchText(new URL(p, dirUrl))];
        } catch {
          return null; // a missing file is simply not in the project
        }
      }),
    );
    let it = null;
    try {
      it = await loadIntentree(WASM_URL);
    } catch (e) {
      s.error = { kind: "wasm", message: String(e && e.message ? e.message : e) };
    }
    for (const entry of await filesP) if (entry) s.files[entry[0]] = entry[1];
    s.original = s.edited = s.files[systemPath] ?? "";
    s.good = { ...s.files };
    s.it = it;
    s.done = true;
    return s;
  })();

  // Loads `files` into the wasm instance unless they are already there.
  s.use = (files) => {
    if (s.loaded === files) return;
    s.it.setFiles(files);
    s.loaded = files;
  };
  s.run = (files, args) => {
    s.use(files);
    return s.it.run(args);
  };
  return s;
}

// Parses what `check` prints: `error file:line: text`, indented hints, a summary line.
function parseCheck(res) {
  const issues = [];
  for (const line of (res.stdout + "\n" + res.stderr).split("\n")) {
    const m = /^(error|warning|gap)\s+(?:(\S+?):(\d+): )?(.*)$/.exec(line);
    if (m) {
      issues.push({ level: m[1], file: m[2] || null, line: m[3] ? Number(m[3]) : null, text: m[4], hint: "" });
    } else if (/^\s+hint:/.test(line) && issues.length) {
      issues[issues.length - 1].hint = line.replace(/^\s+hint:\s*/, "");
    } else if (/^\s+\S/.test(line) && issues.length && !issues[issues.length - 1].hint) {
      issues[issues.length - 1].text += " " + line.trim();
    }
  }
  const sum = /(\d+) errors?, (\d+) entities, (\d+) facts, (\d+) invariants, (\d+) rules, (\d+) gaps, (\d+) warnings?/.exec(res.stdout);
  const cov = /\((\d+)%\)/.exec(res.stdout);
  return {
    ok: res.code === 0,
    issues,
    errors: sum ? Number(sum[1]) : issues.filter((i) => i.level === "error").length,
    invariants: sum ? Number(sum[4]) : null,
    gaps: sum ? Number(sum[6]) : null,
    warnings: sum ? Number(sum[7]) : null,
    coverage: cov ? Number(cov[1]) : null,
    raw: res.stdout.trim().split("\n").pop() || res.stderr.trim(),
  };
}

function describeCheck(c) {
  const parts = [`${c.errors} error${c.errors === 1 ? "" : "s"}`];
  if (c.invariants != null) parts.push(`${c.invariants} invariant${c.invariants === 1 ? "" : "s"}`);
  if (c.gaps) parts.push(`${c.gaps} gap${c.gaps === 1 ? "" : "s"}`);
  if (c.warnings) parts.push(`${c.warnings} warning${c.warnings === 1 ? "" : "s"}`);
  if (c.coverage != null) parts.push(`${c.coverage}% covered`);
  return parts.join(", ");
}

// ---------------------------------------------------------------- the live highlight

// Finds the shapes of an entity in the shown SVG and pulses them. The SVG carries
// data-entity="<name>" (and data-state="<state>" on the shapes of a state machine).
function createHighlighter(getSvg, onNote) {
  let index = null; // Map<string, Element[]>
  let indexedFor = null;
  const lastPulse = new Map();
  const currentState = new Map(); // machine -> Element[] lit as "current state"

  const build = () => {
    const svg = getSvg();
    if (!svg) return null;
    if (indexedFor === svg) return index;
    index = new Map();
    for (const el of svg.querySelectorAll("[data-entity]")) {
      const entity = el.getAttribute("data-entity");
      const state = el.getAttribute("data-state");
      const key = state == null ? entity : `${entity}\u0000${state}`;
      if (!index.has(key)) index.set(key, []);
      index.get(key).push(el);
    }
    indexedFor = svg;
    currentState.clear();
    return index;
  };

  const pulse = (els, key) => {
    const now = performance.now();
    if (now - (lastPulse.get(key) || 0) < 120) return;
    lastPulse.set(key, now);
    for (const el of els) {
      el.classList.remove("it-pulse", "it-flash");
      el.getBoundingClientRect(); // restart the animation
      if (REDUCED_MOTION.matches) {
        el.classList.add("it-flash");
        setTimeout(() => el.classList.remove("it-flash"), 700);
      } else {
        el.classList.add("it-pulse");
        el.addEventListener("animationend", () => el.classList.remove("it-pulse"), { once: true });
      }
    }
  };

  return {
    reset() {
      indexedFor = null;
      index = null;
    },
    event(ev) {
      const idx = build();
      if (!idx || !ev) return;
      let hit = false;
      if (ev.entity != null && idx.has(String(ev.entity))) {
        pulse(idx.get(String(ev.entity)), String(ev.entity));
        hit = true;
      }
      const machine = ev.detail?.machine ?? ev.machine;
      const to = ev.detail?.to ?? ev.to;
      if (ev.kind === "state" && machine != null && to != null) {
        const els = idx.get(`${machine}\u0000${to}`);
        if (els) {
          for (const el of currentState.get(machine) || []) el.classList.remove("it-current");
          for (const el of els) el.classList.add("it-current");
          currentState.set(machine, els);
          pulse(els, `${machine}\u0000${to}`);
          hit = true;
        }
      }
      onNote(ev, hit);
    },
  };
}

// ---------------------------------------------------------------- the panel

function createShell(demo, project, dirUrl) {
  const views = demo.views || [];
  const tour = demo.tour || [];
  const ui = {
    open: false,
    tab: "views",
    viewId: views[0]?.id ?? "structure",
    zoom: "fit", // "fit" or a number
    tourAt: -1, // -1: not touring
  };
  const svgCache = new Map(); // viewId -> {files, svg} for the last good project
  let svgEl = null;

  // ----- skeleton
  let panel = document.getElementById("intentree-panel");
  if (!panel) panel = h("aside", { id: "intentree-panel" });
  panel.removeAttribute("hidden");
  panel.classList.add("it-panel");
  panel.setAttribute("aria-label", "intentree");
  panel.inert = true;
  document.body.append(panel); // the shell owns the layout: the panel is a column of the page itself

  const closeBtn = h("button", { class: "it-icon-btn", type: "button", "aria-label": "Close the panel", title: "Close", onclick: () => setOpen(false) }, "×");
  const header = h(
    "header",
    { class: "it-head" },
    h("div", { class: "it-head__text" }, h("h2", { class: "it-title", text: `${demo.title || "Demo"}: the architecture` }), h("p", { class: "it-sub", text: DESCRIPTION })),
    closeBtn,
  );

  const tabDefs = [
    ["views", "Views"],
    ["intent", "Intent"],
    ["where", "Where"],
    ["tour", "Tour"],
  ].filter(([id]) => id !== "tour" || tour.length);
  const tabButtons = {};
  const tabsBar = h(
    "div",
    { class: "it-tabs", role: "tablist" },
    tabDefs.map(([id, label]) => {
      const b = h("button", { class: "it-tab", type: "button", role: "tab", id: `it-tab-${id}`, onclick: () => setTab(id) }, label);
      tabButtons[id] = b;
      return b;
    }),
  );
  const tourCard = h("div", { class: "it-tourcard", hidden: true });
  const pages = {};
  const body = h("div", { class: "it-body" });
  for (const [id] of tabDefs) {
    pages[id] = h("section", { class: `it-page it-page--${id}`, role: "tabpanel", "aria-labelledby": `it-tab-${id}`, hidden: true });
    body.append(pages[id]);
  }
  const grip = h("div", { class: "it-grip", role: "separator", "aria-orientation": "vertical", "aria-label": "Resize the panel", title: "Drag to resize" });
  panel.replaceChildren(grip, header, tabsBar, tourCard, body);
  grip.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    grip.setPointerCapture(e.pointerId);
    const root = document.documentElement;
    root.classList.add("it-resizing");
    const move = (ev) => {
      const wide = matchMedia("(min-width: 861px)").matches;
      if (wide) root.style.setProperty("--it-w", `${Math.min(innerWidth - 320, Math.max(340, innerWidth - ev.clientX))}px`);
      else root.style.setProperty("--it-h", `${Math.min(innerHeight - 140, Math.max(200, innerHeight - ev.clientY))}px`);
    };
    const up = () => {
      root.classList.remove("it-resizing");
      grip.removeEventListener("pointermove", move);
      grip.removeEventListener("pointerup", up);
    };
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", up);
  });

  // ----- toggle
  const toggle = h("button", { class: "it-toggle__main", type: "button", "aria-controls": "intentree-panel", "aria-expanded": "false", onclick: () => setOpen(!ui.open) }, "See the architecture");
  const toggleTour = tour.length ? h("button", { class: "it-toggle__tour", type: "button", onclick: () => { setOpen(true); startTour(); } }, "Take the tour") : null;
  const toggleBox = h("div", { class: "it-toggle" }, toggle, toggleTour);
  document.body.append(toggleBox);

  function setOpen(open) {
    ui.open = open;
    document.documentElement.classList.toggle("it-open", open);
    panel.inert = !open;
    toggleBox.classList.toggle("is-open", open);
    toggle.setAttribute("aria-expanded", String(open));
    toggle.textContent = open ? "Hide the architecture" : "See the architecture";
    if (open) {
      renderActive();
      closeBtn.focus({ preventScroll: true });
    } else if (ui.tourAt >= 0) {
      endTour();
    }
  }

  function setTab(id) {
    ui.tab = id;
    for (const [tid] of tabDefs) {
      tabButtons[tid].setAttribute("aria-selected", String(tid === id));
      tabButtons[tid].classList.toggle("is-on", tid === id);
      pages[tid].hidden = tid !== id;
    }
    renderActive();
  }

  function renderActive() {
    if (!ui.open) return;
    ({ views: renderViews, intent: renderIntent, where: renderWhere, tour: renderTourPage })[ui.tab]?.();
  }

  // ----- the unavailable states
  function unavailable(into) {
    if (!project.done) return into.replaceChildren(h("div", { class: "it-empty" }, h("p", { text: "Loading intentree..." })));
    into.replaceChildren(
      h(
        "div",
        { class: "it-empty" },
        h("strong", { text: "intentree is not available on this page." }),
        project.error?.kind === "wasm"
          ? h("p", {}, "The WebAssembly build is missing. Build it with ", h("code", { text: "scripts/build-wasm.sh" }), " (it writes ", h("code", { text: "web/intentree.wasm" }), ") and reload. The simulator keeps working without it.")
          : h("p", { text: "The project could not be loaded." }),
      ),
    );
  }

  // =============================================================== Views tab
  const viewsPage = pages.views;
  const pillRow = h("div", { class: "it-pills", role: "group", "aria-label": "Views" });
  const question = h("p", { class: "it-question" });
  const note = h("p", { class: "it-note", hidden: true });
  const zoomOut = h("button", { class: "it-btn it-btn--small", type: "button", title: "Zoom out", "aria-label": "Zoom out", onclick: () => stepZoom(-1) }, "−");
  const zoomIn = h("button", { class: "it-btn it-btn--small", type: "button", title: "Zoom in", "aria-label": "Zoom in", onclick: () => stepZoom(1) }, "+");
  const zoomFit = h("button", { class: "it-btn it-btn--small", type: "button", title: "Fit to width", onclick: () => { ui.zoom = "fit"; applyZoom(); } }, "Fit");
  const zoomOne = h("button", { class: "it-btn it-btn--small", type: "button", title: "Actual size", onclick: () => { ui.zoom = 1; applyZoom(); } }, "1:1");
  const zoomLabel = h("span", { class: "it-zoom__label", "aria-live": "off" });
  const tourStart = tour.length ? h("button", { class: "it-btn it-btn--accent", type: "button", onclick: () => startTour() }, "Start tour") : null;
  const ticker = h("span", { class: "it-ticker", "aria-hidden": "true" });
  const toolbar = h("div", { class: "it-toolbar" }, h("div", { class: "it-zoom" }, zoomOut, zoomLabel, zoomIn, zoomFit, zoomOne), ticker, tourStart);
  const stage = h("div", { class: "it-stage", tabindex: "0", "aria-label": "View drawing, scrollable" });
  viewsPage.append(pillRow, question, note, toolbar, stage);

  for (const v of views) {
    v.button = h("button", { class: "it-pill", type: "button", onclick: () => selectView(v.id) }, v.label || v.id);
    pillRow.append(v.button);
  }

  const highlighter = createHighlighter(
    () => svgEl,
    (ev, hit) => {
      if (!ui.open || ui.tab !== "views") return;
      const text = `${ev.entity} · ${ev.kind}${ev.detail?.to != null ? " → " + ev.detail.to : ev.to != null ? " → " + ev.to : ""}`;
      ticker.textContent = text;
      ticker.classList.toggle("is-miss", !hit);
      ticker.title = hit ? "Pulsing in the drawing" : "This entity is not in the shown view";
    },
  );

  function selectView(id) {
    ui.viewId = id;
    renderViews();
  }

  function viewSvg(id) {
    const cached = svgCache.get(id);
    if (cached && cached.files === project.good) return cached;
    const res = project.run(project.good, ["view", id, "--format", "svg"]);
    const entry = { files: project.good, svg: res.code === 0 ? res.stdout : null, err: res.code === 0 ? null : res.stderr.trim() || res.stdout.trim() };
    svgCache.set(id, entry);
    return entry;
  }

  function renderViews() {
    for (const v of views) {
      v.button.classList.toggle("is-on", v.id === ui.viewId);
      v.button.setAttribute("aria-pressed", String(v.id === ui.viewId));
    }
    const meta = views.find((v) => v.id === ui.viewId);
    question.textContent = meta?.question || "";
    question.hidden = !meta?.question;
    toolbar.hidden = !project.it;
    if (!project.it) {
      stage.replaceChildren();
      svgEl = null;
      const box = h("div");
      unavailable(box);
      stage.replaceChildren(box);
      return;
    }
    renderNote();
    let entry;
    try {
      entry = viewSvg(ui.viewId);
    } catch (e) {
      entry = { svg: null, err: e.message };
    }
    if (entry.svg == null) {
      svgEl = null;
      highlighter.reset();
      stage.replaceChildren(h("pre", { class: "it-error", text: entry.err || "the view could not be drawn" }));
      return;
    }
    const holder = h("div", { class: "it-paper" });
    holder.innerHTML = entry.svg;
    const el = holder.querySelector("svg");
    el.classList.add("it-svg");
    el.setAttribute("role", "img");
    el.setAttribute("aria-label", meta?.question || meta?.label || ui.viewId);
    el.natural = { w: Number(el.getAttribute("width")) || 800, h: Number(el.getAttribute("height")) || 600 };
    stage.scrollTop = 0;
    stage.replaceChildren(holder);
    svgEl = el;
    highlighter.reset();
    applyZoom();
  }

  function renderNote() {
    const bad = lastCheck && !lastCheck.ok;
    note.hidden = !bad;
    if (bad) note.textContent = `Showing the last valid version: the edited intent has ${lastCheck.errors} error${lastCheck.errors === 1 ? "" : "s"} (see the Intent tab).`;
  }

  function currentScale() {
    if (!svgEl) return 1;
    if (ui.zoom === "fit") {
      const room = stage.clientWidth - 24;
      return Math.max(0.15, Math.min(1.25, room / svgEl.natural.w));
    }
    return ui.zoom;
  }

  function applyZoom() {
    if (!svgEl) return;
    const k = currentScale();
    svgEl.style.width = `${Math.round(svgEl.natural.w * k)}px`;
    svgEl.style.height = `${Math.round(svgEl.natural.h * k)}px`;
    zoomLabel.textContent = `${Math.round(k * 100)}%`;
    zoomFit.classList.toggle("is-on", ui.zoom === "fit");
    zoomOne.classList.toggle("is-on", ui.zoom === 1);
  }

  function stepZoom(dir, anchor) {
    const k = currentScale();
    const next = Math.min(3, Math.max(0.15, +(k * (dir > 0 ? 1.25 : 0.8)).toFixed(3)));
    const before = anchor && stage.scrollWidth ? { fx: (stage.scrollLeft + anchor.x) / stage.scrollWidth, fy: (stage.scrollTop + anchor.y) / stage.scrollHeight } : null;
    ui.zoom = next;
    applyZoom();
    if (before) {
      stage.scrollLeft = before.fx * stage.scrollWidth - anchor.x;
      stage.scrollTop = before.fy * stage.scrollHeight - anchor.y;
    }
  }

  stage.addEventListener(
    "wheel",
    (e) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const r = stage.getBoundingClientRect();
      stepZoom(e.deltaY < 0 ? 1 : -1, { x: e.clientX - r.left, y: e.clientY - r.top });
    },
    { passive: false },
  );
  new ResizeObserver(() => ui.zoom === "fit" && applyZoom()).observe(stage);

  // =============================================================== Intent tab
  const intentPage = pages.intent;
  let lastCheck = null;
  const gutter = h("div", { class: "it-gutter", "aria-hidden": "true" });
  const textarea = h("textarea", { class: "it-code", spellcheck: "false", autocapitalize: "off", autocomplete: "off", wrap: "off", "aria-label": `${project.systemPath}, editable` });
  const editor = h("div", { class: "it-editor" }, gutter, textarea);
  const status = h("p", { class: "it-status", role: "status" });
  const issuesBox = h("ul", { class: "it-issues" });
  const resetBtn = h("button", { class: "it-btn", type: "button", onclick: () => { textarea.value = project.original; onEdit(); } }, "Reset");
  const fileLabel = h("span", { class: "it-file", text: project.systemPath });
  const intro = h("p", { class: "it-hint", text: "This is the file that drives the views. Edit a sentence: intentree re-checks it as you type and the drawings follow." });
  intentPage.append(intro, h("div", { class: "it-editor-head" }, fileLabel, resetBtn), editor, status, issuesBox);
  let intentBuilt = false;

  function drawGutter() {
    const n = textarea.value.split("\n").length;
    const bad = new Set((lastCheck?.issues || []).filter((i) => i.level === "error" && i.line).map((i) => i.line));
    const frag = document.createDocumentFragment();
    for (let i = 1; i <= n; i++) frag.append(h("span", { class: bad.has(i) ? "is-bad" : "", text: String(i) }));
    gutter.replaceChildren(frag);
    gutter.scrollTop = textarea.scrollTop;
  }
  textarea.addEventListener("scroll", () => { gutter.scrollTop = textarea.scrollTop; });

  function runCheck() {
    if (!project.it) return;
    let c;
    try {
      c = parseCheck(project.run({ ...project.files, [project.systemPath]: project.edited }, ["check"]));
    } catch (e) {
      c = { ok: false, issues: [{ level: "error", file: null, line: null, text: e.message, hint: "" }], errors: 1, raw: e.message };
    }
    lastCheck = c;
    if (c.ok) {
      project.good = { ...project.files, [project.systemPath]: project.edited };
      svgCache.clear();
    }
    renderStatus();
    drawGutter();
    if (ui.tab === "views" && c.ok) renderViews();
    else renderNote();
  }

  function renderStatus() {
    const c = lastCheck;
    if (!c) return;
    status.className = `it-status ${c.ok ? (c.warnings || c.gaps ? "is-warn" : "is-ok") : "is-bad"}`;
    status.textContent = describeCheck(c) + (c.ok ? "" : ". Views keep showing the last valid version.");
    const items = c.issues.filter((i) => i.level === "error" || i.level === "warning" || i.level === "gap");
    issuesBox.replaceChildren(
      ...items.map((i) => {
        const where = i.line != null ? `line ${i.line}` : i.file || "";
        const li = h(
          "li",
          { class: `it-issue it-issue--${i.level}` },
          h(
            i.line != null ? "button" : "div",
            { class: "it-issue__main", type: i.line != null ? "button" : null, onclick: i.line != null ? () => gotoLine(i.line) : null },
            h("span", { class: "it-issue__where", text: where ? `${i.level} · ${where}` : i.level }),
            h("span", { class: "it-issue__text", text: i.text }),
          ),
          i.hint ? h("div", { class: "it-issue__hint", text: i.hint }) : null,
        );
        return li;
      }),
    );
  }

  function gotoLine(n) {
    const lines = textarea.value.split("\n");
    let start = 0;
    for (let i = 0; i < n - 1; i++) start += lines[i].length + 1;
    textarea.focus({ preventScroll: true });
    textarea.setSelectionRange(start, start + (lines[n - 1] || "").length);
    const lh = parseFloat(getComputedStyle(textarea).lineHeight) || 20;
    textarea.scrollTop = Math.max(0, (n - 1) * lh - textarea.clientHeight / 2);
    gutter.scrollTop = textarea.scrollTop;
  }

  const onEditDebounced = debounce(() => {
    project.edited = textarea.value;
    runCheck();
  }, 250);
  function onEdit() {
    drawGutter();
    onEditDebounced();
  }
  textarea.addEventListener("input", onEdit);
  function renderIntent() {
    if (!project.it) return unavailable(intentPage);
    if (!intentBuilt) {
      textarea.value = project.edited;
      intentBuilt = true;
      drawGutter();
      renderStatus();
    }
  }

  // =============================================================== Where tab
  const wherePage = pages.where;
  const whereInput = h("input", { class: "it-input", type: "text", placeholder: "Where would I change...?", "aria-label": "Where would I change", autocomplete: "off" });
  const whereForm = h("form", { class: "it-whereform", onsubmit: (e) => { e.preventDefault(); runWhere(whereInput.value); } }, whereInput, h("button", { class: "it-btn it-btn--accent", type: "submit" }, "Ask"));
  const chips = h(
    "div",
    { class: "it-chips" },
    (demo.whereExamples || []).map((q) => h("button", { class: "it-chip", type: "button", onclick: () => { whereInput.value = q; runWhere(q); } }, q)),
  );
  const whereOut = h("div", { class: "it-where-out" });
  const whereHint = h("p", { class: "it-hint", text: "This is what a coding agent runs before it opens any code: the ranked components, their files and symbols, and the rules that cite your words." });
  wherePage.append(whereForm, chips, whereHint, whereOut);

  function runWhere(q) {
    q = q.trim();
    if (!q || !project.it) return;
    let res;
    try {
      res = project.run(project.good, ["where", ...q.split(/\s+/)]);
    } catch (e) {
      res = { code: 2, stdout: "", stderr: e.message };
    }
    whereOut.replaceChildren(renderWhereText(res.code === 0 ? res.stdout : res.stderr || res.stdout, res.code !== 0));
  }

  function renderWhereText(text, isError) {
    if (isError) return h("pre", { class: "it-error", text });
    const root = h("div", { class: "it-where" });
    let card = null;
    let section = null;
    for (const line of text.replace(/\n+$/, "").split("\n")) {
      let m;
      if ((m = /^where: (.*)$/.exec(line))) {
        root.append(h("p", { class: "it-where__head", text: m[0] }));
        card = section = null;
      } else if ((m = /^(\d+) (.+?) · (.*)$/.exec(line))) {
        card = h("div", { class: "it-card" }, h("div", { class: "it-card__title" }, h("span", { class: "it-card__rank", text: m[1] }), h("strong", { class: "it-card__name", text: m[2] }), h("span", { class: "it-card__meta", text: m[3] })));
        root.append(card);
        section = null;
      } else if (/^\s+\S/.test(line) && card) {
        card.append(whereLine(line.trim()));
      } else if ((m = /^(nearby rules|more\b.*)$/.exec(line))) {
        section = h("div", { class: "it-card it-card--plain" });
        if (m[1] === "nearby rules") section.append(h("div", { class: "it-card__label", text: "nearby rules" }));
        else section.append(h("div", { class: "it-line" }, h("span", { class: "it-line__k", text: "more" }), h("span", { class: "it-line__v", text: line.replace(/^more\s*/, "") })));
        root.append(section);
        card = null;
      } else if (line.trim() && section) {
        const r = /^- \[([^\]]+)\] (.*)$/.exec(line);
        section.append(
          r
            ? h("div", { class: "it-line it-line--rule" }, h("span", { class: "it-line__k", text: r[1] }), h("span", { class: "it-line__v", text: r[2] }))
            : h("div", { class: "it-line", text: line }),
        );
      } else if (line.trim()) {
        root.append(h("pre", { class: "it-raw", text: line }));
      }
    }
    return root;
  }

  function whereLine(t) {
    const m = /^(files|symbols|uses|used by|same state|reads|writes|runs on|implements)\b\s*(.*)$/.exec(t);
    if (m) return h("div", { class: "it-line" }, h("span", { class: "it-line__k", text: m[1] }), h("span", { class: "it-line__v it-mono", text: m[2] }));
    if (t.startsWith("- ")) return h("div", { class: "it-line it-line--rule" }, h("span", { class: "it-line__k", text: "rule" }), h("span", { class: "it-line__v", text: t.slice(2) }));
    return h("div", { class: "it-line", text: t });
  }

  function renderWhere() {
    if (!project.it) return unavailable(wherePage);
  }

  // =============================================================== Tour
  const tourPage = pages.tour;

  function renderTourPage() {
    const list = h(
      "ol",
      { class: "it-steps" },
      tour.map((s, i) =>
        h("li", {}, h("button", { class: `it-step ${i === ui.tourAt ? "is-on" : ""}`, type: "button", onclick: () => goStep(i) }, h("span", { class: "it-step__n", text: String(i + 1) }), h("span", { class: "it-step__t", text: s.title }))),
      ),
    );
    tourPage.replaceChildren(
      h("p", { class: "it-hint", text: "A short walk through the demo: each step shows a view of the architecture and, when it has one, triggers something in the simulator so you can see the two agree." }),
      ui.tourAt < 0 ? h("button", { class: "it-btn it-btn--accent", type: "button", onclick: () => startTour() }, "Start tour") : null,
      list,
    );
  }

  function renderTourCard() {
    const active = ui.tourAt >= 0;
    tourCard.hidden = !active;
    if (!active) return tourCard.replaceChildren();
    const s = tour[ui.tourAt];
    const dots = h(
      "div",
      { class: "it-dots", role: "group", "aria-label": "Tour steps" },
      tour.map((_, i) => h("button", { class: `it-dot ${i === ui.tourAt ? "is-on" : ""}`, type: "button", "aria-label": `Step ${i + 1}`, "aria-current": i === ui.tourAt ? "step" : null, onclick: () => goStep(i) })),
    );
    tourCard.replaceChildren(
      h("div", { class: "it-tourcard__top" }, h("span", { class: "it-tourcard__count", text: `Step ${ui.tourAt + 1} of ${tour.length}` }), h("button", { class: "it-link", type: "button", onclick: () => endTour() }, "End tour")),
      h("strong", { class: "it-tourcard__title", text: s.title }),
      h("p", { class: "it-tourcard__text", text: s.text }),
      h(
        "div",
        { class: "it-tourcard__nav" },
        h("button", { class: "it-btn", type: "button", disabled: ui.tourAt === 0, onclick: () => goStep(ui.tourAt - 1) }, "Previous"),
        dots,
        ui.tourAt === tour.length - 1 ? h("button", { class: "it-btn it-btn--accent", type: "button", onclick: () => endTour() }, "Finish") : h("button", { class: "it-btn it-btn--accent", type: "button", onclick: () => goStep(ui.tourAt + 1) }, "Next"),
      ),
    );
  }

  function startTour() {
    if (!tour.length) return;
    setOpen(true);
    goStep(0);
  }

  function goStep(i) {
    if (i < 0 || i >= tour.length) return;
    ui.tourAt = i;
    const s = tour[i];
    if (s.view) ui.viewId = s.view;
    renderTourCard();
    if (tourStart) tourStart.hidden = true;
    setTab("views");
    const act = s.do && window.demo?.actions?.[s.do];
    if (typeof act === "function") {
      try {
        act.call(window.demo.actions);
      } catch (e) {
        console.warn("tour action failed:", s.do, e);
      }
    }
  }

  function endTour() {
    ui.tourAt = -1;
    if (tourStart) tourStart.hidden = false;
    renderTourCard();
    if (ui.tab === "tour") renderTourPage();
  }

  // =============================================================== boot of the panel
  setTab("views");
  project.ready.then(() => {
    if (!project.it) {
      renderActive();
      return;
    }
    // the check at load: its line is the first thing the Intent tab shows
    const c = parseCheck(project.run(project.good, ["check"]));
    lastCheck = c;
    if (!c.ok) console.warn("intentree check:", c.raw);
    renderStatus();
    renderActive();
    if (new URLSearchParams(location.search).has("architecture")) setOpen(true);
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && ui.open && !e.defaultPrevented && panel.contains(document.activeElement)) setOpen(false);
  });

  return { highlight: (ev) => highlighter.event(ev), open: () => setOpen(true) };
}

// ---------------------------------------------------------------- start

(async function main() {
  await ready();
  if (window.__intentreeShell) return;
  window.__intentreeShell = true;
  const dirUrl = new URL("./", location.href);
  let demo;
  try {
    demo = JSON.parse(await fetchText(new URL("demo.json", dirUrl)));
  } catch (e) {
    console.info("intentree shell: no demo.json next to this page, nothing to show.", e.message);
    return;
  }
  if (!document.querySelector('link[data-intentree-shell]')) {
    document.head.append(h("link", { rel: "stylesheet", href: CSS_URL.href, "data-intentree-shell": "" }));
  }
  const project = createProject(demo, dirUrl);
  const shell = createShell(demo, project, dirUrl);

  // window.demo may be defined slightly after this module runs: wait for it.
  let attempts = 0;
  const attach = () => {
    if (typeof window.demo?.onEvent === "function") {
      window.demo.onEvent((ev) => shell.highlight(ev));
      return;
    }
    if (++attempts < 100) setTimeout(attach, 100);
  };
  attach();
})();
