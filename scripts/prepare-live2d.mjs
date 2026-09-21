import { build } from "esbuild";
import { mkdir, copyFile, readFile, writeFile, cp } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";
const sdk = resolve(process.argv[2] ?? ".local/live2d/sdk");
const output = resolve(".local/live2d/runtime");
const frameworkVersion = (
  await readFile(join(sdk, "Framework/CHANGELOG.md"), "utf8")
).match(/^## \[([^\]]+)\]/m)?.[1];
if (frameworkVersion !== "5-r.5")
  throw Error(`Expected official Framework 5-r.5, found ${frameworkVersion}`);
await mkdir(output, { recursive: true });
const core = process.argv[3]
  ? resolve(process.argv[3])
  : join(sdk, "Core/live2dcubismcore.min.js");
if (core !== join(output, "live2dcubismcore.min.js"))
  await copyFile(core, join(output, "live2dcubismcore.min.js"));
await build({
  entryPoints: ["scripts/live2d/bridge.js"],
  bundle: true,
  format: "esm",
  platform: "browser",
  target: "es2022",
  outfile: join(output, "bridge.js"),
  alias: { "@framework": join(sdk, "Framework/src") },
  legalComments: "eof",
});
await cp(join(sdk, "LICENSE.md"), join(output, "SDK-LICENSE.md"));
await cp(
  join(sdk, "Framework/LICENSE.md"),
  join(output, "FRAMEWORK-LICENSE.md"),
);
await cp(join(sdk, "Core/LICENSE.md"), join(output, "CORE-LICENSE.md"));
await cp(join(sdk, "Framework/Shaders/WebGL"), join(output, "shaders"), {
  recursive: true,
});
const hashes = {};
for (const name of ["bridge.js", "live2dcubismcore.min.js"])
  hashes[name] = createHash("sha256")
    .update(await readFile(join(output, name)))
    .digest("hex");
await writeFile(
  join(output, "provenance.json"),
  JSON.stringify(
    {
      framework: frameworkVersion,
      sdkSource: "https://github.com/Live2D/CubismWebSamples/tree/5-r.5",
      coreSource: "https://www.live2d.com/en/sdk/download/web/",
      hashes,
    },
    null,
    2,
  ),
);
console.log("Local runtime prepared:", output);
