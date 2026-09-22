// Local diagnostic. Sends only "测试" in a fresh conversation; never exports keys or reply text.
import { chromium } from "@playwright/test";
import { writeFile } from "node:fs/promises";
const browser = await chromium.connectOverCDP("http://127.0.0.1:9227");
let assistant, avatar;
for (const page of browser.contexts().flatMap((c) => c.pages())) {
  const label = await page.evaluate(
    () => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label,
  );
  if (label === "assistant") assistant = page;
  if (label === "avatar") avatar = page;
}
if (!assistant || !avatar) throw Error("Debug windows unavailable");
const invoke = (command, args = {}) =>
  assistant.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
if (await invoke("get_active_turn_snapshot"))
  throw Error("A turn is already running");
if (
  (await assistant.locator("article.message").count()) ||
  (await assistant.getByLabel("问题", { exact: true }).inputValue())
)
  throw Error("Diagnostic requires a blank conversation");
let turnId, conversationId;
try {
  const bootstrap = await invoke("get_assistant_bootstrap");
  const profile = bootstrap.modelProfiles.find(
    (p) => p.id === bootstrap.activeModelProfileId,
  );
  await invoke("toggle_assistant");
  await assistant.getByLabel("问题", { exact: true }).fill("测试");
  const start = performance.now();
  await assistant.getByRole("button", { name: "发送", exact: true }).click();
  const phases = {};
  let firstTextMs;
  while (performance.now() - start < 90000) {
    const p = await invoke("get_avatar_presentation");
    if (p.turnId) {
      turnId = p.turnId;
      phases[p.phase] ??= Math.round(performance.now() - start);
    }
    if (
      firstTextMs === undefined &&
      (await assistant.locator(".assistant-message p").count())
    )
      firstTextMs = Math.round(performance.now() - start);
    if (turnId && p.phase === "terminal") break;
    await new Promise((r) => setTimeout(r, 20));
  }
  if (!turnId) throw Error("No submitted turn");
  const snapshot = await invoke("get_turn_snapshot", { turnId });
  conversationId = snapshot?.conversationId;
  if (!firstTextMs)
    throw Error(`No text arrived; phase timings ${JSON.stringify(phases)}`);
  const result = {
    model: profile.modelId,
    stream: profile.capabilities.supportsStreaming,
    tools: profile.capabilities.supportsTools,
    firstVisibleTextMs: firstTextMs,
    elapsedMs: Math.round(performance.now() - start),
    phases,
    status: snapshot?.status,
  };
  await writeFile(
    ".local/live2d/verification/response-latency.json",
    JSON.stringify(result, null, 2),
  );
  console.log(JSON.stringify(result, null, 2));
} finally {
  if (turnId) {
    await invoke("cancel_turn", { turnId });
    if (!conversationId)
      conversationId = (await invoke("get_turn_snapshot", { turnId }))
        ?.conversationId;
  }
  if (conversationId) await invoke("delete_conversation", { conversationId });
  await assistant
    .getByTitle("新建会话", { exact: true })
    .click()
    .catch(() => {});
  await invoke("hide_assistant");
  await browser.close();
}
