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

  let group = [];
  let index = 0;
  let current = null;
  let lastFocus = null;
  let flight = null;
  let token = 0;
  let tiltRaf = 0;

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
      '<div class="lightbox-stage"><figure class="lightbox-frame"><img alt="" /></figure></div>' +
      '<button class="lightbox-close" type="button">' +
      ICON('<path d="M6 6l12 12M18 6 6 18" stroke-linecap="round"/>') +
      "</button>" +
      '<button class="lightbox-nav lightbox-prev" type="button">' +
      ICON('<path d="M15 5l-7 7 7 7" stroke-linecap="round" stroke-linejoin="round"/>') +
      "</button>" +
      '<button class="lightbox-nav lightbox-next" type="button">' +
      ICON('<path d="M9 5l7 7-7 7" stroke-linecap="round" stroke-linejoin="round"/>') +
      "</button>" +
      '<div class="lightbox-bar"><span class="lightbox-caption"></span>' +
      '<span class="lightbox-count"></span></div>' +
      '<div class="lightbox-strip"></div>';
    doc.body.appendChild(box);

    scrim = box.querySelector(".lightbox-scrim");
    stage = box.querySelector(".lightbox-stage");
    frame = box.querySelector(".lightbox-frame");
    img = box.querySelector("img");
    btnClose = box.querySelector(".lightbox-close");
    btnPrev = box.querySelector(".lightbox-prev");
    btnNext = box.querySelector(".lightbox-next");
    bar = box.querySelector(".lightbox-bar");
    caption = box.querySelector(".lightbox-caption");
    count = box.querySelector(".lightbox-count");
    strip = box.querySelector(".lightbox-strip");

    btnClose.addEventListener("click", close);
    btnPrev.addEventListener("click", () => step(rtl() ? 1 : -1));
    btnNext.addEventListener("click", () => step(rtl() ? -1 : 1));

    box.addEventListener("click", (event) => {
      if (event.target === box || event.target === scrim || event.target === stage) close();
    });

    doc.addEventListener("keydown", onKey);
    bindSwipe();
    bindTilt();
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

    img.src = full;
    img.alt = text;
    paintCaption(text);
    count.textContent = group.length > 1 ? index + 1 + " / " + group.length : "";

    const many = group.length > 1;
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
    token += 1;
    lastFocus = doc.activeElement;
    group = groupFor(link);
    index = Math.max(0, group.indexOf(link));
    current = link;

    const source = link.querySelector("img");
    const from = source ? source.getBoundingClientRect() : null;
    const x = event && event.clientX ? event.clientX : from ? from.left + from.width / 2 : window.innerWidth / 2;
    const y = event && event.clientY ? event.clientY : from ? from.top + from.height / 2 : window.innerHeight / 2;

    box.style.setProperty("--lb-x", x.toFixed(0) + "px");
    box.style.setProperty("--lb-y", y.toFixed(0) + "px");
    box.style.setProperty("--lb-accent", accent());
    box.setAttribute("aria-label", label("Screenshot viewer", "عارض الصور"));
    btnClose.setAttribute("aria-label", label("Close", "إغلاق"));
    btnPrev.setAttribute("aria-label", label("Previous screenshot", "الصورة السابقة"));
    btnNext.setAttribute("aria-label", label("Next screenshot", "الصورة التالية"));
    strip.setAttribute("aria-label", label("Screenshots in this gallery", "صور هذه المجموعة"));

    buildStrip();
    lockScroll(true);
    box.classList.add("is-open");
    root.classList.add("lb-open");
    show(index, {});
    btnClose.focus();

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
    if (flight) {
      flight.cancel();
      flight = null;
    }
    const mine = ++token;
    box.classList.remove("is-open");
    root.classList.remove("lb-open");
    lockScroll(false);
    frame.style.removeProperty("--lb-rx");
    frame.style.removeProperty("--lb-ry");
    img.style.removeProperty("--lb-px");
    img.style.removeProperty("--lb-py");

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
        img.removeAttribute("src");
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
        img.removeAttribute("src");
        fade.cancel();
      };
    } else {
      img.removeAttribute("src");
    }

    // A cancelled or interrupted flight must never leave the big image behind.
    window.setTimeout(() => {
      if (mine === token && !box.classList.contains("is-open")) img.removeAttribute("src");
    }, 480);

    if (lastFocus && lastFocus.focus) lastFocus.focus();
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
        if (event.touches.length !== 1) return;
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
        const rect = frame.getBoundingClientRect();
        if (!rect.width || !rect.height) return;
        const px = (event.clientX - rect.left) / rect.width - 0.5;
        const py = (event.clientY - rect.top) / rect.height - 0.5;
        if (tiltRaf) return;
        tiltRaf = requestAnimationFrame(() => {
          tiltRaf = 0;
          frame.style.setProperty("--lb-rx", (-py * 2.2).toFixed(2) + "deg");
          frame.style.setProperty("--lb-ry", (px * 2.2).toFixed(2) + "deg");
          img.style.setProperty("--lb-px", (-px * 10).toFixed(1) + "px");
          img.style.setProperty("--lb-py", (-py * 7).toFixed(1) + "px");
        });
      },
      { passive: true }
    );
    box.addEventListener("pointerleave", () => {
      frame.style.removeProperty("--lb-rx");
      frame.style.removeProperty("--lb-ry");
      img.style.removeProperty("--lb-px");
      img.style.removeProperty("--lb-py");
    });
  }

  links.forEach((link) => {
    link.addEventListener("click", (event) => {
      event.preventDefault();
      open(link, event);
    });
  });
})();
