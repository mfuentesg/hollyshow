import { describe, expect, it } from "vitest";
import {
  addArrangement,
  addSection,
  moveInSequence,
  moveSection,
  removeSection,
  songSlides,
} from "./songOps";
import { emptySong } from "./songText";

const mk = () => {
  let s = emptySong("Prueba");
  s = {
    ...s,
    copyright: "PD",
    sections: [{ ...s.sections[0]!, id: "v1", lyrics: "[G]uno\ndos\n---\ntres" }],
    arrangements: [{ id: "a", name: "A", sequence: ["v1"] }],
  };
  return s;
};
const FIXED = mk();
const song = () => FIXED;

describe("song ops", () => {
  it("adds a section to the first arrangement and removes it everywhere", () => {
    const { song: s2, section } = addSection(song(), "chorus");
    expect(s2.arrangements[0]!.sequence).toEqual(["v1", section.id]);
    expect(section.label).toBe("Coro");
    const s3 = removeSection(s2, section.id);
    expect(s3.sections).toHaveLength(1);
    expect(s3.arrangements[0]!.sequence).toEqual(["v1"]);
  });

  it("reorders sections and sequence chips", () => {
    const { song: s2 } = addSection(song(), "chorus");
    expect(moveSection(s2, 0, 1).sections.map((x) => x.id)[1]).toBe("v1");
    expect(moveInSequence(["a", "b", "c"], 0, 2)).toEqual(["b", "c", "a"]);
    const same = ["a"];
    expect(moveInSequence(same, 0, 0)).toBe(same);
  });

  it("new arrangements start from all sections", () => {
    const { song: s2, arrangement } = addArrangement(song(), "Corto");
    expect(arrangement.sequence).toEqual(["v1"]);
    expect(s2.arrangements).toHaveLength(2);
  });

  it("expands slides: chords stripped for lines, kept in chart, copyright on the last", () => {
    const slides = songSlides(song(), { arrangementId: "a", maxLines: 4, copyright: "last" });
    expect(slides).toHaveLength(2);
    expect(slides[0]!.lines).toEqual(["uno", "dos"]);
    expect(slides[0]!.chart[0]).toBe("[G]uno");
    expect(slides[1]!.footer).toBe("PD");
    expect(slides[0]!.footer).toBeNull();
    expect(slides[0]!.sizeGroup).toBe(song().id);
  });

  it("transposes and honors uniform size off", () => {
    const slides = songSlides(song(), {
      arrangementId: null,
      maxLines: 4,
      transpose: 2,
      uniformSize: false,
      copyright: "none",
    });
    expect(slides[0]!.chart[0]).toBe("[A]uno");
    expect(slides[0]!.sizeGroup).toBeNull();
  });
});
