// The main entry runs inside the engine's V8 (spec: "The test clock"): bundled for the browser, it carries no server
// renderer; renderFrame comes from the test entry.
import { expect, test } from "bun:test";

test("@raea/motion's main entry bundles without react-dom/server", async () => {
  const out = await Bun.build({ entrypoints: [`${import.meta.dir}/../src/index.tsx`], target: "browser", external: ["react"] });
  expect(out.success).toBe(true);
  const code = await out.outputs[0].text();
  expect(code).not.toContain("renderToStaticMarkup");
  expect(code).not.toContain("react-dom/server");
});

test("the test entry has renderFrame", async () => {
  const t = await import("../src/test");
  expect(typeof t.renderFrame).toBe("function");
});
