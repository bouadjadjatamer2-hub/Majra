/* Asraab motion layer — smooth scroll, depth, and page transitions.
 *
 * This file is optional on purpose. If GSAP, ScrollTrigger or Lenis are
 * missing, or the visitor asks for reduced motion, the site keeps the plain
 * CSS behaviour it had before. Nothing here is required to read the pages.
 */
(function () {
  "use strict";

  const root = document.documentElement;
  const body = document.body;
  const reduceQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
  const finePointer = window.matchMedia("(hover: hover) and (pointer: fine)").matches;
  const gsap = window.gsap;
  const ScrollTrigger = window.ScrollTrigger;
  const Lenis = window.Lenis;
  const reduced = reduceQuery.matches;
  const canMotion = !reduced && !!gsap && !!ScrollTrigger;
  const canSmooth = canMotion && typeof Lenis === "function";

  root.classList.add(canMotion ? "motion-ready" : "motion-static");

  /* ---------- Scroll progress ---------- */
  const progress = document.createElement("div");
  progress.className = "scroll-progress";
  progress.setAttribute("aria-hidden", "true");
  const progressFill = document.createElement("span");
  progress.appendChild(progressFill);
  body.appendChild(progress);

  let lastProgress = -1;

  function paintProgress() {
    const max = document.documentElement.scrollHeight - window.innerHeight;
    const value = max > 0 ? Math.min(1, Math.max(0, window.scrollY / max)) : 0;
    if (value === lastProgress) return;
    lastProgress = value;
    progressFill.style.transform = "scaleX(" + value.toFixed(4) + ")";
  }
  paintProgress();
  window.addEventListener("scroll", paintProgress, { passive: true });
  window.addEventListener("resize", paintProgress);

  /* ---------- Smooth scrolling ---------- */
  let lenis = null;
  if (canSmooth) {
    lenis = new Lenis({
      duration: 1.05,
      easing: (t) => Math.min(1, 1.001 - Math.pow(2, -10 * t)),
      smoothWheel: true,
      wheelMultiplier: 1,
      touchMultiplier: 1.5,
      syncTouch: false,
    });
    lenis.on("scroll", () => {
      paintProgress();
      ScrollTrigger.update();
    });
    const raf = (time) => {
      lenis.raf(time);
      paintProgress();
      requestAnimationFrame(raf);
    };
    requestAnimationFrame(raf);
  }

  /* ---------- Anchor links ---------- */
  const header = document.querySelector(".site-header");

  function scrollToTarget(target) {
    const offset = -((header ? header.offsetHeight : 0) + 8);
    if (lenis) {
      lenis.scrollTo(target, { offset, duration: 1.15 });
      return;
    }
    const top = target.getBoundingClientRect().top + window.scrollY + offset;
    window.scrollTo({ top, behavior: reduced ? "auto" : "smooth" });
  }

  document.addEventListener("click", (event) => {
    const link = event.target && event.target.closest ? event.target.closest('a[href^="#"]') : null;
    if (!link) return;
    const hash = link.getAttribute("href");
    if (!hash || hash === "#") return;
    let target;
    try {
      target = document.querySelector(hash);
    } catch (error) {
      return;
    }
    if (!target) return;
    event.preventDefault();
    scrollToTarget(target);
    if (window.history && history.pushState) history.pushState(null, "", hash);
  });

  /* ---------- Cursor light ---------- */
  if (canMotion && finePointer) {
    const glow = document.createElement("div");
    glow.className = "cursor-glow";
    glow.setAttribute("aria-hidden", "true");
    body.appendChild(glow);

    let x = window.innerWidth / 2;
    let y = window.innerHeight / 2;

    window.addEventListener(
      "pointermove",
      (event) => {
        x = event.clientX;
        y = event.clientY;
        glow.classList.add("is-on");
        glow.style.transform = "translate3d(" + x.toFixed(1) + "px," + y.toFixed(1) + "px,0)";
      },
      { passive: true }
    );
    document.addEventListener("pointerleave", () => glow.classList.remove("is-on"));
  }

  /* ---------- Spotlight on tilted cards ---------- */
  if (canMotion && finePointer) {
    document.querySelectorAll("[data-tilt]").forEach((card) => {
      card.addEventListener(
        "pointermove",
        (event) => {
          const rect = card.getBoundingClientRect();
          const mx = ((event.clientX - rect.left) / rect.width) * 100;
          const my = ((event.clientY - rect.top) / rect.height) * 100;
          card.style.setProperty("--mx", mx.toFixed(1) + "%");
          card.style.setProperty("--my", my.toFixed(1) + "%");
          card.classList.add("is-tilting");
        },
        { passive: true }
      );
      card.addEventListener("pointerleave", () => card.classList.remove("is-tilting"));
    });
  }

  /* ---------- Scroll-linked depth ---------- */
  if (!canMotion) return;

  gsap.registerPlugin(ScrollTrigger);
  ScrollTrigger.config({ ignoreMobileResize: true });

  const compact = window.matchMedia("(max-width: 900px)").matches;

  // A handful of blocks earn a slightly deeper entrance. They already have a
  // `.reveal` class; the attribute only changes the transform the CSS uses.
  document
    .querySelectorAll(
      ".section-head, .feature, .step, .limit, .callout, .tab-card, .download, .facts, .panel, .faq details, .cta-band h2, .cta-band p, .cta-band .btn-row, .cross-band__copy"
    )
    .forEach((el) => {
      if (el.classList.contains("reveal") || el.classList.contains("reveal-scale")) {
        el.setAttribute("data-reveal", "3d");
      }
    });

  // Hero: the copy settles back while the WebGL frame leans away.
  const hero = document.querySelector(".hero, .page-hero");
  if (hero) {
    const copy = hero.querySelector(".hero-copy");
    const scene = hero.querySelector("[data-depth-scene]");

    if (copy) {
      gsap.to(copy, {
        yPercent: compact ? -4 : -9,
        opacity: 0.55,
        ease: "none",
        scrollTrigger: { trigger: hero, start: "top top", end: "bottom top", scrub: true },
      });
    }

    if (scene) {
      gsap.fromTo(
        scene,
        { rotateX: 0, rotateY: 0, yPercent: 0 },
        {
          rotateX: compact ? 4 : 9,
          rotateY: compact ? -3 : -7,
          yPercent: compact ? -3 : -7,
          ease: "none",
          scrollTrigger: { trigger: hero, start: "top top", end: "bottom top", scrub: 1 },
        }
      );
    }
  }

  // Grids and galleries drift against the page a little. Transforms stay on
  // the container so the reveal and tilt transforms on the children are safe.
  document.querySelectorAll("[data-parallax]").forEach((el) => {
    const amount = (parseFloat(el.dataset.parallax) || 0.35) * (compact ? 12 : 26);
    gsap.fromTo(
      el,
      { y: amount },
      {
        y: -amount,
        ease: "none",
        scrollTrigger: { trigger: el, start: "top bottom", end: "bottom top", scrub: 0.7 },
      }
    );
  });

  // Rules draw themselves in as they enter the viewport.
  document.querySelectorAll(".section-rule").forEach((rule) => {
    gsap.fromTo(
      rule,
      { scaleX: 0.4, opacity: 0.2 },
      {
        scaleX: 1,
        opacity: 1,
        ease: "none",
        scrollTrigger: { trigger: rule, start: "top 92%", end: "top 55%", scrub: true },
      }
    );
  });

  // Layout can shift after fonts and lazy images arrive.
  window.addEventListener("load", () => ScrollTrigger.refresh());
  if (document.fonts && document.fonts.ready) {
    document.fonts.ready.then(() => ScrollTrigger.refresh()).catch(() => {});
  }
})();
