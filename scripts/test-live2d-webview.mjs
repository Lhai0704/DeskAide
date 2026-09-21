// Attach only to the locally launched DeskAide debug WebView2 endpoint.
import { chromium } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
const browser = await chromium.connectOverCDP("http://127.0.0.1:9227");
const pages = browser.contexts().flatMap((c) => c.pages());
let avatar;
for (const page of pages) {
  const label = await page
    .evaluate(() => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label)
    .catch(() => null);
  if (label === "avatar") avatar = page;
}
if (!avatar) throw Error("DeskAide avatar WebView not found");
const invoke = (command, args = {}) =>
  avatar.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
const settings = await invoke("get_avatar_settings");
const errors = [];
avatar.on("pageerror", (e) => errors.push(String(e)));
try {
  const list = await invoke("list_local_avatar_packs");
  if (!list.runtimeReady) throw Error("Runtime missing");
  await invoke("save_avatar_settings", {
    value: { ...settings, packId: "local-haru" },
  });
  await avatar.waitForSelector("canvas");
  await avatar.waitForFunction(() => !document.querySelector(".fallback"), {
    timeout: 20000,
  });
  await new Promise((r) => setTimeout(r, 2500));
  const cursor = await invoke("sample_avatar_cursor");
  const dimensions = await avatar.evaluate(() => ({
    width: innerWidth,
    height: innerHeight,
    dpr: devicePixelRatio,
    canvas: !!document.querySelector("canvas"),
  }));
  await mkdir(".local/live2d/verification", { recursive: true });
  await avatar.screenshot({
    path: ".local/live2d/verification/webview-haru.png",
    omitBackground: true,
  });
  await avatar.evaluate(() => {
    window.__testContext = document
      .querySelector("canvas")
      .getContext("webgl2")
      .getExtension("WEBGL_lose_context");
    window.__testContext.loseContext();
  });
  await avatar.waitForSelector(".fallback");
  await avatar.evaluate(() => window.__testContext.restoreContext());
  await avatar.waitForFunction(() => !document.querySelector(".fallback"), {
    timeout: 20000,
  });
  await avatar.locator("button.avatar").click();
  await avatar.waitForFunction(
    () => document.querySelector("button.avatar").dataset.state === "activated",
  );
  await avatar.waitForFunction(
    () => document.querySelector("button.avatar").dataset.state === "idle",
  );
  const profiles = await invoke("get_assistant_bootstrap");
  const mock = profiles.modelProfiles.find((p) => p.providerType === "mock");
  if (!mock) throw Error("Offline Mock profile required for lifecycle smoke");
  if (await invoke("get_active_turn_snapshot"))
    throw Error("Do not run smoke during a user turn");
  const conversationId = crypto.randomUUID(),
    turnId = crypto.randomUUID();
  try {
    await invoke("set_active_model_profile", { profileId: mock.id });
    await invoke("submit_turn", {
      input: {
        conversationId,
        turnId,
        expectedRevision: 0,
        prompt: "Local avatar lifecycle test.",
        contextDrafts: [],
      },
    });
    await avatar.waitForFunction(
      () =>
        document.querySelector("button.avatar").dataset.state === "responding",
    );
    await avatar.waitForFunction(
      () => document.querySelector("button.avatar").dataset.state === "idle",
    );
    const terminal = await invoke("get_turn_snapshot", { turnId });
    if (terminal?.phase !== "terminal" || terminal?.status !== "completed")
      throw Error("Turn did not complete");
  } finally {
    await invoke("cancel_turn", { turnId });
    await invoke("delete_conversation", { conversationId });
    await invoke("set_active_model_profile", {
      profileId: profiles.activeModelProfileId,
    });
    await invoke("hide_assistant");
  }
  await invoke("save_avatar_settings", {
    value: { ...settings, packId: "local-ren" },
  });
  await new Promise((r) => setTimeout(r, 3000));
  if (await avatar.locator(".fallback").count())
    throw Error(await avatar.locator(".fallback").getAttribute("title"));
  await avatar.screenshot({
    path: ".local/live2d/verification/webview-ren.png",
    omitBackground: true,
  });
  await invoke("save_avatar_settings", {
    value: { ...settings, packId: "default-assistant" },
  });
  await avatar.waitForSelector("img.avatar-media");
  if (errors.length) throw Error(errors.join("\n"));
  const result = {
    webview2: await browser.version(),
    dimensions,
    cursor,
    models: ["Haru", "Ren"],
    restoredStatic: true,
    contextRecovery: true,
    clickActivation: true,
    offlineRuntimeResponding: true,
  };
  await writeFile(
    ".local/live2d/verification/webview.json",
    JSON.stringify(result, null, 2),
  );
  console.log(JSON.stringify(result, null, 2));
} finally {
  await invoke("save_avatar_settings", { value: settings });
  await browser.close();
}
