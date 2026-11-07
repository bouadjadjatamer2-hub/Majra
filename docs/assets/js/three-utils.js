/* Shared Three.js helpers for the Asraab scenes.
 * Kept small on purpose: one renderer per scene, one RAF loop, and a clean
 * pause path for hidden tabs, offscreen sections and reduced motion.
 */
import * as THREE from "../vendor/three.module.min.js";

export { THREE };

const REDUCED = () =>
  window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export function glowTexture() {
  const size = 128;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext("2d");
  const g = ctx.createRadialGradient(size / 2, size / 2, 0, size / 2, size / 2, size / 2);
  g.addColorStop(0, "rgba(255,255,255,1)");
  g.addColorStop(0.25, "rgba(255,255,255,0.85)");
  g.addColorStop(0.55, "rgba(255,255,255,0.28)");
  g.addColorStop(1, "rgba(255,255,255,0)");
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, size, size);
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

export function textSprite(text, options = {}) {
  const {
    color = "#f6f2ea",
    font = "600 34px 'Roboto Mono', monospace",
    padding = 26,
    opacity = 0.85,
    letterSpacing = 0,
  } = options;
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  ctx.font = font;
  const width = Math.ceil(ctx.measureText(text).width) + padding * 2;
  const height = 76;
  canvas.width = width;
  canvas.height = height;
  const c = canvas.getContext("2d");
  c.font = font;
  c.textAlign = "center";
  c.textBaseline = "middle";
  c.fillStyle = color;
  c.globalAlpha = opacity;
  if (letterSpacing) c.letterSpacing = `${letterSpacing}px`;
  c.fillText(text, width / 2, height / 2 + 1);
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.minFilter = THREE.LinearFilter;
  const material = new THREE.SpriteMaterial({
    map: tex,
    transparent: true,
    depthWrite: false,
    opacity,
  });
  const sprite = new THREE.Sprite(material);
  sprite.userData.aspect = width / height;
  sprite.userData.baseOpacity = opacity;
  return sprite;
}

export function addGlow(parent, texture, color, scale) {
  const material = new THREE.SpriteMaterial({
    map: texture,
    color,
    transparent: true,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
    opacity: 0.9,
  });
  const sprite = new THREE.Sprite(material);
  sprite.scale.set(scale, scale, 1);
  parent.add(sprite);
  return sprite;
}

/* Build a stage around a scene factory.
 * factory({ scene, camera, renderer, size }) -> { update(t, dt, pointer), dispose? }
 */
export function createStage(container, factory, options = {}) {
  if (!container) return null;

  let renderer;
  try {
    renderer = new THREE.WebGLRenderer({
      antialias: true,
      alpha: true,
      powerPreference: "high-performance",
    });
  } catch (error) {
    showFallback(container);
    return null;
  }
  if (!renderer.getContext()) {
    showFallback(container);
    return null;
  }

  const reduced = REDUCED();
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.setClearColor(0x000000, 0);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.domElement.setAttribute("aria-hidden", "true");
  container.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(42, 1, 0.1, 100);
  camera.position.set(0, 0, options.cameraZ || 9);
  camera.lookAt(0, 0, 0);

  const pointer = { x: 0, y: 0, tx: 0, ty: 0 };
  const size = { width: 1, height: 1 };
  let api = null;
  let raf = 0;
  let visible = true;
  let running = false;
  let last = 0;
  let elapsed = 0;

  const resize = () => {
    const rect = container.getBoundingClientRect();
    const width = Math.max(1, Math.round(rect.width));
    const height = Math.max(1, Math.round(rect.height));
    size.width = width;
    size.height = height;
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    // Pull the camera back on narrow portrait frames so the subject still fits.
    const base = options.cameraZ || 9;
    const fit = Math.max(1, 1.18 / Math.min(1.18, width / height));
    camera.position.z = base * (width < height ? Math.min(fit, 1.9) : 1);
    camera.updateProjectionMatrix();
    if (api && api.resize) api.resize(size);
    renderer.render(scene, camera);
  };

  api = factory({ scene, camera, renderer, size, reduced });

  const frame = (now) => {
    if (!running) return;
    const dt = Math.min(0.05, (now - last) / 1000 || 0.016);
    last = now;
    elapsed += dt;
    pointer.x += (pointer.tx - pointer.x) * 0.05;
    pointer.y += (pointer.ty - pointer.y) * 0.05;
    if (api && api.update) api.update(elapsed, dt, pointer);
    renderer.render(scene, camera);
    raf = requestAnimationFrame(frame);
  };

  const start = () => {
    if (running || reduced || !visible || document.hidden) return;
    running = true;
    last = performance.now();
    raf = requestAnimationFrame(frame);
  };
  const stop = () => {
    running = false;
    cancelAnimationFrame(raf);
  };

  const onPointer = (event) => {
    const rect = container.getBoundingClientRect();
    pointer.tx = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    pointer.ty = ((event.clientY - rect.top) / rect.height) * 2 - 1;
  };

  const onVisibility = () => (document.hidden ? stop() : start());

  resize();
  if (api && api.update) api.update(0, 0, pointer);
  renderer.render(scene, camera);

  // Keep the drawing buffer correct at every width, even in reduced-motion mode.
  const ro = new ResizeObserver(resize);
  ro.observe(container);

  if (!reduced) {
    container.addEventListener("pointermove", onPointer, { passive: true });
    container.addEventListener("pointerleave", () => {
      pointer.tx = 0;
      pointer.ty = 0;
    });
    document.addEventListener("visibilitychange", onVisibility);

    if ("IntersectionObserver" in window) {
      const io = new IntersectionObserver(
        (entries) => {
          visible = entries[0].isIntersecting;
          if (visible) start();
          else stop();
        },
        { threshold: 0.02 }
      );
      io.observe(container);
    } else {
      start();
    }

    // Stop rendering when the page is being unloaded.
    window.addEventListener("pagehide", stop, { once: true });
  }

  return {
    renderer,
    scene,
    camera,
    resize,
    dispose() {
      stop();
      if (api && api.dispose) api.dispose();
      renderer.dispose();
      renderer.domElement.remove();
    },
  };
}

function showFallback(container) {
  const frame = container.closest(".scene-frame") || container;
  if (frame.querySelector(".scene-fallback")) return;
  frame.classList.add("is-fallback");
  const fallback = document.createElement("div");
  fallback.className = "scene-fallback";
  const icon = document.createElement("img");
  icon.src = container.dataset.fallback || "";
  icon.alt = "";
  fallback.appendChild(icon);
  frame.appendChild(fallback);
}
