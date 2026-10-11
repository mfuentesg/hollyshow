//! Checks on the Bibles shipped in `apps/desktop/src-tauri/resources/bibles`.

use hollyshow_bible::import::HsbImporter;
use hollyshow_bible::BibleImporter;
use hollyshow_bible::{
    build_bible_slide, canon_report, install_bundled, BibleIndex, BibleSink, ImportedBible, Parser,
    ReferenceParser, VersionText,
};
use hollyshow_core::{BibleRef, BibleVersion, ParallelLayout, Passage, Verse, VersionCode};
use hollyshow_storage_api::memory::MemoryStore;
use hollyshow_storage_api::BibleRepo;
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src-tauri/resources/bibles")
}

fn load(file: &str) -> ImportedBible {
    let bytes = std::fs::read(dir().join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    HsbImporter::default().import(&bytes).unwrap()
}

fn verse<'a>(b: &'a ImportedBible, osis: &str) -> &'a Verse {
    let r: BibleRef = osis.parse().unwrap();
    b.verses
        .iter()
        .find(|v| v.reference == r)
        .unwrap_or_else(|| panic!("{osis} missing"))
}

#[test]
fn kjv_has_exactly_the_31102_standard_verses() {
    let b = load("kjv.hsb.gz");
    assert_eq!(b.verses.len(), 31_102);
    let report = canon_report(&b.verses);
    assert!(report.is_complete(), "missing: {:?}", report.missing);
    assert!(report.extra.is_empty());
    assert_eq!(b.version.code.0, "KJV");
    assert_eq!(b.version.language, "en");
    assert!(b.version.is_public_domain);
    assert_eq!(
        verse(&b, "John.3.16").text,
        "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life."
    );
    assert!(verse(&b, "Acts.8.37").text.starts_with("And Philip said"));
}

#[test]
fn spanish_versions_are_complete_up_to_documented_blank_slots() {
    let rvr = load("rvr1909.hsb.gz");
    let report = canon_report(&rvr.verses);
    // The source keeps 18 slots empty where Reina-Valera 1909 numbers or merges verses
    // differently (see SOURCES.md). Nothing else may be missing.
    assert_eq!(report.missing.len(), 18, "{:?}", report.missing);
    assert!(report.extra.is_empty());
    assert_eq!(rvr.verses.len(), 31_102 - 18);
    assert_eq!(rvr.version.language, "es");
    assert!(rvr.version.is_public_domain);
    assert!(verse(&rvr, "John.3.16")
        .text
        .starts_with("Porque de tal manera am"));
    assert!(verse(&rvr, "Ps.23.1").text.contains("pastor"));

    let rv = load("rv1865.hsb.gz");
    assert_eq!(rv.verses.len(), 31_102 - 4);
    assert_eq!(rv.version.code.0, "RV1865");
}

#[test]
fn bundled_files_are_in_canonical_order_and_free_of_markup() {
    for f in ["kjv.hsb.gz", "rvr1909.hsb.gz", "rv1865.hsb.gz"] {
        let b = load(f);
        let keys: Vec<_> = b
            .verses
            .iter()
            .map(|v| hollyshow_bible::canonical_key(&v.reference))
            .collect();
        assert!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "{f} not sorted/unique"
        );
        for v in &b.verses {
            assert!(
                !v.text.contains(['<', '>', '\t', '\n']),
                "{f} {}",
                v.reference
            );
            assert_eq!(v.text, v.text.trim());
        }
    }
}

#[test]
fn full_text_search_per_version() {
    let kjv = BibleIndex::build(VersionCode("KJV".into()), load("kjv.hsb.gz").verses);
    let hits = kjv.search("\"only begotten\" son", 50);
    assert!(hits.iter().any(|h| h.reference.to_string() == "John.3.16"));
    let rvr = BibleIndex::build(VersionCode("RVR1909".into()), load("rvr1909.hsb.gz").verses);
    // Accent-insensitive: "amo" finds "amó".
    let hits = rvr.search("tal manera amo dios mundo", 10);
    assert_eq!(
        hits.first().map(|h| h.reference.to_string()).as_deref(),
        Some("John.3.16")
    );
}

#[test]
fn parser_passages_resolve_in_every_bundled_version() {
    let versions = [load("rvr1909.hsb.gz"), load("kjv.hsb.gz")];
    let p = Parser.parse("Jn 3:16-18", "es").unwrap();
    for b in &versions {
        let n = b
            .verses
            .iter()
            .filter(|v| v.reference.book.0 == "John" && v.reference.chapter == 3)
            .filter(|v| (16..=18).contains(&v.reference.verse.unwrap()))
            .count();
        assert_eq!(n, 3, "{}", b.version.code.0);
    }
    assert_eq!(p.to_osis(), "John.3.16-John.3.18");
}

#[derive(Default)]
struct StoreSink(MemoryStore);
impl BibleSink for StoreSink {
    fn installed(&self) -> Result<Vec<VersionCode>, String> {
        Ok(self
            .0
            .list_versions()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|v| v.code)
            .collect())
    }
    fn install(&self, v: &BibleVersion, verses: &[Verse]) -> Result<(), String> {
        self.0.install_version(v.clone(), verses.to_vec());
        Ok(())
    }
}

#[test]
fn first_run_installs_everything_through_the_storage_traits() {
    let sink = StoreSink::default();
    let report = install_bundled(&dir(), &sink).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        report
            .installed
            .iter()
            .map(|c| c.0.as_str())
            .collect::<Vec<_>>(),
        ["RVR1909", "KJV", "RV1865"]
    );
    let versions = sink.0.list_versions().unwrap();
    assert_eq!(versions.len(), 3);
    assert!(
        versions
            .iter()
            .find(|v| v.code.0 == "RVR1909")
            .unwrap()
            .is_default
    );
    assert!(
        versions
            .iter()
            .find(|v| v.code.0 == "KJV")
            .unwrap()
            .is_secondary
    );

    // Read back through BibleRepo and build a two-column slide.
    let passage = Passage::parse_osis("John.3.16").unwrap();
    let rvr = versions.iter().find(|v| v.code.0 == "RVR1909").unwrap();
    let kjv = versions.iter().find(|v| v.code.0 == "KJV").unwrap();
    let a = sink.0.passage(&rvr.code, &passage).unwrap();
    let b = sink.0.passage(&kjv.code, &passage).unwrap();
    assert_eq!((a.len(), b.len()), (1, 1));
    let slide = build_bible_slide(
        &passage,
        ParallelLayout::Columns,
        &[
            VersionText {
                version: rvr,
                verses: &a,
            },
            VersionText {
                version: kjv,
                verses: &b,
            },
        ],
    );
    assert_eq!(slide.bible[0].reference, "Juan 3:16");
    assert_eq!(slide.bible[1].reference, "John 3:16");
    assert!(slide.bible[1].text.contains("only begotten"));

    // Idempotent.
    let again = install_bundled(&dir(), &sink).unwrap();
    assert!(again.installed.is_empty());
    assert_eq!(again.already_present.len(), 3);
}
