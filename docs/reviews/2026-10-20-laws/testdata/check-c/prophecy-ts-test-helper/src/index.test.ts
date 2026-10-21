import { seal } from "./index.js";

function sealedTwice(r: string): string {
  return seal(seal(r));
}

test("seal keeps the report", () => {
  expect(seal("r")).toBe("r");
  expect(sealedTwice("r")).toBe("r");
});
