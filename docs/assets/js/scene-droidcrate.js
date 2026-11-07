/* DroidCrate scene — a phone you can read.
 * A rounded 3D handset floats inside two orbit rings. Its screen is a live
 * canvas texture: device card, six tabs, a transfer in progress and logcat.
 */
import { THREE, createStage, glowTexture, addGlow, textSprite } from "./three-utils.js";

const container = document.querySelector('[data-scene="droidcrate"]');
const JADE = 0x54b39c;
const AMBER = 0xf2b46d;

if (container) {
  createStage(
    container,
    ({ scene, camera }) => {
      const glow = glowTexture();

      scene.add(new THREE.AmbientLight(0x8fa2b8, 0.9));
      const key = new THREE.PointLight(0xf2b46d, 55, 24);
      key.position.set(-4, 5, 5);
      scene.add(key);
      const rim = new THREE.PointLight(0x54b39c, 45, 24);
      rim.position.set(5, -3, 4);
      scene.add(rim);

      camera.position.set(0, 0.35, 9);
      camera.lookAt(0, 0, 0);

      const root = new THREE.Group();
      root.position.y = 0.1;
      scene.add(root);

      // --- Phone body ---------------------------------------------------
      const phone = new THREE.Group();
      phone.scale.setScalar(1.1);
      root.add(phone);

      const W = 2.05;
      const H = 4.18;
      const shape = new THREE.Shape();
      const r = 0.3;
      shape.moveTo(-W / 2 + r, -H / 2);
      shape.lineTo(W / 2 - r, -H / 2);
      shape.quadraticCurveTo(W / 2, -H / 2, W / 2, -H / 2 + r);
      shape.lineTo(W / 2, H / 2 - r);
      shape.quadraticCurveTo(W / 2, H / 2, W / 2 - r, H / 2);
      shape.lineTo(-W / 2 + r, H / 2);
      shape.quadraticCurveTo(-W / 2, H / 2, -W / 2, H / 2 - r);
      shape.lineTo(-W / 2, -H / 2 + r);
      shape.quadraticCurveTo(-W / 2, -H / 2, -W / 2 + r, -H / 2);

      const bodyGeo = new THREE.ExtrudeGeometry(shape, {
        depth: 0.24,
        bevelEnabled: true,
        bevelSize: 0.04,
        bevelThickness: 0.04,
        bevelSegments: 3,
        curveSegments: 14,
      });
      bodyGeo.center();
      const body = new THREE.Mesh(
        bodyGeo,
        new THREE.MeshStandardMaterial({
          color: 0x11151d,
          metalness: 0.86,
          roughness: 0.32,
        })
      );
      phone.add(body);

      const edge = new THREE.LineSegments(
        new THREE.EdgesGeometry(bodyGeo, 20),
        new THREE.LineBasicMaterial({ color: 0x4a5568, transparent: true, opacity: 0.5 })
      );
      phone.add(edge);

      // --- Live screen --------------------------------------------------
      const canvas = document.createElement("canvas");
      canvas.width = 512;
      canvas.height = 1024;
      const ctx = canvas.getContext("2d");
      const texture = new THREE.CanvasTexture(canvas);
      texture.colorSpace = THREE.SRGBColorSpace;
      texture.minFilter = THREE.LinearFilter;
      texture.magFilter = THREE.LinearFilter;
      texture.generateMipmaps = false;

      const screen = new THREE.Mesh(
        new THREE.PlaneGeometry(W - 0.2, H - 0.2),
        new THREE.MeshBasicMaterial({ map: texture })
      );
      screen.position.z = 0.205;
      phone.add(screen);

      // Notch and side buttons.
      const notch = new THREE.Mesh(
        new THREE.BoxGeometry(0.52, 0.09, 0.03),
        new THREE.MeshStandardMaterial({ color: 0x05070a, metalness: 0.4, roughness: 0.5 })
      );
      notch.position.set(0, H / 2 - 0.32, 0.23);
      phone.add(notch);

      const buttonMat = new THREE.MeshStandardMaterial({
        color: 0x2b323d,
        metalness: 0.8,
        roughness: 0.4,
      });
      const power = new THREE.Mesh(new THREE.BoxGeometry(0.045, 0.42, 0.1), buttonMat);
      power.position.set(W / 2 + 0.01, 0.55, 0);
      phone.add(power);
      const volume = new THREE.Mesh(new THREE.BoxGeometry(0.045, 0.62, 0.1), buttonMat);
      volume.position.set(-W / 2 - 0.01, 0.7, 0);
      phone.add(volume);

      // --- Atmosphere ---------------------------------------------------
      const ringGroup = new THREE.Group();
      const ringSpecs = [
        { radius: 2.75, tilt: 1.16, color: JADE, opacity: 0.34, speed: 0.16 },
        { radius: 3.25, tilt: 1.42, color: AMBER, opacity: 0.2, speed: -0.1 },
      ];
      ringSpecs.forEach((spec) => {
        const ring = new THREE.Mesh(
          new THREE.TorusGeometry(spec.radius, 0.008, 6, 180),
          new THREE.MeshBasicMaterial({
            color: spec.color,
            transparent: true,
            opacity: spec.opacity,
          })
        );
        ring.rotation.x = spec.tilt;
        ring.userData.speed = spec.speed;
        ringGroup.add(ring);
      });
      root.add(ringGroup);

      // Floating labels around the handset.
      const labels = [
        { text: "ADB", x: -2.35, y: 1.75, color: "#b7f0e2" },
        { text: "FASTBOOT", x: 2.4, y: 0.55, color: "#f8d3a4" },
        { text: "APK", x: -2.3, y: -1.55, color: "#f8d3a4" },
        { text: "LOGCAT", x: 2.3, y: -2.05, color: "#b7f0e2" },
      ];
      labels.forEach((item) => {
        const sprite = textSprite(item.text, {
          color: item.color,
          font: "600 26px 'Roboto Mono', monospace",
          opacity: 0.78,
          letterSpacing: 3,
        });
        sprite.scale.set(sprite.userData.aspect * 0.34, 0.34, 1);
        sprite.position.set(item.x, item.y, 0.2);
        root.add(sprite);
      });

      // Orbiting dust.
      const dustCount = 150;
      const dustPos = new Float32Array(dustCount * 3);
      const dustBase = [];
      for (let i = 0; i < dustCount; i += 1) {
        const radius = 2.2 + Math.random() * 1.9;
        const angle = Math.random() * Math.PI * 2;
        const y = (Math.random() - 0.5) * 5.2;
        dustBase.push({ radius, angle, y, speed: 0.06 + Math.random() * 0.12 });
        dustPos[i * 3] = Math.cos(angle) * radius;
        dustPos[i * 3 + 1] = y;
        dustPos[i * 3 + 2] = Math.sin(angle) * radius;
      }
      const dustGeo = new THREE.BufferGeometry();
      dustGeo.setAttribute("position", new THREE.BufferAttribute(dustPos, 3));
      const dust = new THREE.Points(
        dustGeo,
        new THREE.PointsMaterial({
          size: 0.05,
          map: glow,
          color: 0x9fd8ca,
          transparent: true,
          opacity: 0.5,
          depthWrite: false,
          blending: THREE.AdditiveBlending,
        })
      );
      root.add(dust);

      const halo = addGlow(root, glow, JADE, 6.2);
      halo.position.z = -1.6;
      halo.material.opacity = 0.16;

      // --- Screen painter ----------------------------------------------
      const paint = (time) => {
        const w = canvas.width;
        const h = canvas.height;
        const progress = 34 + Math.sin(time * 0.55) * 26;
        const cycle = Math.floor(time / 1.4);

        ctx.clearRect(0, 0, w, h);

        const bg = ctx.createLinearGradient(0, 0, 0, h);
        bg.addColorStop(0, "#0a0f15");
        bg.addColorStop(0.55, "#0b1118");
        bg.addColorStop(1, "#080c11");
        ctx.fillStyle = bg;
        ctx.fillRect(0, 0, w, h);

        // Soft aurora at the top.
        const aurora = ctx.createRadialGradient(w * 0.75, 0, 0, w * 0.75, 0, 340);
        aurora.addColorStop(0, "rgba(84,179,156,0.18)");
        aurora.addColorStop(1, "rgba(84,179,156,0)");
        ctx.fillStyle = aurora;
        ctx.fillRect(0, 0, w, 400);

        const sans = "Manrope, system-ui, sans-serif";
        const mono = "'Roboto Mono', monospace";

        // Status bar.
        ctx.fillStyle = "rgba(246,242,234,0.72)";
        ctx.font = `600 15px ${sans}`;
        ctx.textAlign = "left";
        ctx.fillText("9:41", 30, 36);
        ctx.textAlign = "right";
        ctx.fillText("5G", 442, 36);
        ctx.fillStyle = "rgba(246,242,234,0.5)";
        ctx.fillRect(456, 26, 22, 11);
        ctx.fillStyle = "#54b39c";
        ctx.fillRect(458, 28, 14, 7);

        // Header.
        ctx.textAlign = "left";
        ctx.fillStyle = "#111820";
        roundRect(ctx, 30, 64, 46, 46, 12);
        ctx.fill();
        ctx.strokeStyle = "rgba(84,179,156,0.55)";
        ctx.lineWidth = 2;
        roundRect(ctx, 30, 64, 46, 46, 12);
        ctx.stroke();
        ctx.strokeStyle = "#54b39c";
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.moveTo(41, 88);
        ctx.lineTo(52, 78);
        ctx.lineTo(64, 88);
        ctx.stroke();

        ctx.fillStyle = "#f6f2ea";
        ctx.font = `700 25px ${sans}`;
        ctx.fillText("DroidCrate", 92, 88);
        ctx.fillStyle = "rgba(160,156,147,0.95)";
        ctx.font = `500 14px ${sans}`;
        ctx.fillText("Android workbench", 92, 108);

        // ADB pill.
        ctx.fillStyle = "rgba(84,179,156,0.14)";
        roundRect(ctx, 390, 68, 92, 34, 17);
        ctx.fill();
        ctx.fillStyle = "#54b39c";
        ctx.beginPath();
        ctx.arc(410, 85, 5, 0, Math.PI * 2);
        ctx.fill();
        ctx.font = `700 13px ${mono}`;
        ctx.fillText("ADB", 424, 90);

        // Device card.
        ctx.fillStyle = "rgba(255,255,255,0.045)";
        roundRect(ctx, 26, 134, 460, 152, 20);
        ctx.fill();
        ctx.strokeStyle = "rgba(255,255,255,0.08)";
        ctx.lineWidth = 1;
        roundRect(ctx, 26, 134, 460, 152, 20);
        ctx.stroke();

        ctx.fillStyle = "rgba(160,156,147,0.9)";
        ctx.font = `700 11px ${mono}`;
        ctx.fillText("CONNECTED DEVICE", 48, 164);

        ctx.fillStyle = "#f6f2ea";
        ctx.font = `700 32px ${sans}`;
        ctx.fillText("Pixel · Android 14", 48, 204);

        ctx.fillStyle = "rgba(160,156,147,0.95)";
        ctx.font = `500 14px ${sans}`;
        ctx.fillText("arm64-v8a · Platform Tools 37.0.1", 48, 228);

        const chips = [
          { text: "ADB ready", color: "#54b39c", x: 48 },
          { text: "Bootloader locked", color: "#f2b46d", x: 158 },
        ];
        chips.forEach((chip) => {
          ctx.fillStyle = "rgba(255,255,255,0.05)";
          roundRect(ctx, chip.x, 244, chip.text.length * 7.6 + 24, 26, 13);
          ctx.fill();
          ctx.fillStyle = chip.color;
          ctx.beginPath();
          ctx.arc(chip.x + 13, 257, 4, 0, Math.PI * 2);
          ctx.fill();
          ctx.font = `600 12px ${sans}`;
          ctx.fillText(chip.text, chip.x + 23, 261);
        });

        // Tabs.
        const tabs = ["Overview", "Apps", "Files", "Screen", "Flash", "Terminal"];
        let tx = 30;
        tabs.forEach((tab, i) => {
          ctx.font = i === 0 ? `700 14px ${sans}` : `500 14px ${sans}`;
          const width = ctx.measureText(tab).width;
          if (i === 0) {
            ctx.fillStyle = "rgba(84,179,156,0.15)";
            roundRect(ctx, tx - 8, 310, width + 16, 30, 15);
            ctx.fill();
            ctx.fillStyle = "#8fe0cd";
          } else {
            ctx.fillStyle = "rgba(160,156,147,0.9)";
          }
          ctx.fillText(tab, tx, 330);
          tx += width + 22;
        });

        // Action cards.
        const actions = [
          { label: "Restart", glyph: "restart" },
          { label: "Screenshot", glyph: "screen" },
          { label: "Backup", glyph: "folder" },
          { label: "Install APK", glyph: "box" },
          { label: "Logcat", glyph: "terminal" },
          { label: "Fastboot", glyph: "bolt" },
        ];
        actions.forEach((action, i) => {
          const col = i % 2;
          const row = Math.floor(i / 2);
          const x = 26 + col * 236;
          const y = 358 + row * 92;
          ctx.fillStyle = "rgba(255,255,255,0.038)";
          roundRect(ctx, x, y, 224, 80, 16);
          ctx.fill();
          ctx.strokeStyle = "rgba(255,255,255,0.06)";
          roundRect(ctx, x, y, 224, 80, 16);
          ctx.stroke();

          ctx.strokeStyle = i === 1 ? "#54b39c" : "rgba(246,242,234,0.75)";
          ctx.lineWidth = 2;
          drawGlyph(ctx, action.glyph, x + 26, y + 40, 13);

          ctx.fillStyle = "#f6f2ea";
          ctx.font = `600 15px ${sans}`;
          ctx.fillText(action.label, x + 50, y + 38);
          ctx.fillStyle = "rgba(160,156,147,0.9)";
          ctx.font = `500 11px ${mono}`;
          ctx.fillText(action.glyph === "bolt" ? "fastboot" : "adb", x + 50, y + 56);
        });

        // Transfer card.
        ctx.fillStyle = "rgba(84,179,156,0.07)";
        roundRect(ctx, 26, 646, 460, 116, 18);
        ctx.fill();
        ctx.strokeStyle = "rgba(84,179,156,0.2)";
        roundRect(ctx, 26, 646, 460, 116, 18);
        ctx.stroke();

        ctx.fillStyle = "#f6f2ea";
        ctx.font = `600 15px ${sans}`;
        ctx.fillText("Pulling /sdcard/DCIM", 46, 680);
        ctx.textAlign = "right";
        ctx.fillStyle = "#8fe0cd";
        ctx.font = `700 14px ${mono}`;
        ctx.fillText(`${Math.round(progress)}%`, 466, 680);
        ctx.textAlign = "left";

        ctx.fillStyle = "rgba(255,255,255,0.08)";
        roundRect(ctx, 46, 698, 420, 10, 5);
        ctx.fill();
        const bar = ctx.createLinearGradient(46, 0, 466, 0);
        bar.addColorStop(0, "#2e9276");
        bar.addColorStop(1, "#8fe0cd");
        ctx.fillStyle = bar;
        roundRect(ctx, 46, 698, (420 * progress) / 100, 10, 5);
        ctx.fill();

        ctx.fillStyle = "rgba(160,156,147,0.95)";
        ctx.font = `500 12px ${mono}`;
        ctx.fillText("18.4 MB/s", 46, 736);
        ctx.textAlign = "right";
        ctx.fillText("2.1 GB / 4.8 GB", 466, 736);
        ctx.textAlign = "left";

        // Logcat card.
        ctx.fillStyle = "rgba(255,255,255,0.035)";
        roundRect(ctx, 26, 782, 460, 196, 18);
        ctx.fill();
        ctx.strokeStyle = "rgba(255,255,255,0.07)";
        roundRect(ctx, 26, 782, 460, 196, 18);
        ctx.stroke();

        ctx.fillStyle = "rgba(160,156,147,0.95)";
        ctx.font = `700 11px ${mono}`;
        ctx.fillText("LIVE LOGCAT", 46, 812);
        ctx.fillStyle = "#54b39c";
        ctx.beginPath();
        ctx.arc(452, 808, 4, 0, Math.PI * 2);
        ctx.fill();

        const lines = [
          ["I", "ActivityManager", "Displayed com.android.settings"],
          ["D", "PackageManager", "install package=com.example.app"],
          ["I", "adb", "device authorised · transport 1"],
          ["D", "DroidCrate", "pull DCIM/Camera/VID_0042.mp4"],
          ["I", "MediaScanner", "scan complete · 128 files"],
          ["V", "DroidCrate", "logcat filter=all"],
          ["I", "boot", "bootloader state: locked"],
        ];
        ctx.font = `500 11px ${mono}`;
        for (let i = 0; i < 6; i += 1) {
          const line = lines[(i + cycle) % lines.length];
          const y = 838 + i * 22;
          ctx.fillStyle = "rgba(84,179,156,0.9)";
          ctx.fillText(line[0], 46, y);
          ctx.fillStyle = "rgba(160,156,147,0.85)";
          ctx.fillText(line[1], 62, y);
          ctx.fillStyle = "rgba(246,242,234,0.72)";
          ctx.fillText(line[2], 168, y);
        }
        // Blinking cursor on the last row.
        if (Math.floor(time * 2) % 2 === 0) {
          ctx.fillStyle = "#8fe0cd";
          ctx.fillRect(46, 962, 7, 12);
        }
      };

      paint(0);
      texture.needsUpdate = true;

      let lastPaint = 0;
      return {
        update(t, dt, pointer) {
          // Phone motion.
          phone.rotation.y = -0.16 + Math.sin(t * 0.38) * 0.12 + pointer.x * 0.1;
          phone.rotation.x = 0.04 + pointer.y * 0.07;
          phone.rotation.z = -0.025 + Math.sin(t * 0.5) * 0.012;
          phone.position.y = Math.sin(t * 0.6) * 0.09;

          ringGroup.rotation.z = t * 0.06;
          ringGroup.children.forEach((ring, index) => {
            ring.rotation.z = t * ring.userData.speed + index;
          });

          // Orbit dust.
          const positions = dust.geometry.attributes.position.array;
          dustBase.forEach((item, i) => {
            const a = item.angle + t * item.speed;
            positions[i * 3] = Math.cos(a) * item.radius;
            positions[i * 3 + 1] = item.y + Math.sin(t * 0.5 + i) * 0.08;
            positions[i * 3 + 2] = Math.sin(a) * item.radius * 0.6;
          });
          dust.geometry.attributes.position.needsUpdate = true;

          halo.material.opacity = 0.12 + Math.sin(t * 0.8) * 0.04;

          if (t - lastPaint > 0.12) {
            lastPaint = t;
            paint(t);
            texture.needsUpdate = true;
          }
        },
      };
    },
    { cameraZ: 9 }
  );
}

