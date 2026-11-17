#![cfg(feature = "reference-data")]

use std::{fs, path::Path};

use gaeb_core::{Document, ExchangePhase, ParseOptions, SourceVersion, parse_bytes};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/official");

#[test]
fn downloaded_official_and_bvbs_fixtures_parse_with_declared_phase() {
    let fixtures = [
        (
            "bvbs-33-ava-x81/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X81",
            SourceVersion::DaXml33,
            ExchangePhase::X81,
        ),
        (
            "bvbs-33-ava-x84/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X84",
            SourceVersion::DaXml33,
            ExchangePhase::X84,
        ),
        (
            "bvbs-33-ava-x86/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X86",
            SourceVersion::DaXml33,
            ExchangePhase::X86,
        ),
        (
            "bvbs-33-construction-x83/Pruefdatei GAEB DA XML 3.3 - Bauausfuehrung - V 04 04 2024.x83",
            SourceVersion::DaXml33,
            ExchangePhase::X83,
        ),
        (
            "bvbs-33-construction-x84/BVBS_Pruefdatei GAEB DA XML 3.3 - Bauausfuehrung - V 11 06 2021.x84",
            SourceVersion::DaXml33,
            ExchangePhase::X84,
        ),
        (
            "bvbs-33-quantity-x31/Pruefdatei GAEB DA XML 3.3 - Mengenermittlung - V 11 06 2021.x31",
            SourceVersion::DaXml33,
            ExchangePhase::X31,
        ),
        (
            "bvbs-33-text-x82/GAEB Pruefdatei GAEB DA XML 3.3 - Texterstellung V 11 06 2021.x82",
            SourceVersion::DaXml33,
            ExchangePhase::X82,
        ),
        (
            "gaeb-32-direct-samples/Beispiel_01_31_3.2_2013-07-31.x31.xml",
            SourceVersion::DaXml32,
            ExchangePhase::X31,
        ),
        (
            "gaeb-32-direct-samples/Beispiel_01_83Z_3.2_2013-09-02.X83Z.xml",
            SourceVersion::DaXml32,
            ExchangePhase::X83Z,
        ),
        (
            "gaeb-32-direct-samples/Beispiel_01_84Z_3.2_2013-09-02.X84Z.xml",
            SourceVersion::DaXml32,
            ExchangePhase::X84Z,
        ),
        (
            "gaeb-32-direct-samples/Beispiel_01_86ZR_3.2_2013-09-02.X86ZR.xml",
            SourceVersion::DaXml32,
            ExchangePhase::X86ZR,
        ),
        (
            "gaeb-32-direct-samples/Beispiel_01_86ZE_3.2_2013-09-02.X86ZE.xml",
            SourceVersion::DaXml32,
            ExchangePhase::X86ZE,
        ),
        (
            "gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_83Z_3.2_2014-03-24.X83Z",
            SourceVersion::DaXml32,
            ExchangePhase::X83Z,
        ),
        (
            "gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_84Z_3.2_2014-03-24.X84Z",
            SourceVersion::DaXml32,
            ExchangePhase::X84Z,
        ),
        (
            "gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_86ZR_3.2_2014-03-24.X86ZR",
            SourceVersion::DaXml32,
            ExchangePhase::X86ZR,
        ),
        (
            "gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_86ZE_3.2_2014-03-24.X86ZE",
            SourceVersion::DaXml32,
            ExchangePhase::X86ZE,
        ),
        (
            "gaeb-31-2007-11-samples/Musterdatei_3.1_2008-001.X81",
            SourceVersion::DaXml31,
            ExchangePhase::X81,
        ),
        (
            "gaeb-31-2007-11-samples/Musterdatei_3.1_2008-001.X83",
            SourceVersion::DaXml31,
            ExchangePhase::X83,
        ),
        (
            "gaeb-31-2007-11-samples/Musterdatei_3.1_2008-001.X84",
            SourceVersion::DaXml31,
            ExchangePhase::X84,
        ),
        (
            "gaeb-31-2007-11-samples/Musterdatei_3.1_2008-001.X86",
            SourceVersion::DaXml31,
            ExchangePhase::X86,
        ),
    ];

    for (relative, version, phase) in fixtures {
        let path = Path::new(ROOT).join(relative);
        assert!(
            path.exists(),
            "missing official fixture: {}",
            path.display()
        );
        let document = parse_bytes(&fs::read(&path).unwrap(), &ParseOptions::default())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(document.source_version(), version, "{}", path.display());
        assert_eq!(document.phase(), phase, "{}", path.display());
        assert_eq!(document.kind(), phase.kind(), "{}", path.display());
        assert!(
            document.items().count() > 0,
            "{} has no extracted items",
            path.display()
        );
    }
}

#[test]
fn official_beta_spatial_fixture_round_trips_as_structured_content() {
    let path =
        Path::new(ROOT).join("gaeb-32-beta-research/Beispiel_01_61_3.2_2013-07-31Bungalow.x61.xml");
    let document = parse_bytes(&fs::read(path).unwrap(), &ParseOptions::default()).unwrap();
    let Document::Structured(document) = document else {
        panic!("structured document expected");
    };
    assert_eq!(document.meta.phase, ExchangePhase::X61);
    assert!(
        document
            .elements
            .iter()
            .any(|element| element.name == "SpatialHierarchy")
    );
}

#[test]
fn every_supported_official_xml_fixture_parses() {
    fn visit(path: &Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                if path.ends_with("gaeb-32-beta-research") {
                    continue;
                }
                visit(&path, files);
            } else if path.extension().is_some_and(|extension| {
                let extension = extension.to_string_lossy().to_ascii_lowercase();
                extension.starts_with('x') && extension != "xsd"
            }) {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    visit(Path::new(ROOT), &mut files);
    assert!(!files.is_empty(), "official corpus has no XML fixtures");
    for path in files {
        let document = parse_bytes(&fs::read(&path).unwrap(), &ParseOptions::default())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            document.items().count() > 0,
            "{} has no items",
            path.display()
        );
    }
}
