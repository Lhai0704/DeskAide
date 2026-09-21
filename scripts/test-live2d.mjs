import { chromium } from "@playwright/test";
import { createServer } from "vite";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";
const root = resolve(".local/live2d");
await readFile(resolve(root, "runtime/live2dcubismcore.min.js"));
await readFile(resolve(root, "packs/haru/manifest.json"));
const server = await createServer({
  configFile: false,
  root: resolve("apps/desktop"),
  server: { host: "127.0.0.1", port: 1432, strictPort: true },
  plugins: [
    {
      name: "local-live2d-test",
      configureServer(s) {
        s.middlewares.use(async (req, res, next) => {
          if (req.url === "/smoke") {
            res.setHeader("Content-Type", "text/html");
            res.end(
              '<canvas id="model" style="width:240px;height:320px"></canvas>',
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
              {
                ".js": "text/javascript",
                ".json": "application/json",
                ".png": "image/png",
              }[extname(path)] ?? "application/octet-stream",
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
const page = await browser.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
try {
  await page.goto("http://127.0.0.1:1432/smoke");
  await page.evaluate(() => {
    window.__TAURI_INTERNALS__ = { convertFileSrc: (path) => `/local/${path}` };
  });
  const result = await page.evaluate(async () => {
    const { Live2DRenderer } = await import("/src/avatar/live2d/model.ts");
    const { avatarDefaults } = await import("/src/avatar/types.ts");
    const results = [];
    for (let index = 0; index < 22; index++) {
      const name = index % 2 ? "ren" : "haru";
      const pack = await (
        await fetch(`/local/packs/${name}/manifest.json`)
      ).json();
      const canvas = document.createElement("canvas");
      canvas.style.cssText = "width:240px;height:320px";
      document.body.replaceChildren(canvas);
      const input = {
        state: "idle",
        speakingLevel: 0,
        interaction: 0,
        cursorFocus: { x: 400, y: 60 },
        preferences: avatarDefaults(pack),
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
      await new Promise((r) => setTimeout(r, index < 2 ? 1800 : 120));
      if (index < 2) {
        renderer.update({ ...input, state: "speaking", speakingLevel: 0.8 });
        await new Promise((r) => setTimeout(r, 100));
        const core = renderer.model.getModel().getModel();
        const mouth =
          core.parameters.values[
            core.parameters.ids.indexOf("ParamMouthOpenY")
          ];
        const gl = canvas.getContext("webgl2");
        renderer.last = 0;
        renderer.tick(performance.now());
        const pixels = new Uint8Array(canvas.width * canvas.height * 4);
        gl.readPixels(
          0,
          0,
          canvas.width,
          canvas.height,
          gl.RGBA,
          gl.UNSIGNED_BYTE,
          pixels,
        );
        const visible = pixels.filter((v, i) => i % 4 === 3 && v > 0).length;
        const transparent = pixels.filter(
          (v, i) => i % 4 === 3 && v === 0,
        ).length;
        if (!visible || !transparent)
          throw Error(`Empty or opaque ${name}: ${visible}/${transparent}`);
        if (mouth < 0.5) throw Error("Mouth signal not applied");
        const eye =
          core.parameters.values[core.parameters.ids.indexOf("ParamEyeBallX")];
        if (!(eye > 0 && eye <= 1)) throw Error("Gaze not applied");
        renderer.update({ ...input, state: "thinking" });
        await new Promise((r) => setTimeout(r, 80));
        if (renderer.model.expressionName !== pack.expressions?.thinking)
          throw Error("Thinking expression not selected");
        renderer.update({ ...input, state: "idle" });
        await new Promise((r) => setTimeout(r, 80));
        if (renderer.model.expressionName !== pack.expressions?.neutral)
          throw Error("Expression not released");
        canvas.style.width = "200px";
        renderer.resize();
        if (canvas.width !== Math.round(200 * Math.min(2, devicePixelRatio)))
          throw Error("Resize/DPR mismatch");
        renderer.pause(true);
        const pausedAt = renderer.last;
        await new Promise((r) => setTimeout(r, 100));
        if (renderer.frame || renderer.last !== pausedAt)
          throw Error("Hidden renderer did not pause");
        renderer.pause(false);
        results.push({
          name,
          visible,
          transparent,
          mouth,
          eye,
          mocVersion: renderer.model.getMocVersionFromBuffer(
            await (
              await fetch(
                `/local/packs/${name}/model/${name === "haru" ? "Haru" : "Ren"}.moc3`,
              )
            ).arrayBuffer(),
          ),
        });
      }
      renderer.dispose();
      renderer.dispose();
      if (renderer.model || renderer.textures.length || renderer.frame)
        throw Error("Resources retained");
    }
    // A short reply must not truncate the real Haru gesture at the next idle snapshot.
    {
      const pack = await (
        await fetch("/local/packs/haru/manifest.json")
      ).json();
      const canvas = document.createElement("canvas");
      canvas.style.cssText = "width:240px;height:320px";
      document.body.replaceChildren(canvas);
      const input = {
        state: "responding",
        speakingLevel: 0,
        interaction: 0,
        cursorFocus: null,
        preferences: avatarDefaults(pack),
      };
      const renderer = new Live2DRenderer(
        canvas,
        pack,
        "/local/packs/haru",
        input,
        (e) => {
          throw e;
        },
      );
      await renderer.load();
      await new Promise((r) => setTimeout(r, 200));
      const key = `${pack.motions.responding.group}:${pack.motions.responding.index}`;
      renderer.update({ ...input, state: "idle" });
      await new Promise((r) => setTimeout(r, 200));
      if (renderer.model.current !== key)
        throw Error(
          `Short reply gesture was truncated: ${JSON.stringify({ key, current: renderer.model.current, active: renderer.motion.active, activeState: renderer.motion.activeState, elapsed: renderer.motion.elapsed, finished: renderer.model.finished() })}`,
        );
      if (renderer.model.motions.get(key).motion.getFadeInTime() !== 0.5)
        throw Error("Model fade settings not applied");
      await new Promise((r) => setTimeout(r, 5700));
      if (
        renderer.model.current !==
        `${pack.motions.idle.group}:${pack.motions.idle.index}`
      )
        throw Error("Reply gesture did not return to idle");
      renderer.dispose();
    }
    const pack = await (await fetch("/local/packs/haru/manifest.json")).json();
    const input = {
      state: "idle",
      speakingLevel: 0,
      interaction: 0,
      cursorFocus: null,
      preferences: avatarDefaults(pack),
    };
    for (const cancel of [true, false]) {
      const canvas = document.createElement("canvas");
      document.body.replaceChildren(canvas);
      const renderer = new Live2DRenderer(
        canvas,
        cancel ? pack : { ...pack, model: "missing.model3.json" },
        "/local/packs/haru",
        input,
        (e) => {
          throw e;
        },
      );
      const pending = renderer.load();
      if (cancel) renderer.dispose();
      let rejected = false;
      try {
        await pending;
      } catch {
        rejected = true;
      } finally {
        renderer.dispose();
      }
      if (
        !rejected ||
        renderer.model ||
        renderer.textures.length ||
        renderer.frame
      )
        throw Error("Load failure/cancellation leaked");
    }
    const { SpeechPlayer } = await import("/src/speech/player.ts");
    const player = new SpeechPlayer();
    await player.start(0.8);
    const pcm = new Float32Array(24000);
    for (let i = 0; i < pcm.length; i++)
      pcm[i] = Math.sin((i / 24000) * 440 * Math.PI * 2) * 0.15;
    const bytes = new Uint8Array(pcm.buffer);
    let binary = "";
    for (const b of bytes) binary += String.fromCharCode(b);
    player.append(btoa(binary), 24000);
    if (player.presentation().playing)
      throw Error("Speaking before scheduled audio");
    let audio = player.presentation();
    for (let i = 0; i < 100 && (!audio.playing || audio.level <= 0); i++) {
      await new Promise((r) => setTimeout(r, 20));
      audio = player.presentation();
    }
    if (!audio.playing || audio.level <= 0)
      throw Error(
        `Real PCM analyser unavailable: ${JSON.stringify({ audio, time: player.context?.currentTime, state: player.context?.state, sources: [...player.sources.values()] })}`,
      );
    player.setVolume(0);
    await new Promise((r) => setTimeout(r, 150));
    for (let i = 0; i < 30; i++) player.presentation();
    if (player.presentation().level > 0.01)
      throw Error("Muted PCM kept mouth open");
    player.stop();
    if (player.presentation().playing || player.presentation().level)
      throw Error("Stop did not close mouth");
    return {
      coreVersion: window.Live2DCubismCore.Version.csmGetVersion(),
      models: results,
      switches: 22,
      resize: true,
      pause: true,
      cancel: true,
      loadError: true,
      realWebAudio: audio,
    };
  });
  if (errors.length) throw Error(errors.join("\n"));
  await mkdir(".local/live2d/verification", { recursive: true });
  await writeFile(
    ".local/live2d/verification/browser.json",
    JSON.stringify(result, null, 2),
  );
  console.log(JSON.stringify(result, null, 2));
} finally {
  await browser.close();
  await server.close();
}
