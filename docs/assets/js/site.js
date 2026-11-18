/* Asraab site behaviour: language, navigation, reveal, tilt, copy. */
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

    // Other layers (the text reveal) rebuild their split text from the new copy.
    document.dispatchEvent(new CustomEvent("asraab:lang", { detail: { lang: lang } }));
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

  function setMenuInert(on) {
    document.querySelectorAll("main, .site-footer").forEach((el) => {
      if (on) el.setAttribute("inert", "");
      else el.removeAttribute("inert");
    });
  }

  function closeMenu(restoreFocus) {
    if (!document.body.classList.contains("nav-open")) return;
    document.body.classList.remove("nav-open");
    setMenuInert(false);
    if (menuBtn) {
      menuBtn.setAttribute("aria-expanded", "false");
      menuBtn.setAttribute("aria-label", lang === "ar" ? "فتح القائمة" : "Open menu");
      if (restoreFocus) menuBtn.focus();
    }
  }

  if (menuBtn) {
    menuBtn.addEventListener("click", () => {
      const open = !document.body.classList.contains("nav-open");
      document.body.classList.toggle("nav-open", open);
      menuBtn.setAttribute("aria-expanded", String(open));
      menuBtn.setAttribute("aria-label", open ? (lang === "ar" ? "إغلاق القائمة" : "Close menu") : lang === "ar" ? "فتح القائمة" : "Open menu");
      setMenuInert(open);
      if (open) {
        const first = nav && nav.querySelector("a");
        // Wait for the panel to be visible: focus() on a hidden link is lost.
        if (first) {
          window.setTimeout(() => {
            if (document.body.classList.contains("nav-open") && document.activeElement === menuBtn) first.focus();
          }, 220);
        }
      }
    });
  }
  if (nav) {
    nav.addEventListener("click", (event) => {
      if (event.target.closest("a")) closeMenu(false);
    });
  }
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") closeMenu(true);
  });
  document.addEventListener(
    "pointerdown",
    (event) => {
      if (!document.body.classList.contains("nav-open")) return;
      if (event.target.closest(".site-header")) return;
      closeMenu(false);
    },
    { passive: true }
  );
  window.addEventListener("resize", () => {
    if (window.innerWidth > 900) closeMenu(false);
  });

  /* ---------- Year ---------- */
  const year = document.getElementById("year");
  if (year) year.textContent = String(new Date().getFullYear());

  /* ---------- Phone-sized download dock ---------- */
  // The header CTA is hidden under 900px, so a small bar takes over once the
  // hero is behind: it stays out of the way near the real download section
  // and near the footer.
  const dock = document.querySelector("[data-dock]");
  if (dock) {
    const download = document.getElementById("download");
    let dockOn = false;
    const updateDock = () => {
      const y = window.scrollY || window.pageYOffset || 0;
      const height = window.innerHeight;
      let nearDownload = false;
      if (download) {
        const rect = download.getBoundingClientRect();
        nearDownload = rect.top < height * 0.9 && rect.bottom > height * 0.35;
      }
      const nearEnd = document.documentElement.scrollHeight - (y + height) < 260;
      const on = y > 560 && !nearDownload && !nearEnd;
      if (on === dockOn) return;
      dockOn = on;
      dock.classList.toggle("is-on", on);
    };
    window.addEventListener("scroll", updateDock, { passive: true });
    window.addEventListener("resize", updateDock);
    updateDock();
  }

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

  /* ---------- 3D tilt on cards ---------- */
  const depthTargets = document.querySelectorAll(
    ".feature, .step, .tab-card, .download, .panel, .limits, .facts, .callout, .faq details"
  );
  depthTargets.forEach((el) => el.classList.add("depth-card"));

  if (finePointer && !reducedMotion) {
    // Large containers are animated by ScrollTrigger instead, so they are not
    // tilted here to avoid two owners of the same transform.
    const tiltTargets = Array.from(
      document.querySelectorAll("[data-tilt], .feature, .step, .tab-card, .panel, .callout, .faq details")
    );

    tiltTargets.forEach((card) => {
      if (!card.hasAttribute("data-tilt")) {
        card.setAttribute("data-tilt", card.matches(".faq details") ? "2.4" : "4");
      }

      const max = parseFloat(card.dataset.tilt) || 7;
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
        targetX = ((event.clientX - rect.left) / rect.width - 0.5) * max;
        targetY = -((event.clientY - rect.top) / rect.height - 0.5) * max;
        if (!raf) raf = requestAnimationFrame(render);
      });
      card.addEventListener("pointerleave", () => {
        targetX = 0;
        targetY = 0;
        card.style.transform = "";
      });
    });
  }

  /* ---------- Screenshot peek ---------- */
  // The frame leans toward the pointer, the picture inside drifts against it,
  // and the light and the rim glow follow the cursor. Everything is written as
  // custom properties, so the reveal animation and the hover never fight over
  // `transform`. Clamped on purpose: the card lifts inside its own cell.
  if (finePointer && !reducedMotion) {
    document.querySelectorAll(".shot").forEach((card) => {
      if (!card.querySelector(".shot-open")) return;

      let raf = 0;
      let tiltX = 0;
      let tiltY = 0;
      let imgX = 0;
      let imgY = 0;
      let glareX = 50;
      let glareY = 50;

      const ready = () =>
        !card.classList.contains("reveal-scale") || card.classList.contains("is-in");

      const render = () => {
        raf = 0;
        card.style.setProperty("--rx", tiltY.toFixed(2) + "deg");
        card.style.setProperty("--ry", tiltX.toFixed(2) + "deg");
        card.style.setProperty("--img-x", imgX.toFixed(1) + "px");
        card.style.setProperty("--img-y", imgY.toFixed(1) + "px");
        card.style.setProperty("--mx", glareX.toFixed(1) + "%");
        card.style.setProperty("--my", glareY.toFixed(1) + "%");
      };

      card.addEventListener("pointerenter", () => {
        if (!ready()) return;
        card.classList.add("is-peeking");
      });

      card.addEventListener("pointermove", (event) => {
        if (!ready()) return;
        const rect = card.getBoundingClientRect();
        if (!rect.width || !rect.height) return;
        const px = (event.clientX - rect.left) / rect.width;
        const py = (event.clientY - rect.top) / rect.height;
        // Face the pointer: the edge under the cursor comes toward the reader.
        tiltX = (0.5 - px) * 13;
        tiltY = (py - 0.5) * 11;
        // The picture slides the other way, which reads as real depth.
        imgX = (px - 0.5) * 16;
        imgY = (py - 0.5) * 12;
        glareX = px * 100;
        glareY = py * 100;
        if (!raf) raf = requestAnimationFrame(render);
      });

      card.addEventListener("pointerleave", () => {
        card.classList.remove("is-peeking");
        ["--rx", "--ry", "--img-x", "--img-y", "--mx", "--my"].forEach((name) =>
          card.style.removeProperty(name)
        );
      });
    });
  }

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

  /* ---------- Ambient fallback ---------- */
  // If the WebGL backdrop never arrives (old browser, blocked GPU, slow asset)
  // the CSS version paints the same room instead of leaving a flat page.
  const ambient = document.querySelector("[data-ambient]");
  if (ambient) {
    const syncAmbient = () => {
      ambient.classList.toggle("is-fallback", !ambient.querySelector("canvas"));
    };
    window.setTimeout(syncAmbient, 2600);
    window.addEventListener("load", () => window.setTimeout(syncAmbient, 1200), { once: true });
  }
})();