/* ---------- small 2D helpers ---------- */
function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function drawGlyph(ctx, glyph, x, y, size) {
  ctx.beginPath();
  if (glyph === "restart") {
    ctx.arc(x, y, size, -0.4, Math.PI * 1.5);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(x + size * 0.5, y - size * 0.95);
    ctx.lineTo(x + size * 1.1, y - size * 0.75);
    ctx.lineTo(x + size * 0.55, y - size * 0.35);
    ctx.stroke();
  } else if (glyph === "screen") {
    ctx.rect(x - size, y - size * 0.72, size * 2, size * 1.44);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(x, y, size * 0.4, 0, Math.PI * 2);
    ctx.stroke();
  } else if (glyph === "folder") {
    ctx.moveTo(x - size, y - size * 0.6);
    ctx.lineTo(x - size, y + size * 0.7);
    ctx.lineTo(x + size, y + size * 0.7);
    ctx.lineTo(x + size, y - size * 0.2);
    ctx.lineTo(x - size * 0.1, y - size * 0.2);
    ctx.lineTo(x - size * 0.45, y - size * 0.6);
    ctx.closePath();
    ctx.stroke();
  } else if (glyph === "box") {
    ctx.rect(x - size * 0.8, y - size * 0.8, size * 1.6, size * 1.6);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(x - size * 0.8, y - size * 0.2);
    ctx.lineTo(x, y + size * 0.3);
    ctx.lineTo(x + size * 0.8, y - size * 0.2);
    ctx.stroke();
  } else if (glyph === "terminal") {
    ctx.rect(x - size, y - size * 0.8, size * 2, size * 1.6);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(x - size * 0.55, y - size * 0.25);
    ctx.lineTo(x - size * 0.15, y + size * 0.05);
    ctx.lineTo(x - size * 0.55, y + size * 0.35);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(x + size * 0.1, y + size * 0.35);
    ctx.lineTo(x + size * 0.55, y + size * 0.35);
    ctx.stroke();
  } else if (glyph === "bolt") {
    ctx.moveTo(x + size * 0.1, y - size);
    ctx.lineTo(x - size * 0.7, y + size * 0.15);
    ctx.lineTo(x - size * 0.05, y + size * 0.15);
    ctx.lineTo(x - size * 0.15, y + size);
    ctx.lineTo(x + size * 0.7, y - size * 0.15);
    ctx.lineTo(x + size * 0.05, y - size * 0.15);
    ctx.closePath();
    ctx.stroke();
  }
}
