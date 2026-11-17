/* Asraab chapter rail.
 *
 * A thin index on the edge of the page: one dot per section, the section you
 * are reading is lit, and hovering the rail names them. The labels come from
 * the page itself (kicker, then heading), so nothing is invented and the rail
 * follows the language switch for free.
 */
(function () {
  "use strict";

  const doc = document;
  const root = doc.documentElement;
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const sections = Array.from(doc.querySelectorAll("main section[id]")).filter(
    (section) => section.querySelector(".kicker, .h2, h1")
  );
  if (sections.length < 3) return;

  const nav = doc.createElement("nav");
  nav.className = "chapters";
  nav.setAttribute("aria-label", "Sections");
  doc.body.appendChild(nav);

  let items = [];
  let active = -1;
  let frame = 0;

  const label = (en, ar) => (root.lang === "ar" ? ar : en);

  function textOf(section) {
    const source = section.querySelector(".kicker") || section.querySelector(".h2, h1");
    if (!source) return "";
    const text = source.getAttribute("aria-label") || source.textContent || "";
    return text.replace(/\s+/g, " ").trim();
  }

  function build() {
    nav.textContent = "";
    items = sections.map((section) => {
      const link = doc.createElement("a");
      link.className = "chapters__link";
      link.href = "#" + section.id;
      link.innerHTML = '<span class="chapters__label"></span><span class="chapters__dot"></span>';
      link.querySelector(".chapters__label").textContent = textOf(section);
      link.setAttribute("aria-label", textOf(section));
      nav.appendChild(link);
      return { section: section, link: link };
    });
    active = -1;
    nav.setAttribute("aria-label", label("Sections", "الأقسام"));
    paint();
  }

  function paint() {
    const line = window.innerHeight * 0.42;
    let next = -1;
    for (let i = 0; i < items.length; i += 1) {
      const rect = items[i].section.getBoundingClientRect();
      if (rect.top <= line) next = i;
      if (rect.top <= line && rect.bottom > line) {
        next = i;
        break;
      }
    }
    if (next === active) return;
    active = next;
    items.forEach((item, i) => {
      item.link.classList.toggle("is-active", i === active);
      if (i === active) item.link.setAttribute("aria-current", "true");
      else item.link.removeAttribute("aria-current");
    });
  }

  function onScroll() {
    if (frame) return;
    frame = window.requestAnimationFrame(() => {
      frame = 0;
      paint();
    });
  }

  build();
  window.addEventListener("scroll", onScroll, { passive: true });
  window.addEventListener("resize", onScroll, { passive: true });
  window.addEventListener("load", onScroll, { once: true });

  doc.addEventListener("asraab:lang", build);

  // Reduced motion should not change the rail; it is information, not motion.
  if (reduced) nav.classList.add("chapters--still");
})();
