/* Home scene — a flock of light.
 * Three spiral arms of particles turn around two glowing app cores: amber for
 * Majra, jade for DroidCrate. The whole group leans toward the pointer.
 */
import { THREE, createStage, glowTexture, addGlow, textSprite } from "./three-utils.js";

const AMBER = 0xf2b46d;
const JADE = 0x54b39c;
const WHITE = 0xfff4e4;

const container = document.querySelector('[data-scene="home"]');

if (container) {
  createStage(
    container,
    ({ scene, camera }) => {
      const glow = glowTexture();

      const root = new THREE.Group();
      scene.add(root);

      // --- Spiral flock -------------------------------------------------
      const COUNT = 2400;
      const positions = new Float32Array(COUNT * 3);
      const colors = new Float32Array(COUNT * 3);
      const palette = [new THREE.Color(AMBER), new THREE.Color(JADE), new THREE.Color(WHITE)];
      const weights = [0.52, 0.24, 0.24];

      for (let i = 0; i < COUNT; i += 1) {
        const arm = i % 3;
        const r = 0.55 + Math.pow(Math.random(), 0.62) * 4.35;
        const angle = r * 1.05 + (arm * Math.PI * 2) / 3 + (Math.random() - 0.5) * 0.5;
        const lift = (Math.random() - 0.5) * (1.5 - r * 0.18);
        positions[i * 3] = Math.cos(angle) * r;
        positions[i * 3 + 1] = lift;
        positions[i * 3 + 2] = Math.sin(angle) * r * 0.72;

        const pick = Math.random();
        const color = pick < weights[0] ? palette[0] : pick < weights[0] + weights[1] ? palette[1] : palette[2];
        const fade = 0.45 + Math.random() * 0.55;
        colors[i * 3] = color.r * fade;
        colors[i * 3 + 1] = color.g * fade;
        colors[i * 3 + 2] = color.b * fade;
      }

      const flockGeo = new THREE.BufferGeometry();
      flockGeo.setAttribute("position", new THREE.BufferAttribute(positions, 3));
      flockGeo.setAttribute("color", new THREE.BufferAttribute(colors, 3));

      // The flock keeps a second arrangement: the studio wordmark, sampled
      // from a canvas the same way Antigravity samples an image. Hovering the
      // frame lets the spiral settle into the name; leaving lets it fly apart
      // again. Nothing is added to the copy, and reduced motion never runs it.
      const basePositions = positions.slice();
      let targets = null;
      let morph = 0;
      let goal = 0;
      let applied = -1;
      let hovering = false;
      let demoPlayed = false;
      const flock = new THREE.Points(
        flockGeo,
        new THREE.PointsMaterial({
          size: 0.075,
          map: glow,
          vertexColors: true,
          transparent: true,
          opacity: 0.95,
          depthWrite: false,
          blending: THREE.AdditiveBlending,
          sizeAttenuation: true,
        })
      );
      root.add(flock);
      flock.frustumCulled = false;

      const markText = () =>
        String(document.documentElement.lang || "en").slice(0, 2) === "ar" ? "أسراب" : "ASRAAB";

      // Rasterise the wordmark, read the pixels, and keep one point per lit
      // pixel. The result is scaled and centred so it fills the stage.
      const sampleMark = () => {
        const W = 1100;
        const H = 320;
        const canvas = document.createElement("canvas");
        canvas.width = W;
        canvas.height = H;
        const ctx = canvas.getContext("2d", { willReadFrequently: true });
        if (!ctx) return null;

        ctx.fillStyle = "#ffffff";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.font = "700 200px 'Space Grotesk', 'Manrope', 'IBM Plex Sans Arabic', sans-serif";
        try {
          ctx.letterSpacing = "18px";
        } catch (error) {}
        ctx.fillText(markText(), W / 2, H / 2);

        const data = ctx.getImageData(0, 0, W, H).data;
        const points = [];
        let minX = Infinity;
        let maxX = -Infinity;
        let minY = Infinity;
        let maxY = -Infinity;
        const step = 2;
        for (let y = 0; y < H; y += step) {
          for (let x = 0; x < W; x += step) {
            if (data[(y * W + x) * 4 + 3] < 140) continue;
            points.push(x, y);
            if (x < minX) minX = x;
            if (x > maxX) maxX = x;
            if (y < minY) minY = y;
            if (y > maxY) maxY = y;
          }
        }
        const total = points.length / 2;
        if (total < 400) return null;

        const scale = 5.6 / Math.max(1, maxX - minX);
        const cx = (minX + maxX) / 2;
        const cy = (minY + maxY) / 2;

        // Shuffle once, then hand each particle its own lit pixel: no clumps
        // and no two particles stacked on the same spot.
        const order = new Int32Array(total);
        for (let i = 0; i < total; i += 1) order[i] = i;
        for (let i = total - 1; i > 0; i -= 1) {
          const j = (Math.random() * (i + 1)) | 0;
          const swap = order[i];
          order[i] = order[j];
          order[j] = swap;
        }

        const out = new Float32Array(COUNT * 3);
        for (let i = 0; i < COUNT; i += 1) {
          const p = order[i % total] * 2;
          out[i * 3] = (points[p] - cx) * scale + (Math.random() - 0.5) * 0.016;
          out[i * 3 + 1] = (cy - points[p + 1]) * scale + (Math.random() - 0.5) * 0.016;
          out[i * 3 + 2] = (Math.random() - 0.5) * 0.5;
        }
        return out;
      };

      const buildMark = () => {
        targets = sampleMark();
        applied = -1;
      };
      if (document.fonts && document.fonts.ready) {
        document.fonts.ready.then(buildMark).catch(() => {});
      } else {
        buildMark();
      }
      // Belt and braces: if the font promise never settles, sample anyway.
      window.setTimeout(() => {
        if (!targets) buildMark();
      }, 1800);

      // A sparse outer dust layer gives the frame depth.
      const dustCount = 500;
      const dustPos = new Float32Array(dustCount * 3);
      for (let i = 0; i < dustCount; i += 1) {
        const r = 5.2 + Math.random() * 3.6;
        const a = Math.random() * Math.PI * 2;
        dustPos[i * 3] = Math.cos(a) * r;
        dustPos[i * 3 + 1] = (Math.random() - 0.5) * 6;
        dustPos[i * 3 + 2] = Math.sin(a) * r * 0.6 - 1;
      }
      const dustGeo = new THREE.BufferGeometry();
      dustGeo.setAttribute("position", new THREE.BufferAttribute(dustPos, 3));
      const dust = new THREE.Points(
        dustGeo,
        new THREE.PointsMaterial({
          size: 0.045,
          map: glow,
          color: 0xb9c4d6,
          transparent: true,
          opacity: 0.38,
          depthWrite: false,
          blending: THREE.AdditiveBlending,
        })
      );
      root.add(dust);

      // --- Orbit rings --------------------------------------------------
      const rings = new THREE.Group();
      [
        { r: 3.15, tilt: 0.5, color: AMBER, opacity: 0.16 },
        { r: 4.15, tilt: -0.7, color: JADE, opacity: 0.12 },
      ].forEach((spec) => {
        const ring = new THREE.Mesh(
          new THREE.TorusGeometry(spec.r, 0.006, 6, 180),
          new THREE.MeshBasicMaterial({
            color: spec.color,
            transparent: true,
            opacity: spec.opacity,
          })
        );
        ring.rotation.x = spec.tilt;
        ring.rotation.z = spec.tilt * 0.4;
        rings.add(ring);
      });
      root.add(rings);

      // --- App cores ----------------------------------------------------
      const makeCore = (color, radius, detail) => {
        const group = new THREE.Group();
        const shell = new THREE.Mesh(
          new THREE.IcosahedronGeometry(radius, detail),
          new THREE.MeshBasicMaterial({
            color,
            wireframe: true,
            transparent: true,
            opacity: 0.72,
          })
        );
        group.add(shell);
        addGlow(group, glow, color, radius * 5.4);
        return { group, shell };
      };

      const majra = makeCore(AMBER, 0.62, 1);
      const droidcrate = makeCore(JADE, 0.4, 1);
      root.add(majra.group, droidcrate.group);

      const labelMajra = textSprite("MAJRA", {
        color: "#f8d3a4",
        font: "600 30px 'Roboto Mono', monospace",
        opacity: 0.9,
        letterSpacing: 4,
      });
      const labelDroid = textSprite("DROIDCRATE", {
        color: "#b7f0e2",
        font: "600 30px 'Roboto Mono', monospace",
        opacity: 0.9,
        letterSpacing: 3,
      });
      [labelMajra, labelDroid].forEach((label) => {
        label.scale.set(label.userData.aspect * 0.42, 0.42, 1);
        root.add(label);
      });

      // Everything except the flock dims while the name is being spelled, so
      // the wordmark reads on its own.
      const decorMats = [];
      root.traverse((obj) => {
        const material = obj.material;
        if (!material || obj === flock || typeof material.opacity !== "number") return;
        material.userData.baseOpacity = material.opacity;
        decorMats.push(material);
      });

      const wake = () => {
        hovering = true;
        goal = 1;
      };
      const rest = () => {
        hovering = false;
        goal = 0;
      };
      container.addEventListener("pointerenter", wake);
      container.addEventListener("pointerleave", rest);
      container.addEventListener("pointerdown", wake);
      container.addEventListener("pointerup", rest);
      container.addEventListener("pointercancel", rest);

      // Touch screens have no hover, so the name is spelled once on its own
      // shortly after the frame is actually on screen.
      if ("IntersectionObserver" in window) {
        const io = new IntersectionObserver(
          (entries) => {
            if (!entries[0].isIntersecting) return;
            io.disconnect();
            window.setTimeout(() => {
              if (demoPlayed || hovering) return;
              demoPlayed = true;
              goal = 1;
              window.setTimeout(() => {
                if (!hovering) goal = 0;
              }, 3400);
            }, 3800);
          },
          { threshold: 0.35 }
        );
        io.observe(container);
      }

      document.addEventListener("asraab:lang", buildMark);

      return {
        update(t, dt, pointer, scroll = 0) {
          const depth = scroll - 0.5;

          // Ease the second arrangement in and out; it never snaps.
          if (morph !== goal) {
            morph += (goal - morph) * Math.min(1, dt * 3.2);
            if (Math.abs(goal - morph) < 0.0015) morph = goal;
          }
          const ease = morph * morph * (3 - 2 * morph);
          const face = 1 - ease;

          root.position.z = -depth * 1.8 + ease * 0.4;
          root.position.y = depth * 0.25;
          root.rotation.y =
            (t * 0.055 + pointer.x * 0.16 + depth * 0.55) * face + pointer.x * 0.05 * ease;
          root.rotation.x =
            (0.24 + pointer.y * 0.08 + Math.sin(t * 0.18) * 0.02 + depth * 0.16) * face +
            0.04 * ease;
          flock.rotation.y = t * 0.02 * face;
          dust.rotation.y = -t * 0.012;
          rings.rotation.y = t * 0.04;
          rings.rotation.x = Math.sin(t * 0.22) * 0.08;

          const a1 = t * 0.24;
          const spread = 1 + depth * 0.24;
          majra.group.position.set(
            Math.cos(a1) * 2.55 * spread,
            Math.sin(a1 * 1.3) * 0.55,
            Math.sin(a1) * 1.35 * spread
          );
          majra.shell.rotation.x = t * 0.5;
          majra.shell.rotation.y = t * 0.62;
          labelMajra.position.set(
            majra.group.position.x,
            majra.group.position.y - 1.02,
            majra.group.position.z
          );

          const a2 = -t * 0.31 + 2.1;
          droidcrate.group.position.set(
            Math.cos(a2) * 3.3 * spread,
            Math.sin(a2 * 1.1) * 0.75,
            Math.sin(a2) * 1.6 * spread
          );
          droidcrate.shell.rotation.x = -t * 0.7;
          droidcrate.shell.rotation.z = t * 0.5;
          labelDroid.position.set(
            droidcrate.group.position.x,
            droidcrate.group.position.y - 0.85,
            droidcrate.group.position.z
          );

          // Write the blended positions only while something is moving.
          if (targets && applied !== morph) {
            const pos = flockGeo.attributes.position.array;
            for (let i = 0; i < COUNT; i += 1) {
              const i3 = i * 3;
              pos[i3] = basePositions[i3] + (targets[i3] - basePositions[i3]) * ease;
              pos[i3 + 1] = basePositions[i3 + 1] + (targets[i3 + 1] - basePositions[i3 + 1]) * ease;
              pos[i3 + 2] = basePositions[i3 + 2] + (targets[i3 + 2] - basePositions[i3 + 2]) * ease;
            }
            flockGeo.attributes.position.needsUpdate = true;
            flock.material.size = 0.075 + ease * 0.04;
            flock.material.opacity = 0.95 + ease * 0.05;
            for (let m = 0; m < decorMats.length; m += 1) {
              decorMats[m].opacity = decorMats[m].userData.baseOpacity * face;
            }
            applied = morph;
          }

          const pulse = 1 + Math.sin(t * 1.4) * 0.04;
          majra.group.scale.setScalar(pulse);
          droidcrate.group.scale.setScalar(2 - pulse);
        },
      };
    },
    { cameraZ: 10.5 }
  );
}
