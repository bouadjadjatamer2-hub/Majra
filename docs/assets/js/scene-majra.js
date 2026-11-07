/* Majra scene — three networks, one file.
 * Glowing blocks travel along Wi-Fi, USB and Ethernet curves into a rotating
 * core. Every arrival pulses the core and pushes a ring outward.
 */
import { THREE, createStage, glowTexture, addGlow, textSprite } from "./three-utils.js";

const container = document.querySelector('[data-scene="majra"]');

if (container) {
  createStage(
    container,
    ({ scene, camera }) => {
      const glow = glowTexture();
      const root = new THREE.Group();
      root.add(new THREE.Group());
      scene.add(root);

      camera.position.set(0, 0.9, 10.6);
      camera.lookAt(0, 0, 0);

      // --- Streams ------------------------------------------------------
      const streams = [
        {
          name: "WI-FI",
          color: 0xf2b46d,
          start: new THREE.Vector3(-3.55, 1.85, 0.4),
          control: new THREE.Vector3(-2.4, 1.55, 0.1),
          mid: new THREE.Vector3(-1.15, 0.5, 0),
        },
        {
          name: "USB",
          color: 0x54b39c,
          start: new THREE.Vector3(-3.55, 0, -0.7),
          control: new THREE.Vector3(-2.3, 0.05, -0.2),
          mid: new THREE.Vector3(-1.1, 0.05, 0),
        },
        {
          name: "ETHERNET",
          color: 0xd9d3c6,
          start: new THREE.Vector3(-3.55, -1.85, 0.4),
          control: new THREE.Vector3(-2.4, -1.55, 0.1),
          mid: new THREE.Vector3(-1.15, -0.5, 0),
        },
      ];

      const packetGroups = [];
      const arrivals = [];

      streams.forEach((stream) => {
        const curve = new THREE.CatmullRomCurve3([
          stream.start,
          stream.control,
          stream.mid,
          new THREE.Vector3(0, 0, 0),
        ]);
        stream.curve = curve;

        const tube = new THREE.Mesh(
          new THREE.TubeGeometry(curve, 90, 0.011, 6, false),
          new THREE.MeshBasicMaterial({
            color: stream.color,
            transparent: true,
            opacity: 0.22,
          })
        );
        root.add(tube);

        // The label sits just outside the stream entry point.
        const label = textSprite(stream.name, {
          color: `#${stream.color.toString(16).padStart(6, "0")}`,
          font: "600 27px 'Roboto Mono', monospace",
          opacity: 0.8,
          letterSpacing: 2,
        });
        label.scale.set(label.userData.aspect * 0.34, 0.34, 1);
        label.position.copy(stream.start).add(new THREE.Vector3(-0.05, 0.44, 0));
        root.add(label);

        // A soft entry node marks where the network enters the frame.
        const node = new THREE.Group();
        node.position.copy(stream.start);
        addGlow(node, glow, stream.color, 0.7);
        root.add(node);

        const packetCount = 5;
        for (let i = 0; i < packetCount; i += 1) {
          const group = new THREE.Group();
          const cube = new THREE.Mesh(
            new THREE.BoxGeometry(0.115, 0.115, 0.115),
            new THREE.MeshBasicMaterial({ color: stream.color })
          );
          group.add(cube);
          addGlow(group, glow, stream.color, 0.42);
          group.userData = {
            curve,
            offset: i / packetCount + Math.random() * 0.06,
            speed: 0.14 + Math.random() * 0.05,
            lastT: 0,
            color: stream.color,
          };
          root.add(group);
          packetGroups.push(group);
        }
      });

      // --- Core ---------------------------------------------------------
      const core = new THREE.Group();
      root.add(core);

      const shell = new THREE.Mesh(
        new THREE.OctahedronGeometry(0.95, 1),
        new THREE.MeshBasicMaterial({
          color: 0xf2b46d,
          wireframe: true,
          transparent: true,
          opacity: 0.55,
        })
      );
      core.add(shell);

      const inner = new THREE.Mesh(
        new THREE.IcosahedronGeometry(0.42, 1),
        new THREE.MeshBasicMaterial({
          color: 0xfff4e4,
          wireframe: true,
          transparent: true,
          opacity: 0.75,
        })
      );
      core.add(inner);
      const coreGlow = addGlow(core, glow, 0xf2b46d, 3.4);

      const orbitRings = new THREE.Group();
      [
        { r: 1.35, x: 0.2, y: 0.1, z: 0.2, color: 0xf2b46d },
        { r: 1.65, x: 1.25, y: 0.35, z: 0, color: 0x54b39c },
        { r: 1.15, x: -0.9, y: 0.6, z: 0.4, color: 0xd9d3c6 },
      ].forEach((spec) => {
        const ring = new THREE.Mesh(
          new THREE.TorusGeometry(spec.r, 0.007, 6, 140),
          new THREE.MeshBasicMaterial({
            color: spec.color,
            transparent: true,
            opacity: 0.35,
          })
        );
        ring.rotation.set(spec.x, spec.y, spec.z);
        orbitRings.add(ring);
      });
      core.add(orbitRings);

      // Expanding rings fired when a block lands.
      const pulseMaterial = () =>
        new THREE.MeshBasicMaterial({
          color: 0xf2b46d,
          transparent: true,
          opacity: 0,
        });
      const pulses = [];
      for (let i = 0; i < 4; i += 1) {
        const mesh = new THREE.Mesh(new THREE.TorusGeometry(1.05, 0.012, 6, 90), pulseMaterial());
        mesh.userData.life = 0;
        core.add(mesh);
        pulses.push(mesh);
      }

      // --- Floor grid ---------------------------------------------------
      const grid = new THREE.GridHelper(16, 32, 0x3a3f4b, 0x1c2029);
      grid.position.y = -3.1;
      grid.material.transparent = true;
      grid.material.opacity = 0.18;
      scene.add(grid);

      let energy = 0;
      let pulseCursor = 0;

      return {
        update(t, dt, pointer) {
          root.rotation.y = pointer.x * 0.18;
          root.rotation.x = pointer.y * 0.06;
          root.position.y = Math.sin(t * 0.4) * 0.06;

          // Packets.
          packetGroups.forEach((packet) => {
            const data = packet.userData;
            const next = (data.offset + t * data.speed) % 1.15;
            const clamped = Math.min(next, 1);
            packet.position.copy(data.curve.getPointAt(clamped));
            packet.rotation.x = t * 1.6;
            packet.rotation.y = t * 1.2;
            const pop = next > 1 ? 0 : 0.7 + Math.sin(clamped * Math.PI) * 0.5;
            packet.scale.setScalar(pop);

            if (data.lastT < 1 && next >= 1) {
              energy = 1;
              const pulse = pulses[pulseCursor % pulses.length];
              pulseCursor += 1;
              pulse.userData.life = 1;
              pulse.rotation.set(Math.random() * 2, Math.random() * 2, 0);
            }
            data.lastT = next;
          });

          // Core.
          energy = Math.max(0, energy - dt * 1.6);
          const beat = 1 + energy * 0.16 + Math.sin(t * 1.8) * 0.025;
          core.scale.setScalar(beat);
          coreGlow.scale.setScalar(3.4 + energy * 1.2);
          coreGlow.material.opacity = 0.34 + energy * 0.3;
          shell.rotation.y = t * 0.35;
          shell.rotation.x = t * 0.18;
          inner.rotation.y = -t * 0.6;
          inner.rotation.z = t * 0.4;
          orbitRings.rotation.y = t * 0.25;
          orbitRings.rotation.z = t * 0.1;

          pulses.forEach((pulse) => {
            if (pulse.userData.life > 0) {
              pulse.userData.life = Math.max(0, pulse.userData.life - dt * 1.1);
              const life = pulse.userData.life;
              const scale = 1 + (1 - life) * 2.4;
              pulse.scale.setScalar(scale);
              pulse.material.opacity = life * 0.5;
            } else {
              pulse.material.opacity = 0;
            }
          });

          grid.material.opacity = 0.16 + energy * 0.12;
        },
      };
    },
    { cameraZ: 10.6 }
  );
}
