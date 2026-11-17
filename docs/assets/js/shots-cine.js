/* Asraab cinematic screenshots.
 *
 * A screenshot is never completely still: while it travels up the page the
 * picture pushes in a little and settles, the way a camera holds a subject
 * instead of parking on it. The scroll position is the playhead, so scrolling
 * back up reverses the move. Reduced motion keeps every frame at rest.
 */
(function () {
  "use strict";

  const gsap = window.gsap;
  const ScrollTrigger = window.ScrollTrigger;
  if (!gsap || !ScrollTrigger) return;
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

  const shots = Array.from(document.querySelectorAll(".shot-open img"));
  if (!shots.length) return;

  const compact = window.matchMedia("(max-width: 900px)").matches;

  shots.forEach((img) => {
    const frame = img.closest(".shot");
    if (!frame) return;

    // The picture starts a touch closer to the lens and settles to 1. The
    // value multiplies the hover zoom in CSS, so the two never fight.
    const state = { z: compact ? 1.028 : 1.055 };
    const write = () => img.style.setProperty("--cine-z", state.z.toFixed(4));
    write();

    gsap.to(state, {
      z: 1,
      ease: "none",
      onUpdate: write,
      scrollTrigger: {
        trigger: frame,
        start: "top 96%",
        end: "top 44%",
        scrub: 0.7,
      },
    });
  });
})();
