// Optional full local TTS/WebView2 smoke; uses an existing reference without saving settings/audio.
import { chromium } from "@playwright/test";
import { writeFile } from "node:fs/promises";
const browser = await chromium.connectOverCDP("http://127.0.0.1:9227");
let avatar, assistant;
for (const p of browser.contexts().flatMap((c) => c.pages())) {
  const label = await p.evaluate(
    () => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label,
  );
  if (label === "avatar") avatar = p;
  if (label === "assistant") assistant = p;
}
if (!avatar || !assistant) throw Error("DeskAide windows missing");
const invoke = (command, args = {}) =>
  avatar.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
const saved = await invoke("get_avatar_settings");
try {
  await invoke("save_avatar_settings", {
    value: { ...saved, packId: "local-haru" },
  });
  await avatar.waitForSelector("canvas");
  await avatar.waitForFunction(() => !document.querySelector(".fallback"));
  await avatar.evaluate(() => {
    const proto = window.Live2DCubismCore.Model.prototype,
      original = proto.update;
    window.__mouthProbe = {
      max: 0,
      value: 0,
      restore: () => {
        proto.update = original;
      },
    };
    proto.update = function (...args) {
      const index = this.parameters.ids.indexOf("ParamMouthOpenY");
      if (index >= 0) {
        window.__mouthProbe.value = this.parameters.values[index];
        window.__mouthProbe.max = Math.max(
          window.__mouthProbe.max,
          window.__mouthProbe.value,
        );
      }
      return original.apply(this, args);
    };
  });
  await invoke("toggle_assistant");
  await assistant.getByTitle("设置", { exact: true }).click();
  await assistant.getByRole("button", { name: /语音播报/ }).click();
  const panel = assistant.locator(".speech-settings");
  await panel.getByRole("button", { name: "测试连接 / 刷新声音" }).click();
  await panel
    .getByRole("button", { name: "测试连接 / 刷新声音" })
    .waitFor({ timeout: 60000 });
  const select = panel.locator("select").first();
  const options = await select
    .locator("option")
    .evaluateAll((nodes) => nodes.map((n) => n.value).filter(Boolean));
  if (!options.length)
    throw Error(
      "Local TTS has no reference voice; " +
        (await panel.locator("[role=status]").first().textContent()),
    );
  await select.selectOption(options[0]);
  await panel.getByLabel("播报音量").fill("0.2");
  await panel.getByRole("button", { name: "试听", exact: true }).click();
  await avatar.waitForFunction(
    () => document.querySelector("button.avatar").dataset.state === "speaking",
    {},
    { timeout: 120000 },
  );
  await avatar.waitForFunction(() => window.__mouthProbe.value > 0.02);
  await invoke("hide_assistant");
  await new Promise((r) => setTimeout(r, 600));
  const signal = await invoke("get_avatar_presentation");
  if (!signal.speech?.playing)
    throw Error("Speech stopped when panel was hidden");
  const peak = await avatar.evaluate(() => window.__mouthProbe.max);
  await invoke("toggle_assistant");
  await panel.getByRole("button", { name: "停止朗读", exact: true }).click();
  await avatar.waitForFunction(
    () => document.querySelector("button.avatar").dataset.state !== "speaking",
  );
  const stopped = await invoke("get_avatar_presentation");
  if (stopped.speech?.playing || stopped.speech?.level)
    throw Error("Speech signal did not reset");
  const result = {
    nativeTts: true,
    hiddenPlayback: true,
    mouthPeak: peak,
    stopReset: true,
  };
  await writeFile(
    ".local/live2d/verification/speech.json",
    JSON.stringify(result, null, 2),
  );
  console.log(JSON.stringify(result, null, 2));
} finally {
  const stop = assistant
    .locator(".speech-settings")
    .getByRole("button", { name: "停止朗读", exact: true });
  if (await stop.isEnabled().catch(() => false))
    await stop.click().catch(() => {});
  await avatar
    .evaluate(() => {
      window.__mouthProbe?.restore();
      delete window.__mouthProbe;
    })
    .catch(() => {});
  await invoke("save_avatar_settings", { value: saved });
  await invoke("hide_assistant");
  await browser.close();
}
