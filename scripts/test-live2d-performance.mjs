// Development measurement against the owned debug process; no production telemetry.
import { chromium } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
const executable = resolve("target/debug/deskaide.exe").replaceAll("'", "''");
const pid = Number(
  await readFile(".local/live2d/verification-process.txt", "utf8"),
);
if (!Number.isInteger(pid) || pid <= 0) throw Error("Invalid verification PID");
const snapshot = () =>
  JSON.parse(
    execFileSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        `
$rootProcess=Get-Process -Id ${pid};
if($rootProcess.Path -ne '${executable}'){throw 'Unexpected process'};
$all=Get-CimInstance Win32_Process;
$ids=[System.Collections.Generic.HashSet[int]]::new();[void]$ids.Add(${pid});
do {$changed=$false;foreach($p in $all){if($ids.Contains([int]$p.ParentProcessId)){if($ids.Add([int]$p.ProcessId)){$changed=$true}}}}while($changed);
$processes=Get-Process -Id @($ids) -ErrorAction SilentlyContinue;
@{cpu=($processes|Measure-Object CPU -Sum).Sum;workingSet=($processes|Measure-Object WorkingSet64 -Sum).Sum;processes=$processes.Count}|ConvertTo-Json -Compress
`,
      ],
      { encoding: "utf8", windowsHide: true },
    ),
  );
const browser = await chromium.connectOverCDP("http://127.0.0.1:9227");
let avatar;
for (const p of browser.contexts().flatMap((c) => c.pages()))
  if (
    (await p.evaluate(
      () => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label,
    )) === "avatar"
  )
    avatar = p;
if (!avatar) throw Error("Avatar missing");
const invoke = (command, args = {}) =>
  avatar.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
const settings = await invoke("get_avatar_settings");
const metrics = await avatar.context().newCDPSession(avatar);
await metrics.send("Performance.enable");
const rows = [];
try {
  for (const [name, packId, off] of [
    ["static", "default-assistant", false],
    ["live2d-idle-tracking", "local-haru", false],
    ["live2d-animation-off", "local-haru", true],
  ]) {
    await invoke("save_avatar_settings", {
      value: {
        ...settings,
        packId,
        preferences: {
          ...settings.preferences,
          ...(off
            ? {
                "local-haru": {
                  mouseTracking: false,
                  idleAnimation: false,
                  autoBlink: false,
                  motions: false,
                  scale: 1,
                  verticalPosition: 0,
                },
              }
            : {}),
        },
      },
    });
    await new Promise((r) => setTimeout(r, 2500));
    const before = snapshot(),
      start = performance.now(),
      a = await metrics.send("Performance.getMetrics");
    await new Promise((r) => setTimeout(r, 5000));
    const b = await metrics.send("Performance.getMetrics"),
      elapsed = (performance.now() - start) / 1000,
      after = snapshot();
    const value = (s, key) => s.metrics.find((m) => m.name === key)?.value ?? 0;
    rows.push({
      name,
      seconds: elapsed,
      processCpuPercentOfOneCore: ((after.cpu - before.cpu) / elapsed) * 100,
      processWorkingSetMiB: after.workingSet / 1048576,
      webviewTaskPercent:
        ((value(b, "TaskDuration") - value(a, "TaskDuration")) / elapsed) * 100,
      jsHeapMiB: value(b, "JSHeapUsedSize") / 1048576,
    });
  }
  await writeFile(
    ".local/live2d/verification/performance.json",
    JSON.stringify(rows, null, 2),
  );
  console.log(JSON.stringify(rows, null, 2));
} finally {
  await invoke("save_avatar_settings", { value: settings });
  await browser.close();
}
