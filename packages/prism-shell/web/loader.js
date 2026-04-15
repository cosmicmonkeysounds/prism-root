// Browser entry point for the emscripten build of prism-shell.
//
// Imports the factory that `.cargo/config.toml` produces
// (`EXPORT_NAME=createPrismShell`), wires DOM events to the C ABI
// exports in `src/web.rs`, and on every requestAnimationFrame reads
// the render-command byte buffer back out via `HEAPU8` and paints
// into the `<canvas id="prism">`.
//
// The .js + .wasm pair is emitted by
// `cargo build --target wasm32-unknown-emscripten
//   -p prism-shell --no-default-features --features web`
// and lands in
// `target/wasm32-unknown-emscripten/<profile>/prism_shell_wasm.{js,wasm}`.
// This loader assumes the `prism dev web` command has copied both
// files next to this `index.html` (or is serving them side-by-side).

import createPrismShell from "./prism_shell_wasm.js";

// Command tags — keep in sync with the table in `src/web.rs`.
const CMD_RECT = 0;
const CMD_BORDER = 1;
const CMD_TEXT = 2;
const CMD_SCISSOR_START = 3;
const CMD_SCISSOR_END = 4;

const TEXT_DECODER = new TextDecoder("utf-8");

async function boot() {
  const canvas = document.getElementById("prism");
  if (!canvas) throw new Error("missing <canvas id=\"prism\">");
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Canvas2D context unavailable");

  const Module = await createPrismShell();

  const api = {
    boot: Module.cwrap("prism_shell_boot", null, ["number", "number"]),
    resize: Module.cwrap("prism_shell_resize", null, ["number", "number"]),
    pointerMove: Module.cwrap("prism_shell_pointer_move", null, [
      "number",
      "number",
    ]),
    pointerButton: Module.cwrap("prism_shell_pointer_button", null, [
      "number",
      "number",
      "number",
      "number",
    ]),
    wheel: Module.cwrap("prism_shell_wheel", null, ["number", "number"]),
    key: Module.cwrap("prism_shell_key", null, ["number", "number"]),
    frame: Module.cwrap("prism_shell_frame", "number", ["number"]),
    frameLen: Module.cwrap("prism_shell_frame_len", "number", []),
  };

  const dpr = () => window.devicePixelRatio || 1;

  function sizeCanvas() {
    const cssW = canvas.clientWidth || 1;
    const cssH = canvas.clientHeight || 1;
    const ratio = dpr();
    canvas.width = Math.max(1, Math.floor(cssW * ratio));
    canvas.height = Math.max(1, Math.floor(cssH * ratio));
    return { cssW, cssH, ratio };
  }

  const initial = sizeCanvas();
  api.boot(initial.cssW, initial.cssH);

  window.addEventListener("resize", () => {
    const { cssW, cssH } = sizeCanvas();
    api.resize(cssW, cssH);
  });

  canvas.addEventListener("mousemove", (e) => {
    api.pointerMove(e.offsetX, e.offsetY);
  });

  canvas.addEventListener("mousedown", (e) => {
    api.pointerButton(e.offsetX, e.offsetY, e.button, 1);
  });

  window.addEventListener("mouseup", (e) => {
    // Track releases outside the canvas so a drag that crosses the
    // edge still clears the button state in AppState.
    const rect = canvas.getBoundingClientRect();
    api.pointerButton(
      e.clientX - rect.left,
      e.clientY - rect.top,
      e.button,
      0,
    );
  });

  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      // delta_mode 1 = line; default line height matches the native
      // LineDelta scaling applied in `src/bin/native.rs`.
      const scale = e.deltaMode === 1 ? 18.0 : 1.0;
      api.wheel(e.deltaX * scale, e.deltaY * scale);
    },
    { passive: false },
  );

  window.addEventListener("keydown", (e) => {
    api.key(e.keyCode, 1);
  });
  window.addEventListener("keyup", (e) => {
    api.key(e.keyCode, 0);
  });

  let lastFrame = performance.now();

  function frame(now) {
    const dt = Math.max(0, (now - lastFrame) / 1000);
    lastFrame = now;

    const ratio = dpr();
    const ptr = api.frame(dt);
    const len = api.frameLen();

    const ctxRatio = ratio;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(ctxRatio, 0, 0, ctxRatio, 0, 0);

    if (ptr !== 0 && len > 0) {
      const heap = Module.HEAPU8;
      paint(ctx, heap, ptr, len);
    }

    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

function paint(ctx, heap, ptr, len) {
  // Copy out a subarray view so a DataView can walk it without
  // churning offsets against the big heap buffer.
  const view = new DataView(heap.buffer, heap.byteOffset + ptr, len);
  let i = 0;

  while (i < len) {
    const tag = view.getUint8(i);
    i += 1;

    switch (tag) {
      case CMD_RECT: {
        const x = view.getFloat32(i, true);
        const y = view.getFloat32(i + 4, true);
        const w = view.getFloat32(i + 8, true);
        const h = view.getFloat32(i + 12, true);
        i += 16;
        const r = view.getUint8(i);
        const g = view.getUint8(i + 1);
        const b = view.getUint8(i + 2);
        const a = view.getUint8(i + 3);
        i += 4;
        ctx.fillStyle = `rgba(${r}, ${g}, ${b}, ${a / 255})`;
        ctx.fillRect(x, y, w, h);
        break;
      }
      case CMD_BORDER: {
        const x = view.getFloat32(i, true);
        const y = view.getFloat32(i + 4, true);
        const w = view.getFloat32(i + 8, true);
        const h = view.getFloat32(i + 12, true);
        i += 16;
        const r = view.getUint8(i);
        const g = view.getUint8(i + 1);
        const b = view.getUint8(i + 2);
        const a = view.getUint8(i + 3);
        i += 4;
        const width = view.getFloat32(i, true);
        i += 4;
        if (width > 0) {
          ctx.strokeStyle = `rgba(${r}, ${g}, ${b}, ${a / 255})`;
          ctx.lineWidth = width;
          const inset = width / 2;
          ctx.strokeRect(
            x + inset,
            y + inset,
            Math.max(0, w - width),
            Math.max(0, h - width),
          );
        }
        break;
      }
      case CMD_TEXT: {
        const x = view.getFloat32(i, true);
        const y = view.getFloat32(i + 4, true);
        i += 16; // skip w, h
        const r = view.getUint8(i);
        const g = view.getUint8(i + 1);
        const b = view.getUint8(i + 2);
        const a = view.getUint8(i + 3);
        i += 4;
        const size = view.getUint16(i, true);
        i += 2;
        const byteLen = view.getUint32(i, true);
        i += 4;
        const textBytes = new Uint8Array(
          heap.buffer,
          heap.byteOffset + ptr + i,
          byteLen,
        );
        const text = TEXT_DECODER.decode(textBytes);
        i += byteLen;

        ctx.fillStyle = `rgba(${r}, ${g}, ${b}, ${a / 255})`;
        ctx.font = `${size}px ui-sans-serif, system-ui, -apple-system, sans-serif`;
        ctx.textBaseline = "top";
        ctx.fillText(text, x, y);
        break;
      }
      case CMD_SCISSOR_START: {
        const x = view.getFloat32(i, true);
        const y = view.getFloat32(i + 4, true);
        const w = view.getFloat32(i + 8, true);
        const h = view.getFloat32(i + 12, true);
        i += 16;
        ctx.save();
        ctx.beginPath();
        ctx.rect(x, y, w, h);
        ctx.clip();
        break;
      }
      case CMD_SCISSOR_END: {
        ctx.restore();
        break;
      }
      default:
        console.error("prism-shell: unknown render command tag", tag, "at", i - 1);
        return;
    }
  }
}

boot().catch((err) => {
  console.error("prism-shell boot failed", err);
});
