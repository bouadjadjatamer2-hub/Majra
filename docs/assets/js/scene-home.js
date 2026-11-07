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

      return {
        update(t, dt, pointer) {
          root.rotation.y = t * 0.055 + pointer.x * 0.16;
          root.rotation.x = 0.24 + pointer.y * 0.08 + Math.sin(t * 0.18) * 0.02;
          flock.rotation.y = t * 0.02;
          dust.rotation.y = -t * 0.012;
          rings.rotation.y = t * 0.04;
          rings.rotation.x = Math.sin(t * 0.22) * 0.08;

          const a1 = t * 0.24;
          majra.group.position.set(Math.cos(a1) * 2.55, Math.sin(a1 * 1.3) * 0.55, Math.sin(a1) * 1.35);
          majra.shell.rotation.x = t * 0.5;
          majra.shell.rotation.y = t * 0.62;
          labelMajra.position.set(
            majra.group.position.x,
            majra.group.position.y - 1.02,
            majra.group.position.z
          );

          const a2 = -t * 0.31 + 2.1;
          droidcrate.group.position.set(
            Math.cos(a2) * 3.3,
            Math.sin(a2 * 1.1) * 0.75,
            Math.sin(a2) * 1.6
          );
          droidcrate.shell.rotation.x = -t * 0.7;
          droidcrate.shell.rotation.z = t * 0.5;
          labelDroid.position.set(
            droidcrate.group.position.x,
            droidcrate.group.position.y - 0.85,
            droidcrate.group.position.z
          );

          const pulse = 1 + Math.sin(t * 1.4) * 0.04;
          majra.group.scale.setScalar(pulse);
          droidcrate.group.scale.setScalar(2 - pulse);
        },
      };
    },
    { cameraZ: 10.5 }
  );
}
