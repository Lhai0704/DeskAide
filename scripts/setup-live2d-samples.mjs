// Copies official SDK samples into ignored local packs; never downloads models.
import { cp, mkdir, writeFile, readFile } from "node:fs/promises";
import { resolve, join } from "node:path";
const sdk = resolve(process.argv[2] ?? ".local/live2d/sdk");
for (const name of ["Haru", "Ren"]) {
  const root = resolve(".local/live2d/packs", name.toLowerCase());
  await mkdir(root, { recursive: true });
  await cp(join(sdk, "Samples/Resources", name), join(root, "model"), {
    recursive: true,
  });
  await cp(
    "apps/desktop/public/avatars/default-assistant/idle.png",
    join(root, "preview.png"),
  );
  await cp(
    join(sdk, "Samples/Resources/Live2DModelsSampleLicense.txt"),
    join(root, "MODEL-LICENSE.txt"),
  ).catch(async () => {
    await writeFile(
      join(root, "MODEL-LICENSE.txt"),
      "Official Live2D sample; local development only. https://www.live2d.com/en/learn/sample/model-terms/\nhttps://www.live2d.com/eula/live2d-free-material-license-agreement_en.html\n",
    );
  });
  const model = JSON.parse(
    await readFile(join(root, "model", `${name}.model3.json`), "utf8"),
  );
  const groups = model.FileReferences.Motions ?? {};
  const idle = groups.Idle ? "Idle" : Object.keys(groups)[0];
  const tap = groups.TapBody ? "TapBody" : idle;
  const expressions = model.FileReferences.Expressions ?? [];
  await writeFile(
    join(root, "manifest.json"),
    JSON.stringify(
      {
        schemaVersion: 3,
        renderer: "live2d",
        id: `local-${name.toLowerCase()}`,
        name: `${name}（官方开发样例）`,
        version: "1.0.0",
        alt: `${name} Live2D 开发形象`,
        preview: "preview.png",
        defaultWidth: 240,
        defaultHeight: 320,
        model: `model/${name}.model3.json`,
        motions: {
          ...(idle
            ? {
                idle: { group: idle, index: 0 },
                responding: {
                  group: idle,
                  index: groups[idle].length > 1 ? 1 : 0,
                },
              }
            : {}),
          ...(tap ? { activated: { group: tap, index: 0 } } : {}),
        },
        expressions: {
          ...(expressions[0] ? { tap: expressions[0].Name } : {}),
          ...(expressions[3] ? { thinking: expressions[3].Name } : {}),
        },
        metadata: {
          author: "Live2D Inc.",
          license: "Official sample terms; local development only",
        },
      },
      null,
      2,
    ),
  );
}
console.log("Official development packs prepared locally.");
