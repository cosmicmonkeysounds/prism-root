import { test, expect } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const SCREENSHOT_DIR = path.join(__dirname, "screenshots");

async function waitForSlintRender(page: any) {
  await page.waitForSelector("canvas#canvas", { state: "visible" });
  // Give Slint time to init wasm + first render
  await page.waitForTimeout(4000);
  // Trigger events so winit processes resize
  await page.mouse.move(640, 400);
  await page.waitForTimeout(500);
}

// Extract the WebGL canvas content via toDataURL inside a rAF
async function saveCanvasScreenshot(page: any, name: string) {
  const filePath = path.join(SCREENSHOT_DIR, name);

  // Try to get canvas content via requestAnimationFrame + toDataURL
  const dataUrl: string | null = await page.evaluate(() => {
    return new Promise<string | null>((resolve) => {
      const canvas = document.querySelector(
        "canvas#canvas",
      ) as HTMLCanvasElement;
      if (!canvas) {
        resolve(null);
        return;
      }
      // Try to get current content immediately (works if preserveDrawingBuffer is true)
      const data = canvas.toDataURL("image/png");
      if (data && data.length > 100) {
        resolve(data);
        return;
      }
      // Fallback: try inside rAF
      requestAnimationFrame(() => {
        resolve(canvas.toDataURL("image/png"));
      });
    });
  });

  if (dataUrl && dataUrl.startsWith("data:image/png")) {
    const base64 = dataUrl.replace(/^data:image\/png;base64,/, "");
    fs.writeFileSync(filePath, Buffer.from(base64, "base64"));
    console.log(`Saved canvas screenshot: ${name} (${base64.length} bytes)`);
  } else {
    // Fallback: regular page screenshot
    await page.screenshot({ path: filePath });
    console.log(`Saved page screenshot (fallback): ${name}`);
  }

  // Also check canvas internal dimensions
  const dims = await page.evaluate(() => {
    const c = document.querySelector("canvas#canvas") as HTMLCanvasElement;
    return c
      ? { w: c.width, h: c.height, cw: c.clientWidth, ch: c.clientHeight }
      : null;
  });
  console.log(`  Canvas dims: ${JSON.stringify(dims)}`);
}

test.describe("Code Editor", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await waitForSlintRender(page);
  });

  test("initial render and code editor panel", async ({ page }) => {
    await saveCanvasScreenshot(page, "01-initial.png");

    // Click code editor nav button
    await page.mouse.click(24, 108);
    await page.waitForTimeout(500);
    await saveCanvasScreenshot(page, "02-code-editor.png");

    // Try other y positions if the button wasn't hit
    await page.mouse.click(24, 88);
    await page.waitForTimeout(500);
    await saveCanvasScreenshot(page, "03-try-y88.png");

    await page.mouse.click(24, 128);
    await page.waitForTimeout(500);
    await saveCanvasScreenshot(page, "04-try-y128.png");
  });

  test("editor click and type", async ({ page }) => {
    // Try each nav button position
    await page.mouse.click(24, 108);
    await page.waitForTimeout(300);
    await page.mouse.click(24, 128);
    await page.waitForTimeout(300);

    // Click in editor content area
    await page.mouse.click(200, 60);
    await page.waitForTimeout(300);
    await saveCanvasScreenshot(page, "05-clicked.png");

    // Type text
    await page.keyboard.type("HELLO");
    await page.waitForTimeout(300);
    await saveCanvasScreenshot(page, "06-typed.png");
  });

  test("drag selection", async ({ page }) => {
    // Navigate to code editor
    await page.mouse.click(24, 108);
    await page.waitForTimeout(300);

    // Focus editor
    await page.mouse.click(200, 40);
    await page.waitForTimeout(300);

    // Drag select
    await page.mouse.move(150, 40);
    await page.mouse.down();
    await page.waitForTimeout(50);
    await page.mouse.move(400, 40, { steps: 20 });
    await page.waitForTimeout(300);
    await saveCanvasScreenshot(page, "07-drag.png");
    await page.mouse.up();
    await page.waitForTimeout(200);
    await saveCanvasScreenshot(page, "08-after-drag.png");
  });
});
