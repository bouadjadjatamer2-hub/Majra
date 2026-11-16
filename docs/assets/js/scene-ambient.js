/* Ambient depth field — a fixed backdrop behind every page.
 *
 * Sparse dust and a few wireframe solids sit far behind the content. Each page
 * gets its own motif: two cores on the home page, three merging streams on
 * Majra, a device and a crate on DroidCrate, a shield and a lock on the
 * privacy page, and a broken orbit on 404. The camera drifts with the page
 * scroll and leans with the pointer, so sections feel like they move through
 * a real space instead of sliding over a flat sheet.
 *
 * Desktop only, and it stays silent for reduced motion.
 */
import { THREE, createStage } from "./three-utils.js";

const AMBER = 0xf2b46d;
const JADE = 0x54b39c;
const WHITE = 0xfff4e4;

const container = document.querySelector("[data-ambient]");
const page = (container && container.dataset.ambient) || "home";
// Phones get a lighter cut of the same backdrop: fewer motes, a smaller
// drawing buffer and a 30 fps cap, but the same motifs.
const small = window.matchMedia("(max-width: 900px)").matches;

const DUST = {
  home: { count: 1200, colors: [AMBER, JADE, WHITE], weights: [0.46, 0.3, 0.24] },
  majra: { count: 1100, colors: [JADE, AMBER, WHITE], weights: [0.5, 0.26, 0.24] },
  droidcrate: { count: 1000, colors: [JADE, WHITE, AMBER], weights: [0.48, 0.3, 0.22] },
  privacy: { count: 780, colors: [WHITE, AMBER, JADE], weights: [0.5, 0.3, 0.2] },
  "404": { count: 900, colors: [AMBER, WHITE, JADE], weights: [0.44, 0.34, 0.22] },
};

const wire = (geometry, color, opacity) =>
  new THREE.LineSegments(
    new THREE.EdgesGeometry(geometry),
    new THREE.LineBasicMaterial({ color, transparent: true, opacity, depthWrite: false })
  );

const ring = (radius, color, opacity, tube = 0.022) =>
  new THREE.Mesh(
    new THREE.TorusGeometry(radius, tube, 4, 72),
    new THREE.MeshBasicMaterial({ color, transparent: true, opacity, depthWrite: false })
  );

/* ---------- Dust ---------- */
function buildDust(key) {
  const config = DUST[key] || DUST.home;
  const count = Math.max(320, Math.round(config.count * (small ? 0.42 : 1)));
  const positions = new Float32Array(count * 3);
  const colors = new Float32Array(count * 3);
  const palette = config.colors.map((c) => new THREE.Color(c));
  const [w0, w1] = config.weights;

  for (let i = 0; i < count; i += 1) {
    let x = 0;
    let y = 0;
    let z = 0;

    if (key === "home") {
      // Three spiral arms, echoing the flock in the hero.
      const arm = i % 3;
      const t = Math.random();
      const angle = t * Math.PI * 3.4 + (arm / 3) * Math.PI * 2;
      const radius = 2 + t * 16;
      x = Math.cos(angle) * radius;
      y = (Math.random() - 0.5) * (2.5 + t * 8) + 1;
      z = -18 - t * 9 + Math.sin(angle) * 1.6;
    } else if (key === "majra") {
      // Three flat lanes of traffic heading for the same core.
      const lane = i % 3;
      x = (Math.random() - 0.5) * 46;
      y = [4.6, 0.4, -3.8][lane] + (Math.random() - 0.5) * 2.4;
      z = -13 - Math.random() * 12;
    } else if (key === "droidcrate") {
      // A loose lattice, like the inside of a crate.
      const step = 2.6;
      x = Math.round(((Math.random() - 0.5) * 44) / step) * step + (Math.random() - 0.5) * 0.6;
      y = Math.round(((Math.random() - 0.5) * 22) / step) * step + (Math.random() - 0.5) * 0.6;
      z = -8 - Math.random() * 20;
    } else if (key === "privacy") {
      // A quiet shell with no lanes: calm and even.
      const r = 12 + Math.random() * 14;
      const theta = Math.random() * Math.PI * 2;
      const phi = Math.acos(2 * Math.random() - 1);
      x = Math.sin(phi) * Math.cos(theta) * r;
      y = Math.cos(phi) * r * 0.55 + 1;
      z = -12 - Math.abs(Math.sin(phi) * Math.sin(theta)) * r;
    } else {
      x = (Math.random() - 0.5) * 48;
      y = (Math.random() - 0.5) * 26;
      z = -8 - Math.random() * 22;
    }

    positions[i * 3] = x;
    positions[i * 3 + 1] = y;
    positions[i * 3 + 2] = z;

    const pick = Math.random();
    const color = pick < w0 ? palette[0] : pick < w0 + w1 ? palette[1] : palette[2];
    const fade = 0.25 + Math.random() * 0.45;
    colors[i * 3] = color.r * fade;
    colors[i * 3 + 1] = color.g * fade;
    colors[i * 3 + 2] = color.b * fade;
  }

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));

  return new THREE.Points(
    geometry,
    new THREE.PointsMaterial({
      size: 0.16,
      vertexColors: true,
      transparent: true,
      opacity: 0.64,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
      sizeAttenuation: true,
    })
  );
}

