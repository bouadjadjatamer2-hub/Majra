/* Asraab site behaviour: language, navigation, reveal, tilt, lightbox, copy. */
(function () {
  "use strict";

  const root = document.documentElement;
  const DICT = (window.ASRAAB_I18N && window.ASRAAB_I18N.ar) || {};
  const STORAGE_KEY = "asraab:lang";
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const finePointer = window.matchMedia("(hover: hover) and (pointer: fine)").matches;

  /* ---------- Language ---------- */
  const textNodes = Array.from(document.querySelectorAll("[data-i18n]"));
  const htmlNodes = Array.from(document.querySelectorAll("[data-i18n-html]"));
  const ariaNodes = Array.from(document.querySelectorAll("[data-i18n-aria]"));

  textNodes.forEach((el) => {
    if (el.dataset.enText === undefined) el.dataset.enText = el.textContent;
  });
  htmlNodes.forEach((el) => {
    if (el.dataset.enHtml === undefined) el.dataset.enHtml = el.innerHTML;
  });
  ariaNodes.forEach((el) => {
    if (el.dataset.enAria === undefined) el.dataset.enAria = el.getAttribute("aria-label") || "";
  });

  const metaDescription = document.querySelector('meta[name="description"]');
  const originalTitle = document.title;
  const originalDescription = metaDescription ? metaDescription.getAttribute("content") : "";
  const ogTitle = document.querySelector('meta[property="og:title"]');
  const ogDescription = document.querySelector('meta[property="og:description"]');
  const originalOgTitle = ogTitle ? ogTitle.getAttribute("content") : "";
  const originalOgDescription = ogDescription ? ogDescription.getAttribute("content") : "";
  const page = root.dataset.page || "home";

  let lang = "en";
  try {
    lang = localStorage.getItem(STORAGE_KEY) === "ar" ? "ar" : "en";
  } catch (e) {}

  function translate(key, fallback) {
    if (lang === "ar" && DICT[key]) return DICT[key];
    return fallback;
  }

  function applyLanguage(next) {
    lang = next === "ar" ? "ar" : "en";
    const isAr = lang === "ar";
    root.lang = isAr ? "ar" : "en";
    root.dir = isAr ? "rtl" : "ltr";

    textNodes.forEach((el) => {
      const value = isAr && DICT[el.dataset.i18n] ? DICT[el.dataset.i18n] : el.dataset.enText;
      if (value !== undefined) el.textContent = value;
    });
    htmlNodes.forEach((el) => {
      const value = isAr && DICT[el.dataset.i18nHtml] ? DICT[el.dataset.i18nHtml] : el.dataset.enHtml;
      if (value !== undefined) el.innerHTML = value;
    });
    ariaNodes.forEach((el) => {
      const value = isAr && DICT[el.dataset.i18nAria] ? DICT[el.dataset.i18nAria] : el.dataset.enAria;
      if (value !== undefined) el.setAttribute("aria-label", value);
    });

    const titleKey = "meta." + page + ".title";
    const descKey = "meta." + page + ".description";
    document.title = isAr && DICT[titleKey] ? DICT[titleKey] : originalTitle;
    if (metaDescription) {
      metaDescription.setAttribute("content", isAr && DICT[descKey] ? DICT[descKey] : originalDescription);
    }
    if (ogTitle) {
      ogTitle.setAttribute("content", isAr && DICT[titleKey] ? DICT[titleKey] : originalOgTitle);
    }
    if (ogDescription) {
      ogDescription.setAttribute("content", isAr && DICT[descKey] ? DICT[descKey] : originalOgDescription);
    }

    const toggle = document.getElementById("langToggle");
    if (toggle) {
      toggle.textContent = isAr ? "EN" : "ع";
      toggle.setAttribute("aria-label", isAr ? "Switch to English" : "التبديل إلى العربية");
      toggle.setAttribute("aria-pressed", String(isAr));
    }

    const menuBtn = document.getElementById("menuBtn");
    if (menuBtn) {
      menuBtn.setAttribute("aria-label", isAr ? "فتح القائمة" : "Open menu");
    }

    try {
      localStorage.setItem(STORAGE_KEY, lang);
    } catch (e) {}
  }

  const langToggle = document.getElementById("langToggle");
  if (langToggle) {
    langToggle.addEventListener("click", () => {
      applyLanguage(lang === "ar" ? "en" : "ar");
    });
  }
  applyLanguage(lang);

  /* ---------- Header and mobile navigation ---------- */
  const header = document.querySelector(".site-header");
  const menuBtn = document.getElementById("menuBtn");
  const nav = document.getElementById("nav");

  const onScroll = () => {
    if (!header) return;
    header.classList.toggle("is-stuck", window.scrollY > 12);
  };
  onScroll();
  window.addEventListener("scroll", onScroll, { passive: true });

  function closeMenu() {
    document.body.classList.remove("nav-open");
    if (menuBtn) {
      menuBtn.setAttribute("aria-expanded", "false");
      menuBtn.setAttribute("aria-label", lang === "ar" ? "فتح القائمة" : "Open menu");
    }
  }

  if (menuBtn) {
    menuBtn.addEventListener("click", () => {
      const open = document.body.classList.toggle("nav-open");
      menuBtn.setAttribute("aria-expanded", String(open));
      menuBtn.setAttribute("aria-label", open ? (lang === "ar" ? "إغلاق القائمة" : "Close menu") : lang === "ar" ? "فتح القائمة" : "Open menu");
    });
  }
  if (nav) {
    nav.addEventListener("click", (event) => {
      if (event.target.closest("a")) closeMenu();
    });
  }
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") closeMenu();
  });
  window.addEventListener("resize", () => {
    if (window.innerWidth > 900) closeMenu();
  });

  /* ---------- Year ---------- */
  const year = document.getElementById("year");
  if (year) year.textContent = String(new Date().getFullYear());

  /* ---------- Reveal ---------- */
  const revealNodes = Array.from(document.querySelectorAll(".reveal, .reveal-scale"));
  if (reducedMotion || !("IntersectionObserver" in window)) {
    revealNodes.forEach((el) => el.classList.add("is-in"));
  } else {
    const pending = new Set(revealNodes);
    const reveal = (el) => {
      el.classList.add("is-in");
      pending.delete(el);
      io.unobserve(el);
    };

    const io = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) reveal(entry.target);
        });
      },
      { rootMargin: "0px 0px -8% 0px", threshold: 0.08 }
    );
    revealNodes.forEach((el) => io.observe(el));

    // Fast scrolling can outrun the observer. A cheap geometry check on scroll
    // keeps every section visible even when frames are busy with WebGL.
    let lastCheck = 0;
    const checkVisible = () => {
      const now = performance.now();
      if (now - lastCheck < 100) return;
      lastCheck = now;
      const height = window.innerHeight;
      pending.forEach((el) => {
        const rect = el.getBoundingClientRect();
        if (rect.top < height * 0.94 && rect.bottom > 0) reveal(el);
      });
    };
    window.addEventListener("scroll", checkVisible, { passive: true });
    window.addEventListener("resize", checkVisible);

    // Safety net: never leave content hidden if both paths miss an element
    // (print, restored scroll positions, unusual embedded browsers).
    window.setTimeout(() => {
      revealNodes.forEach((el) => el.classList.add("is-in"));
    }, 4000);
  }

  /* ---------- 3D tilt on app cards ---------- */
  if (finePointer && !reducedMotion) {
    document.querySelectorAll("[data-tilt]").forEach((card) => {
      let raf = 0;
      let targetX = 0;
      let targetY = 0;

      const render = () => {
        raf = 0;
        card.style.transform =
          "perspective(1200px) rotateX(" + targetY.toFixed(2) + "deg) rotateY(" + targetX.toFixed(2) + "deg)";
      };

      card.addEventListener("pointermove", (event) => {
        const rect = card.getBoundingClientRect();
        targetX = ((event.clientX - rect.left) / rect.width - 0.5) * 7;
        targetY = -((event.clientY - rect.top) / rect.height - 0.5) * 7;
        if (!raf) raf = requestAnimationFrame(render);
      });
      card.addEventListener("pointerleave", () => {
        targetX = 0;
        targetY = 0;
        card.style.transform = "";
      });
    });
  }

  /* ---------- Lightbox ---------- */
  const shotLinks = Array.from(document.querySelectorAll(".shot-open"));
  let lightbox = null;
  let lightboxImg = null;
  let lightboxClose = null;
  let lastFocus = null;

  function ensureLightbox() {
    if (lightbox) return;
    lightbox = document.createElement("div");
    lightbox.className = "lightbox";
    lightbox.setAttribute("role", "dialog");
    lightbox.setAttribute("aria-modal", "true");
    lightbox.innerHTML =
      '<button class="lightbox-close" type="button" aria-label="Close">' +
      '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" stroke-linecap="round"/></svg>' +
      "</button>";
    lightboxImg = document.createElement("img");
    lightboxImg.alt = "";
    lightbox.appendChild(lightboxImg);
    document.body.appendChild(lightbox);
    lightboxClose = lightbox.querySelector(".lightbox-close");

    lightboxClose.addEventListener("click", closeLightbox);
    lightbox.addEventListener("click", (event) => {
      if (event.target === lightbox) closeLightbox();
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && lightbox.classList.contains("is-open")) closeLightbox();
    });
  }

  function openLightbox(link) {
    ensureLightbox();
    lastFocus = document.activeElement;
    const full = link.getAttribute("data-full") || link.getAttribute("href");
    const img = link.querySelector("img");
    lightboxImg.src = full;
    lightboxImg.alt = img ? img.alt : "";
    lightbox.classList.add("is-open");
    document.body.style.overflow = "hidden";
    lightboxClose.setAttribute("aria-label", lang === "ar" ? "إغلاق" : "Close");
    lightboxClose.focus();
  }

  function closeLightbox() {
    if (!lightbox) return;
    lightbox.classList.remove("is-open");
    document.body.style.overflow = "";
    lightboxImg.removeAttribute("src");
    if (lastFocus && lastFocus.focus) lastFocus.focus();
  }

  shotLinks.forEach((link) => {
    link.addEventListener("click", (event) => {
      event.preventDefault();
      openLightbox(link);
    });
  });

  /* ---------- Copy buttons ---------- */
  document.querySelectorAll("[data-copy]").forEach((button) => {
    button.addEventListener("click", async () => {
      const target = document.querySelector(button.getAttribute("data-copy"));
      if (!target) return;
      const text = target.textContent;
      let ok = false;
      try {
        await navigator.clipboard.writeText(text);
        ok = true;
      } catch (e) {
        const area = document.createElement("textarea");
        area.value = text;
        area.setAttribute("readonly", "");
        area.style.position = "fixed";
        area.style.opacity = "0";
        document.body.appendChild(area);
        area.select();
        try {
          ok = document.execCommand("copy");
        } catch (err) {
          ok = false;
        }
        area.remove();
      }
      const original = button.textContent;
      button.textContent = ok ? (lang === "ar" ? "تم النسخ" : "Copied") : lang === "ar" ? "تعذّر" : "Failed";
      button.classList.toggle("is-done", ok);
      window.setTimeout(() => {
        button.textContent = original;
        button.classList.remove("is-done");
      }, 1800);
    });
  });

  /* ---------- Scene fallback ---------- */
  window.setTimeout(() => {
    document.querySelectorAll(".scene[data-scene]").forEach((container) => {
      const frame = container.closest(".scene-frame");
      if (!frame) return;
      if (container.querySelector("canvas") || frame.querySelector(".scene-fallback")) return;
      frame.classList.add("is-fallback");
      const fallback = document.createElement("div");
      fallback.className = "scene-fallback";
      const icon = document.createElement("img");
      icon.src = container.dataset.fallback || "";
      icon.alt = "";
      fallback.appendChild(icon);
      frame.appendChild(fallback);
    });
  }, 3200);
})();
