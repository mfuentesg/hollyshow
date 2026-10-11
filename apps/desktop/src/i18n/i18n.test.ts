import { describe, expect, it } from "vitest";
import en from "./en.json";
import es from "./es.json";

function keys(o: object, prefix = ""): string[] {
  return Object.entries(o).flatMap(([k, v]) =>
    typeof v === "object" && v !== null ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
  );
}

describe("i18n catalogs", () => {
  it("es and en define the same keys", () => {
    expect(keys(en).sort()).toEqual(keys(es).sort());
  });
});
