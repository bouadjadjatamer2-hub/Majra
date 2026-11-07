# Asraab website

This folder is published as a GitHub Pages project site at
<https://bouadjadjatamer2-hub.github.io/Majra/>.

- `index.html` — catalog and home
- `apps/majra/` — Majra download manager page
- `apps/droidcrate/` — DroidCrate Android workbench page
- `extension/privacy/` — browser extension privacy policy
- `assets/css/site.css` — shared design system
- `assets/js/` — language, navigation and Three.js scenes
- `assets/vendor/three.module.min.js` — vendored Three.js r160

The site is static. No build step, no server, no cookies. The English copy is in
the HTML; Arabic lives in `assets/js/i18n.js`. Three.js scenes pause when hidden,
when offscreen, and when `prefers-reduced-motion` is set.
