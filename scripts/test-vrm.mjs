import { chromium } from "@playwright/test";
import { createServer } from "vite";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";

const root = resolve(".local/live2d");
const packDir = resolve(root, "packs/vrm-viewer");
const config = JSON.parse(
  await readFile("apps/desktop/src-tauri/tauri.conf.json", "utf8"),
);
await readFile(resolve(packDir, "avatar.vrm"));
await readFile(resolve(packDir, "motions/relax.vrma"));
await readFile(resolve(packDir, "motions/thinking.vrma"));
await readFile(resolve(packDir, "manifest.json"));

const server = await createServer({
  configFile: false,
  root: resolve("apps/desktop"),
  server: { host: "127.0.0.1", port: 1433, strictPort: true },
  plugins: [
    {
      name: "local-vrm-test",
      configureServer(s) {
        s.middlewares.use(async (req, res, next) => {
          if (req.url === "/favicon.ico") {
            res.statusCode = 204;
            res.end();
            return;
          }
          if (req.url === "/smoke") {
            res.setHeader("Content-Security-Policy", config.app.security.csp);
            res.setHeader("Content-Type", "text/html");
            res.end(
              '<body style="margin:0;background:#1c2430"><canvas id="model" style="width:560px;height:720px;display:block"></canvas></body>',
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
            const data = await readFile(path);
            res.setHeader(
              "Content-Type",
              { ".json": "application/json", ".png": "image/png" }[
                extname(path)
              ] ?? "application/octet-stream",
            );
            res.end(data);
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
const browser = await chromium.launch({
  channel: "msedge",
  headless: true,
  args: ["--autoplay-policy=no-user-gesture-required"],
});
const page = await browser.newPage({ viewport: { width: 560, height: 720 } });
page.setDefaultTimeout(120000);
const errors = [];
page.on("pageerror", (error) => errors.push(String(error)));
page.on("console", (message) => {
  if (message.type() === "error") errors.push(message.text());
});
const shotDir = resolve(".local/live2d/verification/vrm");
await mkdir(shotDir, { recursive: true });
try {
  await page.goto("http://127.0.0.1:1433/smoke");
  await page.evaluate(() => {
    window.__TAURI_INTERNALS__ = {
      convertFileSrc: (path) => `/local/${path}`,
      invoke: async () => {},
    };
  });
  await page.evaluate(async () => {
    const { VrmRenderer } = await import("/src/avatar/vrm/model.ts");
    const { assertManifest } = await import("/src/avatar/manifest.ts");
    const { avatarDefaults } = await import("/src/avatar/types.ts");
    const pack = await (
      await fetch("/local/packs/vrm-viewer/manifest.json")
    ).json();
    assertManifest(pack);
    const canvas = document.querySelector("canvas");
    const input = {
      state: "idle",
      speakingLevel: 0,
      interaction: 0,
      cursorFocus: { x: 500, y: 260 },
      preferences: avatarDefaults(pack),
    };
    const renderer = new VrmRenderer(
      canvas,
      pack,
      "/local/packs/vrm-viewer",
      input,
      (error) => {
        throw error;
      },
    );
    window.__renderer = renderer;
    window.__input = input;
    await renderer.load();
  });
  await page.waitForTimeout(400);
  const result = await page.evaluate(async () => {
    const { pointHitsAvatar } =
      await import("/src/avatar/live2d/passthrough.ts");
    const renderer = window.__renderer;
    const input = window.__input;

    const render = () => {
      renderer.last = 0;
      renderer.tick(performance.now());
    };
    renderer.pause(true);
    renderer.paused = false;
    for (let i = 0; i < 20; i++) render();
    let textures = 0;
    renderer.vrm.scene.traverse((object) => {
      for (const material of [object.material].flat().filter(Boolean)) {
        if (material.map?.image?.width > 0) textures++;
      }
    });
    if (!textures) throw Error("模型没有加载任何颜色贴图");
    if (renderer.currentName !== "idle") throw Error("待机动作没有播放");
    const arm = renderer.vrm.humanoid.getNormalizedBoneNode("leftUpperArm");
    const before = arm.quaternion.clone();
    for (let i = 0; i < 40; i++) render();
    const idleMovement = before.angleTo(arm.quaternion);
    if (idleMovement < 0.001) throw Error("待机动作未驱动骨骼");
    const gl = renderer.renderer.getContext();
    const { width, height } = renderer.canvas;
    const pixels = new Uint8Array(width * height * 4);
    gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    let visible = 0;
    let transparent = 0;
    let head = 0;
    for (let index = 3; index < pixels.length; index += 4) {
      const alpha = pixels[index];
      const pixel = (index - 3) / 4;
      const y = Math.floor(pixel / width);
      if (alpha > 16) {
        visible++;
        if (y > height * 0.55) head++;
      } else transparent++;
    }
    if (!width || !height) throw Error(`画布尺寸无效：${width}x${height}`);
    if (visible < 2000) throw Error(`模型没有画出来：${visible}`);
    if (transparent < 1000) throw Error(`背景不透明：${transparent}`);
    if (head < 200) throw Error(`头部不在画面上方：${head}`);
    const png = renderer.canvas.toDataURL("image/png");
    renderer.update({ ...input, state: "speaking", speakingLevel: 1 });
    for (let i = 0; i < 20; i++) render();
    const mouth = renderer.vrm.expressionManager.getValue("aa") ?? 0;
    if (mouth < 0.15) throw Error(`说话时嘴没有张开：${mouth}`);
    const yaw = renderer.vrm.lookAt?.yaw ?? 0;
    if (!renderer.vrm.lookAt || Math.abs(yaw) < 2)
      throw Error(`没有看向鼠标：${yaw}`);
    renderer.update({ ...input, state: "thinking", speakingLevel: 0 });
    for (let i = 0; i < 40; i++) render();
    if (renderer.currentName !== "thinking") {
      throw Error(
        `思考动作没有播放：${renderer.currentName} clips=${[...renderer.clips.keys()].join(",")}`,
      );
    }
    const thinkingPng = renderer.canvas.toDataURL("image/png");
    renderer.update(input);
    for (let i = 0; i < 40; i++) render();
    if (renderer.currentName !== "idle") throw Error("思考结束后没有恢复待机");
    let hits = 0;
    let misses = 0;
    for (let y = 6; y < window.innerHeight; y += 18) {
      for (let x = 6; x < window.innerWidth; x += 18) {
        if (pointHitsAvatar(x, y)) hits++;
        else misses++;
      }
    }
    if (hits < 4 || misses < 4)
      throw Error(`点击区域异常：命中 ${hits}，穿透 ${misses}`);
    const meta = {
      name: renderer.vrm.meta?.name ?? "",
      metaVersion: renderer.vrm.meta?.metaVersion ?? "",
    };
    const clip = renderer.currentName;
    renderer.dispose(false);
    const { VrmRenderer } = await import("/src/avatar/vrm/model.ts");
    const fallback = new VrmRenderer(
      renderer.canvas,
      { ...renderer.pack, motions: {} },
      renderer.root,
      input,
      (error) => {
        throw error;
      },
    );
    await fallback.load();
    fallback.pause(true);
    fallback.paused = false;
    fallback.tick(performance.now());
    for (const side of ["left", "right"]) {
      const hand = fallback.vrm.humanoid
        .getRawBoneNode(`${side}Hand`)
        .getWorldPosition(arm.position.clone());
      const shoulder = fallback.vrm.humanoid
        .getRawBoneNode(`${side}UpperArm`)
        .getWorldPosition(arm.position.clone());
      if (hand.y >= shoulder.y - 0.1)
        throw Error(`无动作时 ${side} 手臂没有自然下垂`);
    }
    const fallbackPng = fallback.canvas.toDataURL("image/png");
    fallback.dispose(false);
    const broken = new VrmRenderer(
      renderer.canvas,
      { ...renderer.pack, motions: { idle: "avatar.vrm" } },
      renderer.root,
      input,
      (error) => {
        throw error;
      },
    );
    let motionError = "";
    try {
      await broken.load();
    } catch (error) {
      motionError = String(error);
    } finally {
      broken.dispose();
    }
    if (!motionError.includes("VRM 动作加载失败"))
      throw Error("无效动作被静默忽略");
    return {
      textures,
      idleMovement,
      visible,
      transparent,
      head,
      mouth,
      yaw,
      hits,
      misses,
      meta,
      clip,
      png,
      thinkingPng,
      fallbackPng,
    };
  });
  if (errors.length) throw Error(errors.join("\n"));
  const { png, thinkingPng, fallbackPng, ...stats } = result;
  const image = Buffer.from(png.split(",")[1], "base64");
  await writeFile(resolve(shotDir, "idle.png"), image);
  await writeFile(
    resolve(shotDir, "thinking.png"),
    Buffer.from(thinkingPng.split(",")[1], "base64"),
  );
  await writeFile(
    resolve(shotDir, "fallback.png"),
    Buffer.from(fallbackPng.split(",")[1], "base64"),
  );
  await writeFile(resolve(packDir, "preview.png"), image);
  await writeFile(
    resolve(shotDir, "result.json"),
    JSON.stringify(stats, null, 2),
  );
  console.log(JSON.stringify(stats, null, 2));
} finally {
  await browser.close();
  await server.close();
}
