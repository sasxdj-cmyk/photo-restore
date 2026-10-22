/* Photo Restore - logica interfaccia (vanilla JS, nessun bundler). */
(function () {
  "use strict";

  const T = window.__TAURI__;
  if (!T) {
    document.body.innerHTML = "<p>Errore: ambiente Tauri non disponibile.</p>";
    return;
  }
  const invoke = T.core.invoke;
  const listen = T.event.listen;

  const $ = (id) => document.getElementById(id);
  const t = (k) => window.I18N.t(k);
  const tf = (k, par) =>
    t(k).replace(/\{(\w+)\}/g, (_, id) => (par[id] !== undefined ? par[id] : `{${id}}`));

  /* ---------- stato locale ---------- */
  const state = {
    imported: false,
    currentTab: "detect",
    editmode: "none",
    sel: null,       // { type: "rect", x, y, w, h } | { type: "poly", pts: [...] }
    quads: [],
    zoom: 1,
    panX: 0,
    panY: 0,
    imgW: 0,
    imgH: 0,
    dividePos: 0.5,
    dragging: null,
    lastPanel: null,
    origW: null,
    origH: null,
    fusioneAttiva: false,
    fusioneTimer: null,
    stepsDone: new Set(),
  };
  let stepsOrder = [];
  let stepsUndone = [];

  /* ---------- log ---------- */
  function log(msg, cls) {
    const box = $("log-content");
    const time = new Date().toLocaleTimeString("it-IT");
    const line = document.createElement("div");
    if (cls) line.className = cls;
    line.textContent = `[${time}] ${msg}`;
    box.appendChild(line);
    box.scrollTop = box.scrollHeight;
  }

  function showMessage(text, ok) {
    const el = $("message");
    el.textContent = text;
    el.classList.toggle("ok", !!ok);
    el.classList.remove("hidden");
    clearTimeout(showMessage._t);
    showMessage._t = setTimeout(() => el.classList.add("hidden"), ok ? 3500 : 8000);
  }

  /* ---------- invocazione comandi ---------- */
  async function cmd(name, ...args) {
    try {
      log(`comando ${name}`);
      const out = await invoke(name, ...args);
      if (out && out.imported !== undefined) updateState(out);
      return out;
    } catch (err) {
      const msg = typeof err === "string" ? err : (err && err.message) || String(err);
      if (msg.includes("annullata") || msg.includes("annullato")) {
        log(`${name}: operazione annullata`, "ok");
      } else {
        showMessage(msg);
        log(`${name} ERRORE: ${msg}`, "err");
      }
      throw err;
    }
  }

  /* ---------- aggiornamento stato ---------- */
  function UpdateStepsUI() {
    document.querySelectorAll("#tool-nav .step").forEach((b) => {
      const name = b.dataset.step;
      b.disabled = !state.imported;
      b.classList.toggle("active", state.imported && name === (state.lastPanel || state.currentTab));
      b.classList.toggle("done", state.stepsDone.has(name));
    });
  }
  function markStepDone(tab) {
    if (!state.stepsDone.has(tab)) {
      state.stepsDone.add(tab);
      // spunta aggiunta solo dopo un'operazione applicata: la rimozione
      // con Annulla segue l'ordine di applicazione.
      if (!stepsOrder.includes(tab)) stepsOrder.push(tab);
      stepsUndone = [];
      UpdateStepsUI();
    }
  }
  function unmarkLastStep() {
    const last = stepsOrder.pop();
    if (last) {
      state.stepsDone.delete(last);
      stepsUndone.push(last);
      UpdateStepsUI();
    }
  }
  document.querySelectorAll("#tool-nav .step").forEach((b) => {
    b.addEventListener("click", () => focusTab(b.dataset.step, { open: true }));
  });
  function updateState(s) {
    state.imported = !!s.imported;
    $("empty-state").classList.toggle("hidden", state.imported);
    ["btn-undo", "btn-redo", "btn-reset"].forEach((id) => ($(id).disabled = !s.imported));
    $("btn-undo").disabled = !s.can_undo;
    $("btn-redo").disabled = !s.can_redo;
    if (!state.imported) {
      $("doc-info").textContent = "--";
      UpdateStepsUI();
    }

    // Pannelli stile Photoshop: visibili appena c'e' una foto, ciascuno
    // riducibile dai rispettivi titoli.
    document
      .querySelectorAll('#tools .tool')
      .forEach((el) => el.classList.toggle("hidden", !state.imported));
    aggiornaFast();

    if (!state.imported) return;

    // Anteprime via comando interno (base64): nessun percorso su disco.
    refreshPreviews();

    state.imgW = s.current_width;
    state.imgH = s.current_height;
    state.origW = s.original_width || state.origW;
    state.origH = s.original_height || state.origH;
    // i campi delle dimensioni mostrano sempre la dimensione attuale:
    // i pulsanti rapidi (2x/4x/8x) la aggiorna senza dover scrivere nulla
    $("resize-w").value = state.imgW;
    $("resize-h").value = state.imgH;
    fitCompareToImage();
    // resta sull'ultimo pannello utilizzato; al primo import restano chiusi
    focusTab(state.lastPanel, { open: !!state.lastPanel });

    $("info-original").textContent = tf("info-originale", { w: s.original_width, h: s.original_height }) +
      (s.original_file_size ? ` ${fmtBytes(s.original_file_size)},` : "");
    $("info-current").textContent = tf("info-attuale", { w: s.current_width, h: s.current_height });
    $("info-method").textContent = s.method ? tf("info-ultima", { m: s.method }) : "";
    $("info-file").textContent = s.original_path ? fileName(s.original_path) : "--";

    const mp = (state.imgW * state.imgH) / 1e6;
    $("doc-info").textContent = s.original_path
      ? `${fileName(s.original_path)} · ${state.imgW}×${state.imgH} px · ${mp.toFixed(2).replace(".", ",")} MP`
      : "--";

    aggiornaFusioneUI(s);
    $("export-info").textContent = "";
    aggiornaFast();
  }

  let previewsSeq = 0;
  async function refreshPreviews() {
    const seq = ++previewsSeq;
    try {
      const d = await invoke("preview_images");
      if (seq !== previewsSeq || !d) return;
      if (!d.current) log("il backend non ha fornito l'anteprima", "err");
      if (d.current) $("img-after").src = d.current;
      if (d.original) $("img-before").src = d.original;
    } catch (err) {
      log("anteprima non disponibile: " + err, "err");
    }
  }

  function fileName(p) {
    try { return p.split(/[\\/]/).pop(); } catch { return p; }
  }
  function fmtBytes(n) {
    if (n > 1024 * 1024) return `${(n / 1024 / 1024).toFixed(2)} MB`;
    if (n > 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${n} B`;
  }

  /* ---------- visualizzazione ---------- */
  function applyStage() {
    $("stage").style.transform = `translate(${state.panX}px, ${state.panY}px) scale(${state.zoom})`;
    $("zoom-label").textContent = `${Math.round(state.zoom * 100)}%`;
    $("zoom-range").value = Math.min(400, Math.max(5, Math.round(state.zoom * 100)));
    drawOverlays();
  }

  function setZoom(z, cx, cy) {
    const rect = $("viewport").getBoundingClientRect();
    const fx = (cx ?? rect.width / 2) - state.panX;
    const fy = (cy ?? rect.height / 2) - state.panY;
    const nc = Math.min(16, Math.max(0.05, z));
    state.panX -= fx * (nc / state.zoom) - fx;
    state.panY -= fy * (nc / state.zoom) - fy;
    state.zoom = nc;
    applyStage();
  }

  function fitToView() {
    if (!state.imgW) return;
    const r = $("viewport").getBoundingClientRect();
    const pad = 28;
    const zx = (r.width - pad) / state.imgW;
    const zy = (r.height - pad) / state.imgH;
    const z = Math.min(zx, zy, 1.5);
    state.zoom = z;
    state.panX = (r.width - state.imgW * z) / 2;
    state.panY = (r.height - state.imgH * z) / 2;
    applyStage();
  }

  function fitCompareToImage() {
    const c = $("compare");
    c.style.width = `${state.imgW}px`;
    c.style.height = `${state.imgH}px`;
    $("after-clip").style.width = "100%";
    updateDivider();
  }

  function updateDivider() {
    if (!$("compare").classList.contains("mode-divisore")) {
      // fuori dalla modalita' divisore niente clip: la foto elaborata s'inquadra intera
      $("after-clip").style.clipPath = "none";
      return;
    }
    $("divider").style.left = `${state.dividePos * 100}%`;
    $("divider").style.height = "100%";
    $("after-clip").style.clipPath = `inset(0 ${100 - state.dividePos * 100}% 0 0)`;
  }

  function setDividerFromClientX(clientX) {
    const r = $("viewport").getBoundingClientRect();
    const fx = (clientX - r.left - state.panX) / (state.imgW * state.zoom);
    state.dividePos = Math.min(0.98, Math.max(0.02, fx));
    updateDivider();
  }

  /* ---------- overlays ---------- */
  function drawOverlays() {
    const svg = $("overlays");
    svg.innerHTML = "";
    if (!state.imported) return;
    svg.setAttribute("viewBox", `0 0 ${state.imgW} ${state.imgH}`);

    // Quadri rilevati
    for (const q of state.quads) {
      const pts = q.corners.map((c) => `${c[0] * state.imgW},${c[1] * state.imgH}`).join(" ");
      const poly = document.createElementNS("http://www.w3.org/2000/svg", "polygon");
      poly.setAttribute("points", pts);
      poly.setAttribute("class", "quad");
      poly.addEventListener("click", (ev) => {
        ev.stopPropagation();
        selectQuad(q);
      });
      svg.appendChild(poly);
    }

    // Selezione corrente
    if (state.sel) {
      if (state.sel.type === "rect") {
        const r = document.createElementNS("http://www.w3.org/2000/svg", "rect");
        r.setAttribute("x", state.sel.x);
        r.setAttribute("y", state.sel.y);
        r.setAttribute("width", state.sel.w);
        r.setAttribute("height", state.sel.h);
        r.setAttribute("class", "rect");
        svg.appendChild(r);
      } else if (state.sel.type === "poly") {
        const pts = state.sel.pts.map((p) => `${p[0]},${p[1]}`).join(" ");
        const poly = document.createElementNS("http://www.w3.org/2000/svg", "polygon");
        poly.setAttribute("points", pts);
        poly.setAttribute("class", "rect");
        svg.appendChild(poly);
        for (const p of state.sel.pts) {
          const c = document.createElementNS("http://www.w3.org/2000/svg", "circle");
          c.setAttribute("cx", p[0]);
          c.setAttribute("cy", p[1]);
          c.setAttribute("r", Math.max(4, 5 / state.zoom));
          c.setAttribute("class", "point");
          svg.appendChild(c);
        }
      }
    }
  }

  function selectQuad(q) {
    state.sel = {
      type: "detail",
      corners: q.corners,
      rect: bboxOf(q.corners),
    };
    $("detect-result").classList.remove("hidden");
    $("detect-result").textContent =
      `Foto evidenziata: premi "Applica il ritaglio". Se gli angoli non sono precisi, ritaglia a mano con il riquadro o i 4 angoli.`;
    $("manual-actions").classList.remove("hidden");
    $("btn-apply-crop").classList.remove("hidden");
    $("btn-apply-persp").classList.remove("hidden");
    setMode("none");
    drawOverlays();
  }

  function bboxOf(corners) {
    const xs = corners.map((c) => c[0] * state.imgW);
    const ys = corners.map((c) => c[1] * state.imgH);
    const x0 = Math.min(...xs), y0 = Math.min(...ys);
    return { x: Math.round(x0), y: Math.round(y0), w: Math.round(Math.max(...xs) - x0), h: Math.round(Math.max(...ys) - y0) };
  }

  /* ---------- modalita' di modifica ---------- */
  function setMode(mode) {
    state.editmode = mode;
    document.querySelectorAll('input[name=editmode]').forEach((r) => (r.checked = r.value === mode));
    const manualActions = state.sel && (mode === "none" || mode === "crop" || mode === "persp");
    $("manual-actions").classList.toggle("hidden", !(state.sel && mode === "none"));
    $("viewport").style.cursor = mode === "none" ? "default" : "crosshair";
    log(`modalita' imposta: ${mode}`);
  }

  function toImageCoords(ev) {
    const r = $("viewport").getBoundingClientRect();
    const vx = (ev.clientX - r.left - state.panX) / state.zoom;
    const vy = (ev.clientY - r.top - state.panY) / state.zoom;
    return [
      Math.min(state.imgW, Math.max(0, vx)),
      Math.min(state.imgH, Math.max(0, vy)),
    ];
  }

  /* ---------- mouse sul viewport ---------- */
  $("viewport").addEventListener("pointerdown", async (ev) => {
    if (!state.imported) return;
    if (ev.button !== 0) return;
    const [x, y] = toImageCoords(ev);
    if (state.editmode === "crop") {
      state.dragging = { type: "rect", startX: x, startY: y };
      state.sel = { type: "rect", x: Math.round(x), y: Math.round(y), w: 0, h: 0 };
      $("viewport").setPointerCapture(ev.pointerId);
      drawOverlays();
      $("manual-actions").classList.remove("hidden");
      $("btn-apply-crop").classList.remove("hidden");
      $("btn-apply-persp").classList.add("hidden");
    } else if (state.editmode === "persp") {
      if (!state.sel || state.sel.type !== "poly" || state.sel.pts.length >= 4) {
        state.sel = { type: "poly", pts: [] };
      }
      state.sel.pts.push([Math.round(x), Math.round(y)]);
      drawOverlays();
      if (state.sel.pts.length === 4) {
        orderPoly();
        $("manual-actions").classList.remove("hidden");
        $("btn-apply-persp").classList.remove("hidden");
        $("btn-apply-crop").classList.add("hidden");
        $("detect-result").classList.remove("hidden");
        $("detect-result").textContent = t("quattro-angoli");
      }
    } else if (state.editmode === "bg") {
      // Rimozione sfondo: il colore del punto cliccato diventa trasparente.
      const tol = Number($("bg-tolerance").value) || 10;
      const out = await cmd("remove_background", {
        x: Math.round(x),
        y: Math.round(y),
        tolerance: tol,
      });
      if (out) {
        showMessage(t("sfondo-riclicca"), true);
      }
    }
  });

  $("viewport").addEventListener("pointermove", (ev) => {
    if (state.dragging && state.dragging.type === "rect") {
      const [x, y] = toImageCoords(ev);
      const x0 = Math.min(state.dragging.startX, x);
      const y0 = Math.min(state.dragging.startY, y);
      const x1 = Math.max(state.dragging.startX, x);
      const y1 = Math.max(state.dragging.startY, y);
      state.sel = { type: "rect", x: Math.round(x0), y: Math.round(y0), w: Math.round(x1 - x0), h: Math.round(y1 - y0) };
      drawOverlays();
    }
  });

  $("viewport").addEventListener("pointerup", () => {
    if (state.dragging) {
      if (!(state.sel && state.sel.w > 3 && state.sel.h > 3)) {
        state.sel = null;
        $("detect-result").classList.add("hidden");
        $("manual-actions").classList.add("hidden");
      }
      state.dragging = null;
      drawOverlays();
    }
  });

  function orderPoly() {
    // Ordina i punti in senso orario partendo dal piu' alto-sinistra.
    const pts = state.sel.pts;
    const cx = pts.reduce((a, p) => a + p[0], 0) / pts.length;
    const cy = pts.reduce((a, p) => a + p[1], 0) / pts.length;
    pts.sort((a, b) => Math.atan2(a[1] - cy, a[0] - cx) - Math.atan2(b[1] - cy, b[0] - cx));
    let start = 0, minSum = Infinity;
    pts.forEach((p, i) => {
      const s = p[0] + p[1];
      if (s < minSum) { minSum = s; start = i; }
    });
    state.sel.pts = pts.slice(start).concat(pts.slice(0, start));
  }

  function focusTab(tab, opts = {}) {
    const apri = !!opts.open;
    if (tab) {
      state.currentTab = tab;
      if (apri) state.lastPanel = tab;
    }
    const target = tab || state.lastPanel || "detect";
    const available = state.imported
      ? ["detect", "resize", "bg", "volto", "adjust", "ai", "export"]
      : [];
    document.querySelectorAll("#tool-nav .step").forEach((b) => {
      b.disabled = !state.imported;
      b.classList.toggle("active", state.imported && b.dataset.step === target);
      b.classList.toggle("done", state.stepsDone.has(b.dataset.step));
    });
    // un solo pannello alla volta: la tappa scelta mostra il suo contenuto
    document.querySelectorAll("#tools .tool").forEach((el) => {
      const name = el.dataset.tab;
      const isOpen = state.imported && name === target;
      el.classList.toggle("hidden", !isOpen);
      const body = el.querySelector(".tool-body");
      if (body) body.classList.toggle("hidden", !isOpen);
    });
  }

  const bindModeRadios = () => {
    document.querySelectorAll('input[name=editmode]').forEach((r) =>
      r.addEventListener("change", () => setMode(r.value))
    );
  };
  bindModeRadios();

  /* importazione */
  const doImport = async () => {
    const out = await cmd("open_dialog");
    if (out) {
      state.quads = [];
      state.sel = null;
      fitToView();
      setMode("none");
      focusTab(state.lastPanel, { open: !!state.lastPanel });
      // la spunta di Ritaglio arriva solo dopo un ritaglio applicato
    }
  };
  $("btn-import").addEventListener("click", doImport);

  /* clic al centro (senza foto) = finestra di scelta della fotografia */
  $("empty-state").addEventListener("click", () => {
    if (!state.imported) doImport();
  });

  /* drag & drop da sistema: evidenzia l'area mentre trascini */
  const areaFoto = $("viewport");
  const dragOn = () => areaFoto.classList.add("drag-over");
  const dragOff = () => areaFoto.classList.remove("drag-over");
  listen("tauri://drag-enter", dragOn);
  listen("tauri://drag-over", dragOn);
  listen("tauri://drag-leave", dragOff);
  listen("tauri://drag-drop", async (event) => {
    dragOff();
    const paths = (event.payload && event.payload.paths) || [];
    if (!paths.length) return;
    log(`trascinato: ${paths.length} file`);
    const out = await cmd("load_paths", { paths });
    if (out) {
      state.quads = [];
      state.sel = null;
      fitToView();
      setMode("none");
      focusTab(state.lastPanel, { open: !!state.lastPanel });
    }
  });

  /* rilevamento automatico: trova le foto dentro una scansione */
  $("btn-detect").addEventListener("click", async () => {
    const quads = await cmd("detect_photos");
    state.quads = quads || [];
    state.sel = null;
    const box = $("detect-result");
    box.classList.remove("hidden");
    $("btn-skip-crop").classList.add("hidden");
    if (!state.quads.length) {
      // una foto singola che riempie il quadro non ha nulla da separare:
      // non e' un errore, si puo' proseguire direttamente
      box.textContent = t("detect-nessuna");
      $("btn-skip-crop").classList.remove("hidden");
    } else {
      // seleziona subito la foto migliore: il tasto "Applica il ritaglio"
      // appare immediatamente, niente clic aggiuntivi richiesti
      selectQuad(state.quads[0]);
    }
    drawOverlays();
  });

  $("btn-skip-crop").addEventListener("click", () => {
    $("detect-result").classList.add("hidden");
    $("btn-skip-crop").classList.add("hidden");
    focusTab("resize");
  });

  $("btn-clear-selection").addEventListener("click", () => {
    state.sel = null;
    $("detect-result").classList.add("hidden");
    $("manual-actions").classList.add("hidden");
    drawOverlays();
  });

  /* ritaglio / prospettiva */
  $("btn-apply-crop").addEventListener("click", async () => {
    if (!state.sel) {
      showMessage(t("seleziona-ritaglio"));
      return;
    }
    let box;
    if (state.sel.type === "rect") {
      box = { x: state.sel.x, y: state.sel.y, w: state.sel.w, h: state.sel.h };
    } else if (state.sel.type === "detail" && state.sel.rect) {
      // area rilevata in automatico: si ritaglia il suo riquadro
      box = state.sel.rect;
    } else {
      showMessage(t("seleziona-ritaglio"));
      return;
    }
    const { x, y, w, h } = box;
    const out = await cmd("apply_crop", { x, y, width: w, height: h });
    if (out) {
      state.quads = [];
      state.sel = null;
      $("detect-result").classList.add("hidden");
      $("manual-actions").classList.add("hidden");
      setMode("none");
      fitToView();
      state.currentTab = "resize";
      focusTab("resize");
      markStepDone("detect");
    }
  });

  $("btn-apply-persp").addEventListener("click", async () => {
    let corners = null;
    if (state.sel && state.sel.type === "poly" && state.sel.pts.length === 4) {
      corners = state.sel.pts.map((p) => [p[0] / state.imgW, p[1] / state.imgH]);
    } else if (state.sel && state.sel.type === "detail") {
      corners = state.sel.corners;
    }
    if (!corners) {
      showMessage(t("seleziona-angoli"));
      return;
    }
    const out = await cmd("apply_perspective", { corners });
    if (out) {
      state.quads = [];
      state.sel = null;
      $("detect-result").classList.add("hidden");
      $("manual-actions").classList.add("hidden");
      setMode("none");
      fitToView();
      focusTab("resize");
      markStepDone("detect");
    }
  });

  /* modifica delle dimensioni (proporzioni sempre mantenute) */
  const updateResizePreview = () => {
    if (!state.imported) return { tw: 0, th: 0 };
    const w = Number($("resize-w").value) || 0;
    const h = Number($("resize-h").value) || 0;
    const wh = state.imgW > 0 ? state.imgH / state.imgW : 1;
    let tw, th;
    if (w > 0 && h > 0) {
      // entrambi modificati: assume la larghezza e ricalcola l'altezza
      tw = w;
      th = Math.max(1, Math.round(tw * wh));
      $("resize-h").value = th;
    } else if (w > 0) {
      tw = w;
      th = Math.max(1, Math.round(tw * wh));
      $("resize-h").value = th;
    } else if (h > 0) {
      th = h;
      tw = Math.max(1, Math.round(th * (state.imgW / state.imgH)));
      $("resize-w").value = tw;
    } else {
      tw = 0;
      th = 0;
    }
    const sh = Number($("resize-sharpen").value);
    $("sharpen-label").textContent = sh <= 0 ? t("disattiva") : `+${sh.toFixed(2)}`;
    const box = $("resize-preview");
    if (tw > 0 && th > 0) {
      box.classList.remove("hidden");
      box.textContent = tf("resize-risultato", {
        tw,
        th,
        p: (((tw * th) / (state.imgW * state.imgH)) * 100).toFixed(0),
      });
    } else {
      box.classList.add("hidden");
    }
    return { tw, th };
  };
  ["resize-w", "resize-h", "resize-sharpen"].forEach((id) =>
    $(id).addEventListener("input", () => {
      // modifica manuale dei campi: deseleziona il preset evidenziato
      fastSel = null;
      aggiornaFast();
      updateResizePreview();
    })
  );

  $("btn-apply-resize").addEventListener("click", async () => {
    const dims = updateResizePreview();
    const sharpen = Number($("resize-sharpen").value);
    if (!dims.tw || !dims.th) {
      showMessage(t("resize-auto"));
      return;
    }
    if (dims.tw === state.imgW && dims.th === state.imgH) {
      showMessage(t("resize-gia"), true);
      return;
    }
    const options = {
      method: $("resize-method").value,
      factor: null,
      keep_aspect: true,
      target_width: dims.tw,
      target_height: dims.th,
      sharpen: sharpen > 0.01 ? sharpen : null,
    };
    const out = await cmd("apply_resize", { options });
    if (out) {
      fitToView();
      state.currentTab = "adjust";
      markStepDone("resize");
    }
  });

  /* preset di ingrandimento 2x/4x/8x: selezione esclusiva (cornice), sempre
     relativa all'ORIGINALE. Il clic compila solo i campi; la risoluzione
     cambia davvero solo premendo "Applica le nuove dimensioni" */
  let fastSel = null;
  const aggiornaFast = () => {
    document.querySelectorAll("[data-fast]").forEach((b) => {
      const f = Number(b.dataset.fast);
      const tw = state.origW ? Math.round(state.origW * f) : 0;
      const th = state.origH ? Math.round(state.origH * f) : 0;
      b.disabled = !state.imported;
      b.classList.toggle("fast-attiva", state.imported && fastSel === f);
      b.title = tw
        ? tf("fast-title", { tw, th, ow: state.origW, oh: state.origH })
        : "";
    });
  };
  document.querySelectorAll("[data-fast]").forEach((b) => {
    b.addEventListener("click", () => {
      if (!state.imported) {
        showMessage(t("carica-prima"));
        return;
      }
      if (!state.origW) return;
      fastSel = Number(b.dataset.fast);
      // target calcolato sull'originale, non sull'attuale
      $("resize-w").value = Math.round(state.origW * fastSel);
      $("resize-h").value = Math.round(state.origH * fastSel);
      aggiornaFast();
      updateResizePreview();
    });
  });

  /* correzioni */
  const adjMap = { "adj-brightness": "lbl-brightness", "adj-contrast": "lbl-contrast", "adj-saturation": "lbl-saturation" };
  Object.entries(adjMap).forEach(([input, label]) => {
    $(input).addEventListener("input", () => { $(label).textContent = $(input).value; });
  });
  $("btn-clear-adjust").addEventListener("click", () => {
    ["adj-brightness", "adj-contrast", "adj-saturation"].forEach((id) => { $(id).value = 0; });
    Object.entries(adjMap).forEach(([i, l]) => ($(l).textContent = "0"));
    $("adj-noise").value = 0;
    $("adj-wb").checked = false;
    $("adj-bw").checked = false;
  });

  $("btn-apply-adjust").addEventListener("click", async () => {
    const params = {
      brightness: Number($("adj-brightness").value),
      contrast: Number($("adj-contrast").value),
      saturation: Number($("adj-saturation").value),
      noise_reduction: Number($("adj-noise").value),
      auto_white_balance: $("adj-wb").checked,
      grayscale: $("adj-bw").checked,
    };
    const allZero = params.brightness === 0 && params.contrast === 0 && params.saturation === 0 &&
      params.noise_reduction === 0 && !params.auto_white_balance && !params.grayscale;
    if (allZero) {
      showMessage(t("nessuna-correzione"));
      return;
    }
    const out = await cmd("apply_adjust", { params });
    if (out) {
      state.currentTab = "export";
      markStepDone("adjust");
    }
  });

  /* recupero dettagli JPEG */
  $("btn-jpeg-restore").addEventListener("click", async () => {
    if (!state.imported) {
      showMessage(t("carica-prima"));
      return;
    }
    const strength = Number($("jpeg-strength").value) || 0.4;
    const out = await cmd("apply_jpeg_restore", { strength });
    if (out) {
      state.currentTab = "export";
      markStepDone("adjust");
    }
  });

  /* fragola di stato in basso a destra: fase, barra, velocita' e tempo restante */
  const statusRoot = $("ai-status");
  const statusTitle = $("ai-status-title");
  const statusBar = $("ai-status-bar");
  const statusInfo = $("ai-status-info");
  const progSamples = [];
  const aiStatusShow = (title) => {
    statusRoot.classList.remove("hidden");
    statusTitle.textContent = title;
    statusBar.style.width = "0%";
    statusInfo.textContent = "";
    progSamples.length = 0;
  };
  const aiStatusSet = (pct, info) => {
    if (pct !== null && isFinite(pct)) {
      statusBar.style.width = `${Math.max(0, Math.min(100, pct))}%`;
    }
    if (info !== null) statusInfo.textContent = info;
  };
  const aiStatusHide = () => {
    statusRoot.classList.add("hidden");
  };

  /* rimozione sfondo: AI un clic + automatica base + tolleranza */
  let bgModels = [];
  const loadBgModels = async () => {
    const all = (await invoke("ai_models").catch(() => [])) || [];
    bgModels = all.filter((m) => m.kind === "sfondo");
    const sel = $("bg-model");
    const prev = sel.value;
    sel.textContent = "";
    bgModels.forEach((m) => {
      const o = document.createElement("option");
      o.value = m.id;
      o.textContent = m.id === "sfondo-u2netp" ? t("bg-leggera") : t("bg-precisa");
      sel.appendChild(o);
    });
    if (prev) sel.value = prev;
    updateBgInfo();
  };
  const updateBgInfo = () => {
    const m = bgModels.find((k) => k.id === $("bg-model").value);
    const box = $("bg-licenza");
    if (!m) return;
    box.classList.remove("hidden");
    const stato = m.installed ? t("stato-ai-pronto") : t("stato-ai-mancante");
    box.textContent = `${stato}. ${m.notice}`;
  };
  $("bg-model").addEventListener("change", updateBgInfo);
  const startBgAi = async (m) => {
    aiBusy = true;
    aiStatusShow(t("sfondo-stato"));
    aiStatusSet(15, "elaborazione in corso…");
    await cmd("sfondo_ai", { modelId: m.id });
  };
  $("btn-bg-ai").addEventListener("click", async () => {
    if (!state.imported) {
      showMessage(t("carica-prima"));
      return;
    }
    const m = bgModels.find((k) => k.id === $("bg-model").value);
    if (!m) return;
    if (aiBusy) {
      showMessage(t("gia-in-corso"));
      return;
    }
    await startBgAi(m);
  });
  $("btn-bg-apply").addEventListener("click", async () => {
    if (!state.imported) {
      showMessage(t("carica-prima"));
      return;
    }
    const tol = Number($("bg-tolerance").value) || 10;
    await cmd("auto_remove_background", { tolerance: tol });
    markStepDone("bg");
  });
  $("bg-tolerance").addEventListener("input", () => {
    $("lbl-bg-tol").textContent = $("bg-tolerance").value;
  });

  /* ripristino dei volti: scelta tra CodeFormer/GFPGAN e detector */
  let voltoModels = [];
  let voltoDets = [];
  const loadVoltoModels = async () => {
    const all = (await invoke("ai_models").catch(() => [])) || [];
    voltoModels = all.filter((m) => m.kind === "volto" && m.id.startsWith("volto-") && !m.id.includes("det-"));
    voltoDets = all.filter((m) => m.kind === "volto" && m.id.includes("det-"));
    updateVoltoInfo();
  };
  const updateVoltoInfo = () => {
    const box = $("volto-licenza");
    const m = voltoModels.find((x) => x.id === $("volto-model").value);
    const d = voltoDets.find((x) => x.id === $("volto-detector").value);
    if (!m) return;
    box.classList.remove("hidden");
    const nome = m.installed
      ? `${$("volto-model").selectedOptions[0].text.split(" (")[0]} — ${t("stato-ai-pronto")}`
      : `${$("volto-model").selectedOptions[0].text.split(" (")[0]} — ${t("stato-ai-mancante")}`;
    const detOk = d && d.installed;
    const detNomi = `${t("volto-rilevatore")}: ${$("volto-detector").selectedOptions[0].text.split(" (")[0]}${detOk ? "" : ` — ${t("stato-ai-mancante")}`}`;
    box.textContent = `${nome} · ${detNomi}. ${m.notice}${d ? " " + d.notice : ""}`;
  };
  $("volto-model").addEventListener("change", updateVoltoInfo);
  $("volto-detector").addEventListener("change", updateVoltoInfo);
  $("btn-volto-ai").addEventListener("click", async () => {
    if (!state.imported) {
      showMessage(t("carica-prima"));
      return;
    }
    const m = voltoModels.find((x) => x.id === $("volto-model").value);
    if (!m) return;
    if (aiBusy) {
      showMessage(t("gia-in-corso"));
      return;
    }
    aiBusy = true;
    aiStatusShow(t("volto-stato"));
    aiStatusSet(5, t("volto-lungo"));
    await cmd("volto_ai", { modelId: m.id, detector: $("volto-detector").value });
  });

  /* export */
  const updateExportUi = () => {
    $("quality-row").classList.toggle("hidden", $("export-format").value !== "jpeg");
  };
  $("export-format").addEventListener("change", updateExportUi);
  updateExportUi();
  $("export-quality").addEventListener("input", () => ($("lbl-quality").textContent = $("export-quality").value));

  $("btn-export").addEventListener("click", async () => {
    const out = await cmd("export_image", {
      options: {
        format: $("export-format").value,
        quality: Number($("export-quality").value),
      },
    });
    if (out) {
      $("export-info").textContent = tf("salvato-in", { p: out });
      log(`esportazione riuscita -> ${out}`, "ok");
      markStepDone("export");
    }
  });

  /* confronto: modalita' e divisore */
  document.querySelectorAll('input[name=compare]').forEach((r) =>
    r.addEventListener("change", () => setCompare(r.value))
  );
  function setCompare(mode) {
    const c = $("compare");
    c.classList.remove("mode-dopo", "mode-prima", "mode-divisore", "mode-affiancato");
    c.classList.add(`mode-${mode}`);
    if (mode === "divisore") {
      $("after-clip").style.width = "";
      updateDivider();
    } else if (mode === "affiancato") {
      $("after-clip").style.clipPath = "none";
      $("after-clip").style.width = "50%";
    } else {
      $("after-clip").style.width = "";
      updateDivider();
    }
  }
  setCompare("dopo");
  $("divider").addEventListener("pointerdown", (e) => {
    e.preventDefault();
    $("divider").setPointerCapture(e.pointerId);
    state.dividerDrag = true;
  });
  const dividerDragEnd = () => (state.dividerDrag = false);
  $("divider").addEventListener("pointermove", (e) => {
    if (!state.dividerDrag) return;
    setDividerFromClientX(e.clientX);
  });
  $("divider").addEventListener("pointerup", dividerDragEnd);
  $("divider").addEventListener("pointercancel", dividerDragEnd);

  /* zoom */
  $("viewport").addEventListener("wheel", (ev) => {
    ev.preventDefault();
    if (!state.imported) return;
    const rect = $("viewport").getBoundingClientRect();
    const dir = ev.deltaY < 0 ? 1.12 : 1 / 1.12;
    setZoom(state.zoom * dir, ev.clientX - rect.left, ev.clientY - rect.top);
  }, { passive: false });

  $("btn-zoom-in").addEventListener("click", () => setZoom(state.zoom * 1.2));
  $("btn-zoom-out").addEventListener("click", () => setZoom(state.zoom / 1.2));
  $("btn-zoom-fit").addEventListener("click", fitToView);
  $("zoom-range").addEventListener("input", () => setZoom(Number($("zoom-range").value) / 100));

  /* pan con trascinamento (solo in modalita' none) */
  $("viewport").addEventListener("pointerdown", (ev) => {
    if (ev.button !== 0 || state.editmode !== "none") return;
    if (ev.target && ev.target.closest && ev.target.closest("#overlays")) return;
    if (ev.target && ev.target.closest && ev.target.closest("#divider")) return;
    state.panDrag = { x: ev.clientX - state.panX, y: ev.clientY - state.panY };
    $("viewport").style.cursor = "grabbing";
  });
  $("viewport").addEventListener("pointermove", (ev) => {
    if (state.panDrag && state.editmode === "none") {
      state.panX = ev.clientX - state.panDrag.x;
      state.panY = ev.clientY - state.panDrag.y;
      applyStage();
    }
  });
  $("viewport").addEventListener("pointerup", () => {
    state.panDrag = null;
    if (state.editmode === "none") $("viewport").style.cursor = "default";
  });

  /* undo / redo / reset: le spunte dei passi seguono la cronologia */
  $("btn-undo").addEventListener("click", () =>
    cmd("undo").then((o) => {
      if (o) {
        fitToView();
        unmarkLastStep();
      }
    }));
  $("btn-redo").addEventListener("click", () =>
    cmd("redo").then((o) => {
      if (o) {
        fitToView();
        const s = stepsUndone.pop();
        if (s) {
          state.stepsDone.add(s);
          stepsOrder.push(s);
          UpdateStepsUI();
        }
      }
    }));
  $("btn-reset").addEventListener("click", () =>
    cmd("reset_to_original").then((o) => {
      if (o) {
        fitToView();
        state.stepsDone.clear();
        stepsOrder.length = 0;
        stepsUndone = [];
        UpdateStepsUI();
      }
    }));

  /* ingrandimento AI */
  const aiLic = $("ai-licenza");
  let aiModels = [];
  let aiBusy = false;
  const loadAi = async () => {
    const all = (await invoke("ai_models").catch((e) => {
      log(`pannello AI non disponibile: ${e}`, "err");
      return [];
    })) || [];
    aiModels = all.filter((m) => m.kind === "upscale");
    const sel = $("ai-model");
    const prev = sel.value;
    sel.textContent = "";
    aiModels.forEach((m) => {
      const o = document.createElement("option");
      o.value = m.id;
      o.textContent = m.id.includes("x2")
        ? t("ai-rapido")
        : m.id.includes("ultrasharp")
          ? t("ai-ultrasharp")
          : t("ai-x4");
      sel.appendChild(o);
    });
    if (prev) sel.value = prev;
    updateAiInfo();
  };
  const aiScaleValue = () =>
    Number(document.querySelector('input[name=ai-scale]:checked')?.value) || 4;
  const updateAiScaleWarn = () => {
    const m = aiModels.find((k) => k.id === $("ai-model").value);
    if (!m) return;
    const warn = $("ai-scale-warn");
    const chosen = aiScaleValue();
    if (m.scale > chosen) {
      warn.textContent = tf("scala-warn-giu", { n: m.scale, c: chosen });
      warn.classList.remove("hidden");
    } else if (m.scale < chosen) {
      warn.textContent = tf("scala-warn-su", { n: m.scale, c: chosen });
      warn.classList.remove("hidden");
    } else {
      warn.classList.add("hidden");
    }
  };
  const updateAiInfo = () => {
    const sel = $("ai-model");
    const m = aiModels.find((k) => k.id === sel.value);
    if (!m) return;
    // seleziona di default la scala nativa del modello
    const radio = document.querySelector(`input[name=ai-scale][value="${m.scale}"]`);
    if (radio) radio.checked = true;
    const stato = m.installed ? t("stato-ai-pronto") : t("stato-ai-mancante");
    aiLic.classList.remove("hidden");
    aiLic.textContent = `${stato}. ${m.notice}`;
    updateAiScaleWarn();
    log(`AI: ${m.name} — ${stato}`);
  };
  $("ai-model").addEventListener("change", updateAiInfo);
  document.querySelectorAll('input[name=ai-scale]').forEach((r) =>
    r.addEventListener("change", updateAiScaleWarn)
  );

  $("btn-ai-apply").addEventListener("click", async () => {
    if (aiBusy) {
      showMessage(t("gia-in-corso"));
      return;
    }
    const m = aiModels.find((k) => k.id === $("ai-model").value);
    if (!state.imported) {
      showMessage(t("carica-prima"));
      return;
    }
    if (!m) return;
    await startUpscale(m);
  });
  const startUpscale = async (m) => {
    aiBusy = true;
    aiStatusShow(t("ai-stato"));
    aiStatusSet(2, t("ai-calcolo"));
    await cmd("ai_upscale", { modelId: m.id, scala: aiScaleValue() });
  };

  listen("ai-progress", (ev) => {
    const p = ev.payload || {};
    const done = Number(p.done) || 0;
    const total = Number(p.total) || 0;
    statusTitle.textContent = p.message || t("elaborazione");
    if (total <= 0) {
      aiStatusSet(null, p.message || "");
      return;
    }
    const pct = (done / total) * 100;
    const now = Date.now();
    progSamples.push({ t: now, d: done });
    while (progSamples.length > 2 && now - progSamples[0].t > 4000) progSamples.shift();
    let extra = "";
    const a = progSamples[0];
    const b = progSamples[progSamples.length - 1];
    if (a && b && b.d > a.d) {
      const dt = (b.t - a.t) / 1000;
      if (dt > 0.2) {
        const rate = (b.d - a.d) / dt;
        const rest = (total - done) / rate;
        const restKeys = rest >= 60
          ? tf("minuti-rimasti", { m: Math.floor(rest / 60) })
          : tf("circa-rimasti", { s: Math.max(1, Math.round(rest)) });
        if (p.kind === "upscaling") {
          extra = ` · ${restKeys}`;
        } else {
          extra = ` · ${(rate / (1024 * 1024)).toFixed(1)} MB/s · ${restKeys}`;
        }
      }
    }
    const scala =
      p.kind === "upscaling" ? `${total} ${t("blocchi")}`
      : p.kind === "volto" ? `${total} ${t("volti-n")}`
      : `${(total / (1024 * 1024)).toFixed(1)} MB`;
    aiStatusSet(pct, `${Math.floor(pct)}% di ${scala}${extra}`);
  });
  listen("ai-done", async (ev) => {
    const d = ev.payload || {};
    const wasError = !!d.error;
    aiBusy = false;
    aiStatusHide();
    if (wasError) {
      showMessage(d.error);
      log(`AI errore: ${d.error}`, "err");
    }
    await loadAi();
    loadBgModels().catch(() => null);
    loadVoltoModels().catch(() => null);
    if (!wasError) {
      log(`AI: ${d.message}`, "ok");
      // spunta solo se una modifica e' stata davvero applicata
      const msg = String(d.message || "");
      const passo = msg.includes("completato")
        ? "ai"
        : msg === "volti ripristinati"
          ? "volto"
          : msg === "sfondo rimosso"
            ? "bg"
            : null;
      if (passo) markStepDone(passo);
      const s = await cmd("get_session_state");
      if (s) {
        updateState(s);
        fitToView();
      }
    }
  });
  loadAi().catch(() => null);
  loadBgModels().catch(() => null);
  loadVoltoModels().catch(() => null);

  /* barra di visibilita' della correzione (in tempo reale) */
  function aggiornaFusioneUI(s) {
    ["volto", "ai"].forEach((k) => {
      const row = $(`fusione-${k}`);
      const barra = $(`fusione-${k}-barra`);
      const val = $(`fusione-${k}-val`);
      const attiva = !!s.fusione_kind && s.fusione_kind === k;
      row.classList.toggle("hidden", !(attiva && state.imported));
      if (attiva) {
        if (!state.fusioneAttiva) barra.value = Math.round((s.fusione ?? 1) * 100);
        val.textContent = `${barra.value}%`;
      }
    });
  }
  async function impostaFusione(k) {
    const barra = $(`fusione-${k}-barra`);
    try {
      const s = await invoke("imposta_fusione", { percento: Number(barra.value) });
      if (s) updateState(s);
    } catch (err) {
      log(`fusione ERRORE: ${err}`, "err");
    }
  }
  ["volto", "ai"].forEach((k) => {
    const barra = $(`fusione-${k}-barra`);
    barra.addEventListener("input", () => {
      state.fusioneAttiva = true;
      $(`fusione-${k}-val`).textContent = `${barra.value}%`;
      if (state.fusioneTimer) clearTimeout(state.fusioneTimer);
      state.fusioneTimer = setTimeout(() => impostaFusione(k), 140);
    });
    barra.addEventListener("change", () => {
      state.fusioneAttiva = false;
      if (state.fusioneTimer) {
        clearTimeout(state.fusioneTimer);
        state.fusioneTimer = null;
      }
      impostaFusione(k);
    });
  });

  /* log */
  $("btn-log").addEventListener("click", () => {
    $("log-panel").classList.remove("hidden");
    dumpDiagnostico();
  });
  /* donazione: apre PayPal nel browser predefinito */
  $("btn-dona").addEventListener("click", async () => {
    try {
      await cmd("dona");
      log("Dona: pagina PayPal aperta nel browser. Grazie!", "ok");
    } catch (e) {
      log(`dona fallito: ${e}`, "err");
    }
  });
  function dumpDiagnostico() {
    if (!state.imported) return;
    const cs = (el) => getComputedStyle(el);
    const comp = $("compare");
    const clip = $("after-clip");
    const b = $("img-before");
    const a = $("img-after");
    log(
      `DIAG classe=${comp.className} compDisp=${cs(comp).display} ` +
      `clipDisp=${cs(clip).display} clipPos=${cs(clip).position} clipW=${cs(clip).width} ` +
      `clipInline=${clip.style.cssText.replace(/; /g, ";")} ` +
      `beforeDisp=${cs(b).display} beforeW=${cs(b).width} beforeNat=${b.naturalWidth} ` +
      `afterDisp=${cs(a).display} afterW=${cs(a).width} afterNat=${a.naturalWidth}`
    );
  }
  $("btn-log-close").addEventListener("click", () => $("log-panel").classList.add("hidden"));

  /* impostazioni: scelta della lingua (con bandierine) */
  $("btn-settings").addEventListener("click", () => {
    $("settings-panel").classList.remove("hidden");
  });
  $("btn-settings-close").addEventListener("click", () =>
    $("settings-panel").classList.add("hidden")
  );
  const riapplicaLingua = async () => {
    window.I18N.applica();
    loadAi().catch(() => null);
    loadBgModels().catch(() => null);
    loadVoltoModels().catch(() => null);
    try {
      const s = await invoke("get_session_state");
      if (s) updateState(s);
    } catch (e) { /* nessuna foto: niente da aggiornare */ }
  };
  document.querySelectorAll("#lang-list button").forEach((b) => {
    b.addEventListener("click", () => {
      window.I18N.imposta(b.dataset.lang);
      riapplicaLingua();
      log(`Lingua: ${b.dataset.lang}`, "ok");
    });
  });

  window.I18N.applica();
  log(t("avviato"), "ok");

  /* avvio */
  invoke("get_session_state").then((s) => s && updateState(s)).catch(() => null);
})();
