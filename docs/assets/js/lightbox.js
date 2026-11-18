/* Asraab screenshot viewer.
 *
 * The picture flies out of the frame that was clicked, the room behind it
 * steps back, and every shot of the same gallery waits in a strip along the
 * bottom. Built on the platform (WAAPI) on purpose: no new dependency, and
 * reduced motion falls back to a plain fade.
 */
(function () {
  "use strict";

  const doc = document;
  const root = doc.documentElement;
  const links = Array.from(doc.querySelectorAll(".shot-open"));
  if (!links.length) return;

  const reduceQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
  const finePointer = window.matchMedia("(hover: hover) and (pointer: fine)").matches;

  const ICON = (paths) =>
    '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">' +
    paths +
    "</svg>";

  let box = null;
  let scrim = null;
  let stage = null;
  let frame = null;
  let img = null;
  let btnClose = null;
  let btnPrev = null;
  let btnNext = null;
  let bar = null;
  let caption = null;
  let count = null;
  let strip = null;
  let thumbs = [];
  let hint = null;
  let live = null;
  let inerted = [];
  let reflection = null;
  let progress = null;
  let brandName = null;
  let brandKind = null;
  let zoomBadge = null;

  let group = [];
  let index = 0;
  let current = null;
  let lastFocus = null;
  let flight = null;
  let token = 0;
  let tiltRaf = 0;
  let zoom = 1;
  let panX = 0;
  let panY = 0;
  let drag = null;
  const touches = new Map();
  let pinch = null;

  const reduced = () => reduceQuery.matches;
  const rtl = () => root.dir === "rtl";
  const label = (en, ar) => (root.lang === "ar" ? ar : en);

  function accent() {
    const page = root.dataset.page || "home";
    const name = page === "droidcrate" ? "--jade" : "--gold";
    const value = getComputedStyle(root).getPropertyValue(name).trim();
    return value || "#f2b46d";
  }

  /* ---------- Build the viewer once ---------- */
  function ensure() {
    if (box) return;

    box = doc.createElement("div");
    box.className = "lightbox";
    box.setAttribute("role", "dialog");
    box.setAttribute("aria-modal", "true");
    box.innerHTML =
      '<div class="lightbox-scrim" aria-hidden="true"></div>' +
      '<div class="lightbox-stage">' +
      '<span class="lightbox-floor" aria-hidden="true"></span>' +
      '<figure class="lightbox-frame">' +
      '<div class="lightbox-crop"><img alt="" /></div>' +
      '<span class="lightbox-glare" aria-hidden="true"></span>' +
      '<span class="lightbox-reflection" aria-hidden="true"><img alt="" /></span>' +
      "</figure>" +
      "</div>" +
      '<span class="lightbox-brand" aria-hidden="true"><b></b><em></em></span>' +
      '<span class="lightbox-zoom" aria-hidden="true"></span>' +
      '<button class="lightbox-close" type="button">' +
      ICON('<path d="M6 6l12 12M18 6 6 18" stroke-linecap="round"/>') +
      "</button>" +
      '<button class="lightbox-nav lightbox-prev" type="button">' +
      ICON('<path d="M15 5l-7 7 7 7" stroke-linecap="round" stroke-linejoin="round"/>') +
      "</button>" +
      '<button class="lightbox-nav lightbox-next" type="button">' +
      ICON('<path d="M9 5l7 7-7 7" stroke-linecap="round" stroke-linejoin="round"/>') +
      "</button>" +
      '<div class="lightbox-dock">' +
      '<div class="lightbox-bar"><span class="lightbox-count"></span>' +
      '<span class="lightbox-caption"></span>' +
      '<span class="lightbox-hint"></span></div>' +
      '<span class="lightbox-progress" aria-hidden="true"><i></i></span>' +
      '<div class="lightbox-strip"></div>' +
      "</div>" +
      '<span class="lightbox-live" aria-live="polite" aria-atomic="true"></span>';
    doc.body.appendChild(box);

    scrim = box.querySelector(".lightbox-scrim");
    stage = box.querySelector(".lightbox-stage");
    frame = box.querySelector(".lightbox-frame");
    img = box.querySelector(".lightbox-crop img");
    reflection = box.querySelector(".lightbox-reflection img");
    progress = box.querySelector(".lightbox-progress");
    brandName = box.querySelector(".lightbox-brand b");
    brandKind = box.querySelector(".lightbox-brand em");
    zoomBadge = box.querySelector(".lightbox-zoom");
    btnClose = box.querySelector(".lightbox-close");
    btnPrev = box.querySelector(".lightbox-prev");
    btnNext = box.querySelector(".lightbox-next");
    bar = box.querySelector(".lightbox-bar");
    caption = box.querySelector(".lightbox-caption");
    count = box.querySelector(".lightbox-count");
    strip = box.querySelector(".lightbox-strip");
    hint = box.querySelector(".lightbox-hint");
    live = box.querySelector(".lightbox-live");

    btnClose.addEventListener("click", close);
    btnPrev.addEventListener("click", () => step(rtl() ? 1 : -1));
    btnNext.addEventListener("click", () => step(rtl() ? -1 : 1));

    box.addEventListener("click", (event) => {
      if (event.target === box || event.target === scrim || event.target === stage) close();
    });

    doc.addEventListener("keydown", onKey);
    bindSwipe();
    bindTilt();
    bindZoom();
  }

  /* ---------- Zoom: wheel, double-click, drag ---------- */
  // A screenshot is worth reading, not just seeing. The wheel zooms toward the
  // pointer, a double-click goes back to fit, and a zoomed picture can be
  // dragged. The image is clipped to its frame only while zoomed, so the
  // opening flight is never cut off.
  function paintHint() {
    if (!hint) return;
    hint.hidden = false;
    const coarse = !finePointer;
    if (zoom > 1.001) {
      hint.textContent = coarse
        ? label("Double-tap to fit", "نقرتان للاحتواء")
        : label("Double-click to fit", "نقر مزدوج للاحتواء");
      return;
    }
    hint.textContent = coarse
      ? label("Pinch to zoom", "بإصبعين للتكبير")
      : label("Scroll to zoom", "عجلة الماوس للتكبير");
  }

  function clampPan() {
    const maxX = Math.max(0, ((zoom - 1) * (img.offsetWidth || 0)) / 2);
    const maxY = Math.max(0, ((zoom - 1) * (img.offsetHeight || 0)) / 2);
    panX = Math.min(maxX, Math.max(-maxX, panX));
    panY = Math.min(maxY, Math.max(-maxY, panY));
  }

  function applyZoom() {
    img.style.setProperty("--lb-zoom", zoom.toFixed(3));
    img.style.setProperty("--lb-tx", panX.toFixed(1) + "px");
    img.style.setProperty("--lb-ty", panY.toFixed(1) + "px");
    if (zoomBadge) zoomBadge.textContent = zoom.toFixed(1) + "×";
    box.classList.toggle("is-zoomed", zoom > 1.001);
    paintHint();
  }

  function resetZoom() {
    zoom = 1;
    panX = 0;
    panY = 0;
    drag = null;
    if (zoomBadge) zoomBadge.textContent = "1.0×";
    if (!img) return;
    img.style.removeProperty("--lb-zoom");
    img.style.removeProperty("--lb-tx");
    img.style.removeProperty("--lb-ty");
    if (box) box.classList.remove("is-zoomed", "is-dragging");
    paintHint();
  }

  /* One owner for "the viewer has no picture right now": the big frame and
     the reflection under it always change together. */
  function clearImage() {
    img.removeAttribute("src");
    if (reflection) reflection.removeAttribute("src");
  }

  function setZoom(next, cx, cy) {
    const to = Math.min(4, Math.max(1, next));
    if (Math.abs(to - zoom) < 0.0005) return;

    const rect = frame.getBoundingClientRect();
    const midX = rect.left + rect.width / 2;
    const midY = rect.top + rect.height / 2;
    const dx = (cx == null ? midX : cx) - midX;
    const dy = (cy == null ? midY : cy) - midY;

    // Keep the point under the pointer where it is while the scale changes.
    const ratio = to / zoom;
    panX = panX + dx * (1 - ratio);
    panY = panY + dy * (1 - ratio);
    zoom = to;
    if (zoom === 1) {
      panX = 0;
      panY = 0;
    }

    // Tilt and zoom are two different gestures: the picture squares up.
    frame.style.removeProperty("--lb-rx");
    frame.style.removeProperty("--lb-ry");
    frame.style.removeProperty("--lb-mx");
    frame.style.removeProperty("--lb-my");
    img.style.removeProperty("--lb-px");
    img.style.removeProperty("--lb-py");
    stage.style.removeProperty("--lb-floor-x");

    clampPan();
    applyZoom();
  }

  function bindZoom() {
    frame.addEventListener(
      "wheel",
      (event) => {
        if (!box.classList.contains("is-open")) return;
        event.preventDefault();
        setZoom(zoom * Math.exp(-event.deltaY * 0.0016), event.clientX, event.clientY);
      },
      { passive: false }
    );

    frame.addEventListener("dblclick", (event) => {
      if (!box.classList.contains("is-open")) return;
      event.preventDefault();
      setZoom(zoom > 1.01 ? 1 : 2.2, event.clientX, event.clientY);
    });

    img.addEventListener("pointerdown", (event) => {
      if (zoom <= 1.001 || event.button !== 0) return;
      drag = { x: event.clientX, y: event.clientY, panX: panX, panY: panY };
      box.classList.add("is-dragging");
      if (img.setPointerCapture) img.setPointerCapture(event.pointerId);
    });

    img.addEventListener("pointermove", (event) => {
      if (!drag) return;
      panX = drag.panX + (event.clientX - drag.x);
      panY = drag.panY + (event.clientY - drag.y);
      clampPan();
      applyZoom();
    });

    const end = (event) => {
      if (!drag) return;
      if (
        img.releasePointerCapture &&
        img.hasPointerCapture &&
        img.hasPointerCapture(event.pointerId)
      ) {
        img.releasePointerCapture(event.pointerId);
      }
      drag = null;
      box.classList.remove("is-dragging");
    };
    img.addEventListener("pointerup", end);
    img.addEventListener("pointercancel", end);

    // Touch: two fingers scale around their midpoint. The page behind is
    // locked, and the frame itself carries touch-action: none, so the browser
    // never steals the gesture.
    const distance = () => {
      const points = Array.from(touches.values());
      return Math.hypot(points[0].x - points[1].x, points[0].y - points[1].y);
    };
    const midpoint = () => {
      const points = Array.from(touches.values());
      return {
        x: (points[0].x + points[1].x) / 2,
        y: (points[0].y + points[1].y) / 2,
      };
    };

    frame.addEventListener(
      "pointerdown",
      (event) => {
        if (event.pointerType !== "touch" || !box.classList.contains("is-open")) return;
        touches.set(event.pointerId, { x: event.clientX, y: event.clientY });
        if (touches.size === 2) {
          drag = null;
          box.classList.remove("is-dragging");
          pinch = { dist: distance(), zoom: zoom };
        }
      },
      { passive: true }
    );

    frame.addEventListener(
      "pointermove",
      (event) => {
        if (!touches.has(event.pointerId)) return;
        touches.set(event.pointerId, { x: event.clientX, y: event.clientY });
        if (!pinch || touches.size !== 2) return;
        event.preventDefault();
        if (pinch.dist < 8) return;
        const mid = midpoint();
        setZoom(pinch.zoom * (distance() / pinch.dist), mid.x, mid.y);
      },
      { passive: false }
    );

    const lift = (event) => {
      touches.delete(event.pointerId);
      if (touches.size < 2) pinch = null;
    };
    frame.addEventListener("pointerup", lift);
    frame.addEventListener("pointercancel", lift);
  }

  /* ---------- Caption: one word at a time, like it is being written ---------- */
  function paintCaption(text) {
    caption.textContent = "";
    const words = text.split(/\s+/).filter(Boolean);
    words.forEach((word) => {
      const span = doc.createElement("span");
      span.className = "lb-word";
      span.textContent = word;
      caption.appendChild(span);
      caption.appendChild(doc.createTextNode(" "));
    });
    if (reduced() || !caption.animate) return;
    Array.from(caption.children).forEach((el, i) => {
      el.animate(
        [
          { opacity: 0, transform: "translateY(7px)" },
          { opacity: 1, transform: "none" },
        ],
        {
          duration: 380,
          delay: 90 + i * 18,
          easing: "cubic-bezier(0.16, 1, 0.3, 1)",
          fill: "backwards",
        }
      );
    });
  }

  /* ---------- Strip of the other shots ---------- */
  function buildStrip() {
    strip.textContent = "";
    thumbs = [];
    if (group.length < 2) {
      strip.hidden = true;
      return;
    }
    strip.hidden = false;
    group.forEach((link, i) => {
      const thumb = doc.createElement("button");
      thumb.type = "button";
      thumb.className = "lightbox-thumb";
      thumb.setAttribute("aria-label", label("Screenshot " + (i + 1), "الصورة " + (i + 1)));
      const source = link.querySelector("img");
      const src = source
        ? source.currentSrc || source.src
        : link.getAttribute("data-full") || link.getAttribute("href");
      const picture = doc.createElement("img");
      picture.src = src;
      picture.alt = "";
      thumb.appendChild(picture);
      thumb.addEventListener("click", () => {
        if (i !== index) show(i, { dir: i > index ? 1 : -1 });
      });
      strip.appendChild(thumb);
      thumbs.push(thumb);
    });
  }

  function markStrip() {
    thumbs.forEach((thumb, i) => {
      thumb.classList.toggle("is-active", i === index);
      if (i === index) thumb.setAttribute("aria-current", "true");
      else thumb.removeAttribute("aria-current");
    });
    const active = thumbs[index];
    if (active && active.scrollIntoView) {
      active.scrollIntoView({ block: "nearest", inline: "center" });
    }
  }

  /* ---------- Slides ---------- */
  function groupFor(link) {
    const parent = link.closest(".gallery, .showcase");
    const list = parent ? Array.from(parent.querySelectorAll(".shot-open")) : [link];
    return list.length ? list : [link];
  }

  function show(next, options) {
    if (!group.length) return;
    const opts = options || {};
    if (flight) {
      flight.cancel();
      flight = null;
    }
    const previous = index;
    index = (next + group.length) % group.length;
    current = group[index];

    const full = current.getAttribute("data-full") || current.getAttribute("href");
    const source = current.querySelector("img");
    const text = current.getAttribute("data-caption") || (source ? source.alt : "");

    resetZoom();
    img.src = full;
    img.alt = text;
    if (reflection) reflection.src = full;
    paintCaption(text);
    const many = group.length > 1;
    if (many) {
      count.innerHTML =
        "<b>" + (index + 1) + "</b><i>/</i><span>" + group.length + "</span>";
    } else {
      count.textContent = "";
    }
    if (progress) {
      progress.hidden = !many;
      box.style.setProperty(
        "--lb-progress",
        ((index + 1) / group.length) * 100 + "%"
      );
    }
    if (live) {
      live.textContent = many ? index + 1 + " / " + group.length + ". " + text : text;
    }

    btnPrev.hidden = !many;
    btnNext.hidden = !many;
    markStrip();

    if (many) {
      [index + 1, index - 1].forEach((n) => {
        const link = group[(n + group.length) % group.length];
        const src = link && (link.getAttribute("data-full") || link.getAttribute("href"));
        if (src) new Image().src = src;
      });
    }

    const dir = opts.dir || (index > previous ? 1 : -1);
    if (opts.dir && !reduced() && img.animate) {
      img.animate(
        [
          {
            opacity: 0,
            transform: "translateX(" + dir * (rtl() ? -26 : 26) + "px) scale(0.992)",
          },
          { opacity: 1, transform: "none" },
        ],
        { duration: 360, easing: "cubic-bezier(0.16, 1, 0.3, 1)" }
      );
    }
  }

  function step(delta) {
    show(index + delta, { dir: delta > 0 ? 1 : -1 });
  }

  /* ---------- Open: fly the picture out of its frame ---------- */
  function open(link, event) {
    ensure();
    resetZoom();
    token += 1;
    lastFocus = doc.activeElement;
    group = groupFor(link);
    index = Math.max(0, group.indexOf(link));
    current = link;

    const source = link.querySelector("img");
    const from = source ? source.getBoundingClientRect() : null;
    // A keyboard visitor has no pointer: the pool of light still has to open
    // somewhere on screen, even when the frame itself is far below the fold.
    const clampX = (value) => Math.min(window.innerWidth - 1, Math.max(0, value));
    const clampY = (value) => Math.min(window.innerHeight - 1, Math.max(0, value));
    const x = clampX(
      event && event.clientX ? event.clientX : from ? from.left + from.width / 2 : window.innerWidth / 2
    );
    const y = clampY(
      event && event.clientY ? event.clientY : from ? from.top + from.height / 2 : window.innerHeight / 2
    );

    box.style.setProperty("--lb-x", x.toFixed(0) + "px");
    box.style.setProperty("--lb-y", y.toFixed(0) + "px");
    box.style.setProperty("--lb-accent", accent());
    box.setAttribute("aria-label", label("Screenshot viewer", "عارض الصور"));
    btnClose.setAttribute("aria-label", label("Close", "إغلاق"));
    btnPrev.setAttribute("aria-label", label("Previous screenshot", "الصورة السابقة"));
    btnNext.setAttribute("aria-label", label("Next screenshot", "الصورة التالية"));
    strip.setAttribute("aria-label", label("Screenshots in this gallery", "صور هذه المجموعة"));
    const page = root.dataset.page || "home";
    if (brandName) {
      brandName.textContent =
        page === "droidcrate" ? "DroidCrate" : page === "majra" ? "Majra" : "Asraab";
    }
    if (brandKind) brandKind.textContent = label("Screenshots", "لقطات");

    buildStrip();
    lockScroll(true);
    setInert(true);
    box.classList.add("is-open");
    root.classList.add("lb-open");
    show(index, {});
    // Focus after the first style pass: a visibility:hidden dialog cannot take
    // focus in the same task that makes it visible.
    window.setTimeout(() => {
      if (!box || !box.classList.contains("is-open")) return;
      if (box.contains(doc.activeElement)) return;
      btnClose.focus();
    }, 90);

    if (!from || !from.width || reduced() || !img.animate) return;
    const grow = () => {
      if (!box.classList.contains("is-open")) return;
      const to = img.getBoundingClientRect();
      if (!to.width || !to.height) return;
      const scaleX = from.width / to.width;
      const scaleY = from.height / to.height;
      const dx = from.left + from.width / 2 - (to.left + to.width / 2);
      const dy = from.top + from.height / 2 - (to.top + to.height / 2);
      flight = img.animate(
        [
          {
            transform:
              "translate(" +
              dx.toFixed(1) +
              "px, " +
              dy.toFixed(1) +
              "px) scale(" +
              scaleX.toFixed(3) +
              ", " +
              scaleY.toFixed(3) +
              ")",
            opacity: 1,
          },
          { transform: "none", opacity: 1 },
        ],
        { duration: 620, easing: "cubic-bezier(0.16, 1, 0.3, 1)" }
      );
    };
    if (img.complete && img.naturalWidth) {
      requestAnimationFrame(grow);
    } else {
      img.addEventListener("load", () => requestAnimationFrame(grow), { once: true });
    }
  }

  /* ---------- Close: send it back to the frame it came from ---------- */
  function close() {
    if (!box || !box.classList.contains("is-open")) return;
    resetZoom();
    if (flight) {
      flight.cancel();
      flight = null;
    }
    const mine = ++token;
    box.classList.remove("is-open");
    root.classList.remove("lb-open");
    setInert(false);
    lockScroll(false);
    frame.style.removeProperty("--lb-rx");
    frame.style.removeProperty("--lb-ry");
    frame.style.removeProperty("--lb-mx");
    frame.style.removeProperty("--lb-my");
    img.style.removeProperty("--lb-px");
    img.style.removeProperty("--lb-py");
    stage.style.removeProperty("--lb-floor-x");

    const source = current ? current.querySelector("img") : null;
    const target = source ? source.getBoundingClientRect() : null;
    const here = img.getBoundingClientRect();
    const onScreen = target && target.bottom > 0 && target.top < window.innerHeight && target.width;

    if (target && onScreen && here.width && !reduced() && img.animate) {
      const scaleX = target.width / here.width;
      const scaleY = target.height / here.height;
      const dx = target.left + target.width / 2 - (here.left + here.width / 2);
      const dy = target.top + target.height / 2 - (here.top + here.height / 2);
      const back = img.animate(
        [
          { transform: "none", opacity: 1 },
          {
            transform:
              "translate(" +
              dx.toFixed(1) +
              "px, " +
              dy.toFixed(1) +
              "px) scale(" +
              scaleX.toFixed(3) +
              ", " +
              scaleY.toFixed(3) +
              ")",
            opacity: 0.2,
          },
        ],
        { duration: 420, easing: "cubic-bezier(0.4, 0, 0.2, 1)", fill: "forwards" }
      );
      back.onfinish = () => {
        if (mine !== token) return;
        clearImage();
        back.cancel();
      };
    } else if (img.animate && !reduced()) {
      const fade = img.animate(
        [
          { transform: "none", opacity: 1 },
          { transform: "scale(0.96)", opacity: 0 },
        ],
        { duration: 200, easing: "ease-out", fill: "forwards" }
      );
      fade.onfinish = () => {
        if (mine !== token) return;
        clearImage();
        fade.cancel();
      };
    } else {
      clearImage();
    }

    // A cancelled or interrupted flight must never leave the big image behind.
    window.setTimeout(() => {
      if (mine === token && !box.classList.contains("is-open")) clearImage();
    }, 480);

    if (lastFocus && lastFocus.focus) lastFocus.focus();
  }

  /* ---------- The page behind the viewer is not reachable ---------- */
  // aria-modal alone does not stop a tab from walking into the header, so the
  // rest of the document is made inert while the viewer is open.
  function setInert(on) {
    if (on) {
      if (inerted.length) return;
      Array.from(doc.body.children).forEach((el) => {
        if (el === box || el.tagName === "SCRIPT" || el.hasAttribute("aria-hidden")) return;
        if (el.hasAttribute("inert")) return;
        el.setAttribute("inert", "");
        inerted.push(el);
      });
      return;
    }
    inerted.forEach((el) => el.removeAttribute("inert"));
    inerted = [];
  }

  /* ---------- Scroll lock without the page jumping ---------- */
  function lockScroll(on) {
    if (on) {
      const gap = window.innerWidth - root.clientWidth;
      root.style.setProperty("--lb-gap", gap > 0 ? gap + "px" : "0px");
      root.classList.add("lb-locked");
    } else {
      root.classList.remove("lb-locked");
    }
  }

  /* ---------- Keyboard ---------- */
  function onKey(event) {
    if (!box || !box.classList.contains("is-open")) return;
    if (event.key === "Escape") {
      close();
      return;
    }
    if (event.key === "ArrowRight") {
      step(rtl() ? -1 : 1);
      return;
    }
    if (event.key === "ArrowLeft") {
      step(rtl() ? 1 : -1);
      return;
    }
    if (event.key === "+" || event.key === "=" || event.key === "-" || event.key === "_" || event.key === "0") {
      event.preventDefault();
      if (event.key === "0") setZoom(1);
      else setZoom(zoom * (event.key === "-" || event.key === "_" ? 0.8 : 1.25));
      return;
    }
    if (event.key !== "Tab") return;

    const stops = [btnClose, btnPrev, btnNext].concat(thumbs).filter((el) => el && !el.hidden);
    if (!stops.length) return;
    const first = stops[0];
    const last = stops[stops.length - 1];
    if (!event.shiftKey && doc.activeElement === last) {
      event.preventDefault();
      first.focus();
    } else if (event.shiftKey && doc.activeElement === first) {
      event.preventDefault();
      last.focus();
    }
  }

  /* ---------- Swipe on touch screens ---------- */
  function bindSwipe() {
    let startX = 0;
    let startY = 0;
    box.addEventListener(
      "touchstart",
      (event) => {
        if (event.touches.length !== 1) {
          startX = 0;
          return;
        }
        startX = event.touches[0].clientX;
        startY = event.touches[0].clientY;
      },
      { passive: true }
    );
    box.addEventListener(
      "touchend",
      (event) => {
        if (!startX || !event.changedTouches.length) return;
        const dx = event.changedTouches[0].clientX - startX;
        const dy = event.changedTouches[0].clientY - startY;
        startX = 0;
        if (Math.abs(dx) > 46 && Math.abs(dx) > Math.abs(dy)) step(dx < 0 ? 1 : -1);
      },
      { passive: true }
    );
  }

  /* ---------- The picture leans inside its frame ---------- */
  function bindTilt() {
    box.addEventListener(
      "pointermove",
      (event) => {
        if (!finePointer || reduced() || !box.classList.contains("is-open")) return;
        // A zoomed picture is being read, not admired: it stays square.
        if (zoom > 1.001 || drag) return;
        const rect = frame.getBoundingClientRect();
        if (!rect.width || !rect.height) return;
        const px = (event.clientX - rect.left) / rect.width - 0.5;
        const py = (event.clientY - rect.top) / rect.height - 0.5;
        if (tiltRaf) return;
        tiltRaf = requestAnimationFrame(() => {
          tiltRaf = 0;
          // Four degrees is enough to read as an object, small enough that the
          // screenshot stays square to the eye while it is being studied.
          frame.style.setProperty("--lb-rx", (-py * 4.2).toFixed(2) + "deg");
          frame.style.setProperty("--lb-ry", (px * 4.6).toFixed(2) + "deg");
          frame.style.setProperty("--lb-mx", ((px + 0.5) * 100).toFixed(1) + "%");
          frame.style.setProperty("--lb-my", ((py + 0.5) * 100).toFixed(1) + "%");
          img.style.setProperty("--lb-px", (-px * 16).toFixed(1) + "px");
          img.style.setProperty("--lb-py", (-py * 11).toFixed(1) + "px");
          stage.style.setProperty("--lb-floor-x", (px * 26).toFixed(1) + "px");
        });
      },
      { passive: true }
    );
    box.addEventListener("pointerleave", () => {
      frame.style.removeProperty("--lb-rx");
      frame.style.removeProperty("--lb-ry");
      frame.style.removeProperty("--lb-mx");
      frame.style.removeProperty("--lb-my");
      img.style.removeProperty("--lb-px");
      img.style.removeProperty("--lb-py");
      stage.style.removeProperty("--lb-floor-x");
    });
  }

  links.forEach((link) => {
    link.addEventListener("click", (event) => {
      event.preventDefault();
      open(link, event);
    });
  });
})();
