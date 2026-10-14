'use strict';
// Mouse and keyboard stand-in for the controller: the arrow keys send D-pad steps, dragging moves a virtual Sixaxis,
// and the pointer's place goes to the flow grid. Created by `index.html`, polled by `particles-reverse.js`.

(function () {
  // The XMB's step directions, in the order the particle system takes them.
  const LEFT = 0;
  const RIGHT = 1;
  const UP = 2;
  const DOWN = 3;
  const KEYS = { ArrowLeft: LEFT, ArrowRight: RIGHT, ArrowUp: UP, ArrowDown: DOWN };
  const MAX_QUEUED = 8;
  // The PPU reads the sensors as (raw - 511) / 512, and at rest the Sixaxis's y reads 399 in the savestates: 1 g.
  const SENSOR_PER_G = 112 / 512;
  const DRAG_SMOOTHING_SEC = 0.03;

  window.createXmbInput = function createXmbInput(canvas, settings) {
    // Where the pointer is, in normalised device coordinates with y up, while it is over the scene and not dragging.
    let pointer = null;
    const steps = []; // directions sent since the last poll
    // The arrow keys down, oldest first, and whether the browser has started repeating one: from then on the newest
    // of them is held, which the XMB repeats in the particle system, counting the console's frames. Letting it go
    // hands the hold to the one before it, if that is still down - the browser repeats only the last key pressed.
    const down = [];
    let repeating = false;
    let dragging = false;
    let lastX = 0;
    let lastY = 0;
    let dragX = 0; // pointer travel while dragging since the last poll, in screen heights
    let dragY = 0;
    let velX = 0; // smoothed controller velocity, screen heights per second
    let velY = 0;
    // x, y and z of the accelerometer and the gyro, in the PPU's units; at rest only gravity reads.
    const sensor = [0, -SENSOR_PER_G, 0, 0];

    function overUi(target) {
      return !!(target && target.closest && target.closest('.settings-layer-host'));
    }

    function send(dir, times) {
      for (let i = 0; i < times && steps.length < MAX_QUEUED; i++) steps.push(dir);
    }

    function clamp1(v) { return Math.max(-1, Math.min(1, v)); }

    canvas.addEventListener('pointerdown', function (e) {
      if (e.button !== 0) return;
      dragging = true;
      pointer = null;
      lastX = e.clientX;
      lastY = e.clientY;
      canvas.setPointerCapture(e.pointerId);
    });

    function endDrag() { dragging = false; }
    canvas.addEventListener('pointerup', endDrag);
    canvas.addEventListener('pointercancel', endDrag);
    window.addEventListener('blur', function () {
      endDrag();
      down.length = 0;
      repeating = false;
      pointer = null;
    });

    window.addEventListener('pointermove', function (e) {
      const h = window.innerHeight || 1;
      if (dragging) {
        dragX += (e.clientX - lastX) / h;
        dragY += (e.clientY - lastY) / h;
        lastX = e.clientX;
        lastY = e.clientY;
        return;
      }
      if (overUi(e.target)) {
        pointer = null;
        return;
      }
      pointer = [(e.clientX / (window.innerWidth || 1)) * 2 - 1, 1 - (e.clientY / h) * 2];
    });

    document.documentElement.addEventListener('pointerleave', function () { pointer = null; });

    window.addEventListener('keydown', function (e) {
      if (overUi(document.activeElement)) return;
      const dir = KEYS[e.key];
      if (dir === undefined) return;
      if (!e.repeat) {
        if (down.indexOf(dir) < 0) down.push(dir);
        send(dir, 1);
        repeating = false;
      } else {
        repeating = true;
      }
    });

    window.addEventListener('keyup', function (e) {
      const i = down.indexOf(KEYS[e.key]);
      if (i >= 0) down.splice(i, 1);
      if (!down.length) repeating = false;
    });

    // Called once per rendered frame: the steps sent since the last call, the direction held, the Sixaxis's reading,
    // where the pointer is, and the diagonal - a horizontal and a vertical arrow down together, as [horizontal,
    // vertical] - or null.
    function poll(dtSec) {
      const dt = Math.max(dtSec, 1e-3);

      // The drag moves the controller: its acceleration reads on the accelerometer, in g through `mouseAccelToG`,
      // and its sideways speed turns it, which the gyro reads through `mouseYawGain`.
      const k = 1 - Math.exp(-dt / DRAG_SMOOTHING_SEC);
      const prevX = velX;
      const prevY = velY;
      velX += (dragX / dt - velX) * k;
      velY += (dragY / dt - velY) * k;
      dragX = dragY = 0;
      const toG = settings.mouseAccelToG;
      const accelRight = ((velX - prevX) / dt) * toG;
      const accelUp = (-(velY - prevY) / dt) * toG;
      sensor[0] = clamp1(accelRight * SENSOR_PER_G);
      sensor[1] = clamp1(-(1 + accelUp) * SENSOR_PER_G);
      sensor[2] = 0;
      sensor[3] = clamp1(velX * settings.mouseYawGain);

      const held = repeating && down.length ? down[down.length - 1] : -1;
      let across = -1;
      let upDown = -1;
      for (const d of down) {
        if (d === LEFT || d === RIGHT) across = d;
        else upDown = d;
      }
      const diagonal = across >= 0 && upDown >= 0 ? [across, upDown] : null;
      const out = { steps: steps.slice(), held, sensor, pointer: pointer && pointer.slice(), diagonal };
      steps.length = 0;
      return out;
    }

    return { poll };
  };
})();