/* ---------- Page motifs ---------- */
function buildMotif(key) {
  const group = new THREE.Group();
  const tickers = [];

  if (key === "majra") {
    const core = wire(new THREE.OctahedronGeometry(1.6, 1), AMBER, 0.26);
    core.position.set(10.8, 0.4, -13);
    const coreRing = ring(2.6, AMBER, 0.13);
    coreRing.rotation.set(1.2, 0.2, 0);
    core.add(coreRing);
    group.add(core);

    const lanes = [
      { y: 4.6, z: -12, color: AMBER, offset: 0 },
      { y: 0.4, z: -14, color: JADE, offset: 0.34 },
      { y: -3.8, z: -16, color: WHITE, offset: 0.68 },
    ];
    const packets = [];

    lanes.forEach((lane) => {
      const curve = new THREE.CatmullRomCurve3([
        new THREE.Vector3(-24, lane.y, lane.z),
        new THREE.Vector3(-12, lane.y + 0.4, lane.z - 1),
        new THREE.Vector3(-2, lane.y * 0.35 + 0.4, -13),
        new THREE.Vector3(9.6, 0.4, -13),
      ]);

      const tube = new THREE.Mesh(
        new THREE.TubeGeometry(curve, 64, 0.03, 4, false),
        new THREE.MeshBasicMaterial({
          color: lane.color,
          transparent: true,
          opacity: 0.11,
          depthWrite: false,
        })
      );
      group.add(tube);

      const packet = new THREE.Mesh(
        new THREE.OctahedronGeometry(0.24, 0),
        new THREE.MeshBasicMaterial({
          color: lane.color,
          transparent: true,
          opacity: 0.62,
          depthWrite: false,
        })
      );
      group.add(packet);
      packets.push({ curve, packet, offset: lane.offset });
    });

    tickers.push((t, p) => {
      core.rotation.y = t * 0.14;
      core.rotation.x = t * 0.05 + p * 0.4;
      core.position.y = 0.4 - p * 1.8;
      coreRing.rotation.z = t * 0.2;
      packets.forEach(({ curve, packet, offset }) => {
        const at = (offset + t * 0.06) % 1;
        curve.getPointAt(at, packet.position);
        packet.rotation.y = t * 1.4;
      });
    });
  } else if (key === "droidcrate") {
    const crate = wire(new THREE.BoxGeometry(3.6, 3.6, 3.6), JADE, 0.2);
    crate.position.set(-12.6, 2.8, -15);
    const crateRing = ring(3.5, JADE, 0.1);
    crateRing.rotation.set(1.35, 0.3, 0);
    crate.add(crateRing);
    group.add(crate);

    const device = wire(new THREE.BoxGeometry(1.5, 3.1, 0.3), WHITE, 0.22);
    device.position.set(11.8, 1.2, -12);
    device.rotation.set(0.12, -0.42, 0.05);
    const screen = new THREE.Mesh(
      new THREE.PlaneGeometry(1.2, 2.6),
      new THREE.MeshBasicMaterial({
        color: JADE,
        transparent: true,
        opacity: 0.07,
        side: THREE.DoubleSide,
        depthWrite: false,
      })
    );
    screen.position.z = 0.17;
    device.add(screen);
    const deviceRing = ring(2.4, WHITE, 0.1);
    deviceRing.rotation.set(1.2, 0.1, 0.2);
    device.add(deviceRing);
    group.add(device);

    const orbiters = [];
    for (let i = 0; i < 3; i += 1) {
      const cube = wire(
        new THREE.BoxGeometry(0.5, 0.5, 0.5),
        i === 1 ? AMBER : WHITE,
        0.2
      );
      group.add(cube);
      orbiters.push({
        cube,
        phase: (i / 3) * Math.PI * 2,
        radius: 5.4,
        height: i * 1.1 - 1.1,
      });
    }

    tickers.push((t, p) => {
      crate.rotation.y = t * 0.08 + p * 0.3;
      crate.rotation.x = t * 0.03;
      crate.position.y = 2.8 - p * 2.2;
      device.rotation.y = -0.42 + Math.sin(t * 0.35) * 0.16;
      device.rotation.z = 0.05 + Math.sin(t * 0.28 + 1) * 0.05;
      device.position.y = 1.2 + Math.sin(t * 0.4 + 0.6) * 0.2 - p * 2;
      deviceRing.rotation.z = t * 0.22;
      orbiters.forEach(({ cube, phase, radius, height }) => {
        const angle = t * 0.22 + phase;
        cube.position.set(
          -12.6 + Math.cos(angle) * radius,
          2.8 + height + Math.sin(t * 0.6 + phase) * 0.3 - p * 2.2,
          -15 + Math.sin(angle) * radius * 0.6
        );
        cube.rotation.y = t * 0.4;
        cube.rotation.x = t * 0.3;
      });
    });
  } else if (key === "privacy") {
    const shieldShape = new THREE.Shape();
    shieldShape.moveTo(0, 1.7);
    shieldShape.lineTo(1.2, 1.15);
    shieldShape.quadraticCurveTo(1.25, -0.7, 0, -1.85);
    shieldShape.quadraticCurveTo(-1.25, -0.7, -1.2, 1.15);
    shieldShape.closePath();

    const shield = wire(
      new THREE.ExtrudeGeometry(shieldShape, { depth: 0.22, bevelEnabled: false }),
      AMBER,
      0.22
    );
    shield.position.set(-11.6, 3.2, -13);
    shield.rotation.set(0.05, 0.38, 0);
    shield.scale.setScalar(1.9);
    group.add(shield);

    const lock = new THREE.Group();
    lock.position.set(11.8, -1.4, -12.5);
    lock.rotation.set(0.1, -0.3, 0);
    const body = wire(new THREE.BoxGeometry(2.1, 1.7, 0.7), WHITE, 0.22);
    lock.add(body);
    const shackle = new THREE.Mesh(
      new THREE.TorusGeometry(0.72, 0.055, 4, 40, Math.PI),
      new THREE.MeshBasicMaterial({
        color: WHITE,
        transparent: true,
        opacity: 0.24,
        depthWrite: false,
      })
    );
    shackle.position.y = 0.85;
    lock.add(shackle);
    const keyhole = ring(0.24, AMBER, 0.4, 0.03);
    keyhole.position.z = 0.37;
    lock.add(keyhole);
    group.add(lock);

    tickers.push((t, p) => {
      shield.rotation.y = 0.38 + Math.sin(t * 0.28) * 0.12;
      shield.position.y = 3.2 + Math.sin(t * 0.34 + 0.8) * 0.28 - p * 2.2;
      lock.rotation.y = -0.3 + Math.sin(t * 0.24 + 1.6) * 0.14;
      lock.position.y = -1.4 + Math.sin(t * 0.4 + 0.2) * 0.24 - p * 2;
      keyhole.rotation.z = t * 0.6;
    });
  } else if (key === "404") {
    const arc = new THREE.Mesh(
      new THREE.TorusGeometry(3.6, 0.045, 4, 64, Math.PI * 1.3),
      new THREE.MeshBasicMaterial({
        color: AMBER,
        transparent: true,
        opacity: 0.2,
        depthWrite: false,
      })
    );
    arc.position.set(-11.4, 2.4, -13);
    arc.rotation.set(0.9, 0.2, 0.4);
    group.add(arc);

    const spots = [
      [-11.4, 6.2, -12],
      [10.6, 4.4, -13],
      [12.4, -2.6, -11],
      [-13.2, -2.2, -14],
      [4.4, 8.4, -10],
      [-4.2, -5.4, -15],
    ];
    const shards = spots.map((spot, i) => {
      const shard = wire(
        new THREE.TetrahedronGeometry(0.55 + (i % 3) * 0.16, 0),
        i % 2 ? WHITE : AMBER,
        0.18
      );
      shard.position.set(spot[0], spot[1], spot[2]);
      group.add(shard);
      return shard;
    });

    tickers.push((t, p) => {
      arc.rotation.z = 0.4 + t * 0.12;
      arc.position.y = 2.4 - p * 2;
      shards.forEach((shard, i) => {
        shard.rotation.x = t * (0.16 + i * 0.03);
        shard.rotation.y = t * (0.22 + i * 0.02);
        shard.position.y = spots[i][1] + Math.sin(t * 0.5 + i * 1.3) * 0.4 - p * 2.4;
      });
    });
  } else {
    // Home: two glowing cores, one for each app, kept on the open right side
    // so the headline column stays clean.
    const amberCore = wire(new THREE.IcosahedronGeometry(2.5, 1), AMBER, 0.24);
    amberCore.position.set(14.2, -6.2, -16);
    const amberRing = ring(3.8, AMBER, 0.13);
    amberRing.rotation.set(1.1, 0.4, 0);
    amberCore.add(amberRing);
    group.add(amberCore);

    const jadeCore = wire(new THREE.OctahedronGeometry(1.9, 0), JADE, 0.22);
    jadeCore.position.set(18.2, 2.6, -14);
    const jadeRing = ring(2.9, JADE, 0.12);
    jadeRing.rotation.set(0.6, 0.9, 0.3);
    jadeCore.add(jadeRing);
    group.add(jadeCore);

    const whiteNode = wire(new THREE.DodecahedronGeometry(1.15, 0), WHITE, 0.15);
    whiteNode.position.set(12.6, 9, -9);
    group.add(whiteNode);

    tickers.push((t, p) => {
      amberCore.rotation.x = t * 0.06 + p * 0.5;
      amberCore.rotation.y = t * 0.09;
      amberCore.position.y = -6.2 + Math.sin(t * 0.4) * 0.35 - p * 2.2;
      jadeCore.rotation.x = -t * 0.07 + p * 0.4;
      jadeCore.rotation.y = t * 0.12;
      jadeCore.position.y = 2.6 + Math.sin(t * 0.33 + 1.4) * 0.3 - p * 2;
      whiteNode.rotation.y = t * 0.16;
      whiteNode.position.y = 9 + Math.sin(t * 0.5 + 2.2) * 0.4 - p * 2.4;
      amberRing.rotation.z = t * 0.2;
      jadeRing.rotation.z = -t * 0.25;
    });
  }

  return { group, tickers };
}

