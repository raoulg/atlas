import { expect, it } from "vitest";

import { kies } from "./api";

it("kiest kort", () => {
  expect(kies({ reden: "x" }, "kort")).toBe("kort");
});
