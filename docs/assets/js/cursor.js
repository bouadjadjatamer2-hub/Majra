/* Asraab view cursor.
 *
 * On a mouse, hovering a screenshot replaces the arrow with a small disc that
 * says what the click will do. It is only an affordance: the native cursor
 * stays everywhere else, touch devices never load it, and reduced motion gets
 * the plain arrow.
 */
(function () {
  "use strict";

  const doc = document;
  const root = doc.documentElement;
  if (!window.matchMedia("(hover: hover) and (pointer: fine)").matches) return;
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

  const shots = Array.from(doc.querySelectorAll(".shot-open"));
  if (!shots.length) return;

  const ICON =
    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
    '<path d="M9 4H4v5M15 4h5v5M9 20H4v-5M15 20h5v-5"/></svg>';

  const host = doc.createElement("span");
  host.className = "view-cursor";
  host.setAttribute("aria-hidden", "true");
  host.innerHTML =
    '<span class="view-cursor__disc">' + ICON + '<span class="view-cursor__label"></span></span>';
  doc.body.appendChild(host);

  root.classList.add("has-view-cursor");

  const label = host.querySelector(".view-cursor__label");
  const setLabel = () => {
    label.textContent = root.lang === "ar" ? "عرض" : "View";
  };
  setLabel();

  let active = false;
  let x = -200;
  let y = -200;
  let tx = -200;
  let ty = -200;
  let raf = 0;

  function draw() {
    raf = 0;
    if (!active) return;
    x += (tx - x) * 0.28;
    y += (ty - y) * 0.28;
    host.style.transform = "translate3d(" + x.toFixed(1) + "px," + y.toFixed(1) + "px,0)";
    if (Math.abs(tx - x) > 0.4 || Math.abs(ty - y) > 0.4) raf = window.requestAnimationFrame(draw);
  }

  function wake() {
    if (!raf && active) raf = window.requestAnimationFrame(draw);
  }

  function onMove(event) {
    tx = event.clientX;
    ty = event.clientY;
    wake();
  }

  function onEnter(event) {
    if (root.classList.contains("lb-open")) return;
    active = true;
    tx = x = event.clientX;
    ty = y = event.clientY;
    host.style.transform = "translate3d(" + x + "px," + y + "px,0)";
    host.classList.add("is-on");
    doc.addEventListener("pointermove", onMove, { passive: true });
  }

  function onLeave() {
    active = false;
    host.classList.remove("is-on");
    doc.removeEventListener("pointermove", onMove);
    if (raf) {
      window.cancelAnimationFrame(raf);
      raf = 0;
    }
  }

  shots.forEach((shot) => {
    shot.addEventListener("pointerenter", onEnter);
    shot.addEventListener("pointerleave", onLeave);
  });

  // The viewer takes over the screen: CSS fades the disc out while it is open,
  // so the disc is already in the right place when the viewer closes.
  doc.addEventListener("asraab:lang", setLabel);
})();
