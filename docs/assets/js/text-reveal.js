/* Asraab written-in text.
 *
 * Headlines are typed in as the page scrolls: one character at a time in
 * Latin, one word at a time in Arabic (the script is joined, so cutting it
 * into letters would break the type), with a caret that travels with the
 * writing and blinks when it pauses. Scrolling back up un-types the line, so
 * the sentence is written again the next time it comes into view.
 *
 * Naming follows the platform rules: a heading can carry the sentence in
 * aria-label while its split copy is hidden, but a paragraph or a span cannot
 * (aria-label is prohibited there). Those keep their real text, which reads
 * correctly because words are never broken apart in running copy.
 */
(function () {
  "use strict";

  const doc = document;
  const root = doc.documentElement;
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  if (!("IntersectionObserver" in window)) return;

  const gsap = window.gsap;
  const ScrollTrigger = window.ScrollTrigger;
  const smooth = !!(gsap && ScrollTrigger);
  const MAX = 240;
  const HEADINGS = { H1: 1, H2: 1, H3: 1 };
  // Elements that may carry an author-provided accessible name. Everything
  // else (p, span) keeps its text in the accessibility tree instead.
  const NAMABLE = { H1: 1, H2: 1, H3: 1, H4: 1, H5: 1, H6: 1, A: 1, BUTTON: 1 };

  const SELECTOR = [
    ".display .line",
    ".h2",
    ".section-lede",
    ".kicker",
    ".app-tagline",
    ".cta-band h2",
    ".cta-band p",
    ".download h2",
    ".cross-band__copy h3",
    ".hero-copy .lede",
    ".page-hero .lede",
  ].join(",");

  const targets = Array.from(doc.querySelectorAll(SELECTOR));
  let built = [];
  let armed = false;

  const isArabic = () => root.lang === "ar" || root.dir === "rtl";

  function arm() {
    if (armed) return;
    armed = true;
    root.classList.add("tw-on");
    if (!smooth) root.classList.add("tw-fallback");
  }

  /* ---------- Build the split text ---------- */
  function wantsCaret(el) {
    if (el.classList.contains("grad-text")) return false;
    return HEADINGS[el.tagName] === 1 || el.classList.contains("line");
  }

  function build(el, text) {
    // Arabic joins its letters, so it is always written word by word. Latin
    // paragraphs are written by word too: per-letter inline boxes change the
    // kerning of running text, and hundreds of layers buy nothing.
    const byWord = isArabic() || text.length > 48;
    const visual = doc.createElement("span");
    visual.className = "tw-visual " + (byWord ? "tw-visual--words" : "tw-visual--chars");
    const units = [];

    text.split(/(\s+)/).forEach((part) => {
      if (!part) return;
      if (/^\s+$/.test(part)) {
        visual.appendChild(doc.createTextNode(part));
        return;
      }
      const word = doc.createElement("span");
      word.className = "tw";
      if (byWord) {
        word.textContent = part;
        units.push(word);
      } else {
        Array.from(part).forEach((letter) => {
          const glyph = doc.createElement("span");
          glyph.className = "tc";
          glyph.textContent = letter;
          word.appendChild(glyph);
          units.push(glyph);
        });
      }
      visual.appendChild(word);
    });

    const caret = wantsCaret(el) ? doc.createElement("span") : null;
    if (caret) {
      caret.className = "tw-caret";
      caret.setAttribute("aria-hidden", "true");
    }

    // A heading may be named by the author; a paragraph or a span may not, so
    // there the real text stays in the tree and the split copy is what is read.
    const named = NAMABLE[el.tagName] === 1;
    if (named) {
      el.setAttribute("aria-label", text);
      visual.setAttribute("aria-hidden", "true");
    }
    el.dataset.twText = text;
    el.textContent = "";
    el.appendChild(visual);
    if (caret) {
      if (getComputedStyle(el).position === "static") el.style.position = "relative";
      el.appendChild(caret);
    }
    if (smooth) {
      units.forEach((unit) => {
        unit.style.opacity = "0";
      });
    }

    return { el: el, units: units, visual: visual, caret: caret, shown: 0, tween: null, timer: 0 };
  }

  function split(el) {
    if (el.dataset.twSkip === "1") return null;
    if (el.children.length) {
      el.dataset.twSkip = "1";
      return null;
    }
    const text = (el.textContent || "").replace(/\s+/g, " ").trim();
    if (!text || text.length > MAX) {
      el.dataset.twSkip = "1";
      return null;
    }
    const item = build(el, text);
    built.push(item);
    return item;
  }

  /* ---------- The caret travels with the writing ---------- */
  function placeCaret(item, n) {
    const caret = item.caret;
    if (!caret) return;
    if (n <= 0) {
      caret.classList.remove("is-live");
      return;
    }
    const unit = item.units[Math.min(n, item.units.length) - 1];
    if (!unit) return;
    const box = item.el.getBoundingClientRect();
    const at = unit.getBoundingClientRect();
    // The caret sits where the next character would go: after the word in
    // Latin, before it in Arabic.
    const edge = isArabic() ? at.left - box.left - 2.5 : at.right - box.left + 1.5;
    caret.style.height = Math.round(at.height * 0.86) + "px";
    caret.style.transform =
      "translate3d(" +
      edge.toFixed(1) +
      "px," +
      (at.top - box.top + at.height * 0.07).toFixed(1) +
      "px,0)";
    caret.classList.add("is-live");
    if (item.timer) window.clearTimeout(item.timer);
    item.timer = window.setTimeout(function () {
      caret.classList.remove("is-live");
    }, 1400);
  }

  function apply(item, n) {
    if (n === item.shown) return;
    const units = item.units;
    if (n > item.shown) {
      for (let i = item.shown; i < n; i += 1) units[i].style.opacity = "1";
    } else {
      for (let i = n; i < item.shown; i += 1) units[i].style.opacity = "0";
    }
    item.shown = n;
    placeCaret(item, n);
  }

  /* ---------- Gradient text: a wipe, never a split ---------- */
  // The gradient is painted by the element's own background and clipped to its
  // glyphs. Wrapping the words in inline-blocks would cut the background off,
  // so this one keeps its plain text and is revealed with a wipe instead.
  function wipe(el) {
    el.style.clipPath = "";
    if (!smooth) {
      el.classList.add("tw-in");
      return;
    }
    const from = isArabic() ? "inset(0 0 0 100%)" : "inset(0 100% 0 0)";
    const rect = el.getBoundingClientRect();
    const inView = rect.top < window.innerHeight * 0.92 && rect.bottom > 0;

    if (inView) {
      gsap.fromTo(
        el,
        { clipPath: from },
        { clipPath: "inset(0 0 0 0)", duration: 1.1, ease: "power3.inOut", delay: 0.2 }
      );
      return;
    }

    const tween = gsap.fromTo(
      el,
      { clipPath: from },
      {
        clipPath: "inset(0 0 0 0)",
        ease: "none",
        scrollTrigger: { trigger: el, start: "top 96%", end: "top 62%", scrub: 0.4 },
      }
    );
    built.push({ el: el, units: [], visual: null, caret: null, shown: 0, tween: tween, timer: 0 });
  }

  /* ---------- Type it ---------- */
  function writeIn(item) {
    const el = item.el;
    const units = item.units;
    if (!units.length) return;

    if (!smooth) {
      // No GSAP: one IntersectionObserver pass with a per-unit CSS delay.
      units.forEach((unit, i) => {
        unit.style.transitionDelay = Math.min(420, i * 14) + "ms";
      });
      const io = new IntersectionObserver(
        (entries) => {
          entries.forEach((entry) => {
            if (!entry.isIntersecting) return;
            item.visual.classList.add("is-in");
            io.disconnect();
          });
        },
        { rootMargin: "0px 0px -10% 0px", threshold: 0.12 }
      );
      io.observe(item.el);
      return;
    }

    const rect = item.el.getBoundingClientRect();
    const inView = rect.top < window.innerHeight * 0.9 && rect.bottom > 0;

    if (inView) {
      // Already on screen at load: one timed pass, like a welcome line. Body
      // copy waits for the headline so the page reads in order.
      const late = /(^|\s)(lede|section-lede)(\s|$)/.test(el.className) ? 0.5 : 0;
      const state = { n: 0 };
      item.tween = gsap.to(state, {
        n: units.length,
        duration: Math.min(2.4, 0.5 + units.length * 0.04),
        delay: 0.35 + late,
        ease: "none",
        onUpdate: function () {
          apply(item, Math.round(state.n));
        },
      });
      return;
    }

    // Below the fold: the scroll position is the keyboard.
    const state = { n: 0 };
    item.tween = gsap.to(state, {
      n: units.length,
      duration: 1,
      ease: "none",
      scrollTrigger: { trigger: item.el, start: "top 90%", end: "top 58%", scrub: 0.4 },
      onUpdate: function () {
        apply(item, Math.round(state.n));
      },
    });
  }

  function prepare(el) {
    if (el.classList.contains("grad-text")) {
      wipe(el);
      return;
    }
    const item = split(el);
    if (item) writeIn(item);
  }

  /* ---------- Safety: never leave a sentence invisible ---------- */
  function rescue() {
    built.forEach((item) => {
      if (!item.units.length || item.shown > 0) return;
      const rect = item.el.getBoundingClientRect();
      if (rect.bottom < 0 || rect.top > window.innerHeight) return;
      if (item.tween) {
        item.tween.kill();
        item.tween = null;
      }
      apply(item, item.units.length);
    });
  }

  function run() {
    arm();
    targets.forEach(prepare);
    if (!built.length) {
      root.classList.remove("tw-on", "tw-fallback");
      return false;
    }
    if (smooth && ScrollTrigger) window.requestAnimationFrame(() => ScrollTrigger.refresh());
    return true;
  }

  if (!run()) return;

  window.setTimeout(rescue, 4000);
  window.addEventListener("load", () => window.setTimeout(rescue, 1200), { once: true });

  /* ---------- Language switch rebuilds the split copy ---------- */
  doc.addEventListener("asraab:lang", () => {
    built.forEach((item) => {
      if (item.tween) {
        try {
          item.tween.kill();
        } catch (e) {}
        item.tween = null;
      }
      if (item.timer) window.clearTimeout(item.timer);
    });
    built = [];

    targets.forEach((el) => {
      const translated = el.hasAttribute("data-i18n") || el.hasAttribute("data-i18n-html");
      // site.js already wrote the new copy for translated nodes. The others
      // kept their split markup, so restore the sentence we stored on them.
      if (!translated && el.dataset.twText) el.textContent = el.dataset.twText;
      delete el.dataset.twText;
      delete el.dataset.twSkip;
      prepare(el);
    });
    if (smooth && ScrollTrigger) window.requestAnimationFrame(() => ScrollTrigger.refresh());
    window.setTimeout(rescue, 1400);
  });
})();
