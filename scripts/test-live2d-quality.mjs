// Real GPU regression: resolve subpixel colour/coverage, then render local models.
import assert from "node:assert/strict";
import { chromium } from "@playwright/test";
import { createServer } from "vite";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";

const baseline = process.argv.includes("--baseline");
const root = resolve(".local/live2d");
const output = resolve(
  root,
  "verification/quality",
  baseline ? "before" : "after",
);
await mkdir(output, { recursive: true });
const server = await createServer({
  configFile: false,
  root: resolve("apps/desktop"),
  server: { host: "127.0.0.1", port: 1433, strictPort: true },
  plugins: [
    {
      name: "quality-fixture",
      configureServer(s) {
        s.middlewares.use(async (req, res, next) => {
          if (req.url === "/quality") {
            res.setHeader("Content-Type", "text/html");
            res.end(
              "<style>body{margin:0;background:#778899}canvas{display:block}</style>",
            );
            return;
          }
          if (!req.url?.startsWith("/local/")) return next();
          try {
            const path = resolve(
              root,
              decodeURIComponent(req.url.split("?")[0].slice(7)),
            );
            if (!path.startsWith(root + sep)) throw Error("path");
            const bytes = await readFile(path);
            res.setHeader(
              "Content-Type",
              {
                ".js": "text/javascript",
                ".json": "application/json",
                ".png": "image/png",
              }[extname(path)] ?? "application/octet-stream",
            );
            res.end(bytes);
          } catch {
            res.statusCode = 404;
            res.end();
          }
        });
      },
    },
  ],
});
await server.listen();
const browser = await chromium.launch({ channel: "msedge", headless: true });
const report = [];
try {
  for (const dpr of [1, 1.5, 2]) {
    const context = await browser.newContext({
      deviceScaleFactor: dpr,
      viewport: { width: 600, height: 760 },
    });
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto("http://127.0.0.1:1433/quality");
    const pixels = await page.evaluate(async () => {
      window.__TAURI_INTERNALS__ = {
        convertFileSrc: (path) => `/local/${path}`,
      };
      const { LineResolve } = await import("/src/avatar/live2d/resolve.ts");
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = 1;
      const gl = canvas.getContext("webgl2", {
        antialias: false,
        premultipliedAlpha: true,
      });
      const scene = new LineResolve(gl);
      if (!scene.resize(1, 1)) throw Error("Resolve target unavailable");
      const sample = (rgba) => {
        gl.bindTexture(gl.TEXTURE_2D, scene.texture);
        gl.texSubImage2D(
          gl.TEXTURE_2D,
          0,
          0,
          0,
          2,
          2,
          gl.RGBA,
          gl.UNSIGNED_BYTE,
          new Uint8Array(rgba.flat()),
        );
        scene.present(1, 1);
        const out = new Uint8Array(4);
        gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, out);
        return [...out];
      };
      const white = [255, 255, 255, 255],
        black = [0, 0, 0, 255],
        clear = [0, 0, 0, 0];
      const opaque = [0, 1, 2, 3].map((i) =>
        sample([0, 1, 2, 3].map((j) => (i === j ? black : white))),
      );
      const edge = sample([white, clear, clear, clear]);
      const mixed = sample([[128, 0, 0, 128], [0, 0, 255, 255], clear, clear]);
      const error = gl.getError();
      scene.dispose();
      gl.getExtension("WEBGL_lose_context")?.loseContext();
      return { opaque, edge, mixed, error };
    });
    if (!baseline) {
      for (const pixel of pixels.opaque) {
        assert.ok(
          Math.abs(pixel[0] - 191) <= 1,
          `Thin dark line lost: ${pixel}`,
        );
        assert.deepEqual(pixel.slice(0, 3), [pixel[0], pixel[0], pixel[0]]);
        assert.equal(pixel[3], 255);
      }
      for (const value of pixels.edge)
        assert.ok(
          Math.abs(value - 64) <= 1,
          `Coverage inflated: ${pixels.edge}`,
        );
      for (const [i, expected] of [32, 0, 64, 96].entries())
        assert.ok(
          Math.abs(pixels.mixed[i] - expected) <= 1,
          `Premultiplied blend incorrect: ${pixels.mixed}`,
        );
      assert.equal(pixels.error, 0);
    }
    for (const name of ["haru", "hiyori"]) {
      const result = await page.evaluate(
        async ({ name }) => {
          const { Live2DRenderer } =
            await import("/src/avatar/live2d/model.ts");
          const { avatarDefaults } = await import("/src/avatar/types.ts");
          const pack = await (
            await fetch(`/local/packs/${name}/manifest.json`)
          ).json();
          const canvas = document.createElement("canvas");
          canvas.style.cssText = "width:560px;height:720px";
          document.body.replaceChildren(canvas);
          const input = {
            state: "idle",
            speakingLevel: 0,
            interaction: 0,
            cursorFocus: null,
            preferences: {
              ...avatarDefaults(pack),
              motions: false,
              idleAnimation: false,
              autoBlink: false,
              mouseTracking: false,
            },
          };
          const renderer = new Live2DRenderer(
            canvas,
            pack,
            `/local/packs/${name}`,
            input,
            (e) => {
              throw e;
            },
          );
          await renderer.load();
          await new Promise((r) => setTimeout(r, 800));
          renderer.pause(true);
          renderer.paused = false;
          renderer.last = 0;
          renderer.tick(1000);
          const gl = canvas.getContext("webgl2");
          const gpuInfo = gl.getExtension("WEBGL_debug_renderer_info");
          const data = new Uint8Array(canvas.width * canvas.height * 4);
          gl.readPixels(
            0,
            0,
            canvas.width,
            canvas.height,
            gl.RGBA,
            gl.UNSIGNED_BYTE,
            data,
          );
          let visible = 0,
            invalidPremultiplied = 0;
          for (let i = 0; i < data.length; i += 4) {
            if (data[i + 3]) visible++;
            if (Math.max(data[i], data[i + 1], data[i + 2]) > data[i + 3] + 2)
              invalidPremultiplied++;
          }
          const png = canvas.toDataURL();
          // Measure animation frame submissions, including waiting for real GPU completion.
          renderer.update({
            ...input,
            preferences: { ...input.preferences, idleAnimation: true },
          });
          const stamps = [];
          const original = renderer.model.frame.bind(renderer.model);
          renderer.model.frame = (...args) => {
            original(...args);
            stamps.push(performance.now());
          };
          await new Promise((r) => setTimeout(r, 1600));
          renderer.pause(true);
          gl.finish();
          const intervals = stamps
            .slice(1)
            .map((t, i) => t - stamps[i])
            .sort((a, b) => a - b);
          const result = {
            name,
            dpr: devicePixelRatio,
            width: canvas.width,
            height: canvas.height,
            visible,
            invalidPremultiplied,
            glError: gl.getError(),
            frames: stamps.length,
            gpu: gpuInfo
              ? gl.getParameter(gpuInfo.UNMASKED_RENDERER_WEBGL)
              : gl.getParameter(gl.RENDERER),
            medianFrameMs: intervals[Math.floor(intervals.length / 2)],
            png,
          };
          renderer.dispose();
          return result;
        },
        { name },
      );
      const { png, ...metrics } = result;
      await writeFile(
        resolve(output, `${name}-${dpr}.png`),
        Buffer.from(png.split(",")[1], "base64"),
      );
      assert.ok(metrics.visible > 0);
      assert.equal(metrics.glError, 0);
      if (!baseline) assert.equal(metrics.invalidPremultiplied, 0);
      report.push({ ...metrics, resolve: pixels });
      console.log(JSON.stringify(metrics));
    }
    assert.deepEqual(errors, []);
    await context.close();
  }
  await writeFile(
    resolve(output, "report.json"),
    JSON.stringify(report, null, 2),
  );
} finally {
  await browser.close();
  await server.close();
}