if (container) {
  createStage(
    container,
    ({ scene, camera }) => {
      const root = new THREE.Group();
      scene.add(root);

      const dust = buildDust(page);
      root.add(dust);

      const motif = buildMotif(page);
      root.add(motif.group);

      // Home, Majra and DroidCrate get a floor so the motifs stand in a room.
      let grid = null;
      if (page === "home" || page === "majra" || page === "droidcrate") {
        grid = new THREE.GridHelper(90, 45, 0x3c6f66, 0x223033);
        grid.material.transparent = true;
        grid.material.opacity = 0.18;
        grid.material.depthWrite = false;
        grid.position.set(0, -8, -16);
        root.add(grid);
      }

      // A portrait phone has a narrow field of view, so the whole world is
      // pulled in to keep the motif on screen.
      const fit = () => {
        const rect = container.getBoundingClientRect();
        const portrait = rect.height > rect.width;
        motif.group.scale.setScalar(portrait ? 0.66 : 1);
        dust.scale.setScalar(portrait ? 0.7 : 1);
      };
      fit();

      return {
        resize: fit,
        update(t, dt, pointer, scroll = 0) {
          const p = scroll;
          const portrait = camera.aspect < 1;

          // The camera drops and pushes forward as the page is read.
          camera.position.x = pointer.x * 1.6;
          camera.position.y = (portrait ? 1.4 : 2.6) - p * 10 - pointer.y * 1.1;
          camera.position.z = 23 * (portrait ? 1.34 : 1) - p * 4;
          camera.lookAt(0, 1.4 - p * 4.5, -11);

          dust.rotation.y = t * 0.013 + p * 0.55;
          dust.rotation.x = 0.05 + p * 0.12;
          dust.position.z = p * 6;

          motif.group.position.z = p * 3;
          motif.tickers.forEach((tick) => tick(t, p));

          if (grid) {
            grid.position.z = -16 + p * 9;
            grid.position.y = -8 - p * 1.8;
            grid.material.opacity = 0.1 + (1 - p) * 0.12;
          }
        },
      };
    },
    {
      cameraZ: 23,
      fov: 55,
      far: 140,
      scrollMode: "page",
      pointerTarget: window,
      dpr: small ? 1.15 : 1.5,
      maxFps: small ? 30 : 0,
    }
  );
}
