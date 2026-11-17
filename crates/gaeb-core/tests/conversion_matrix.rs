#![cfg(feature = "reference-data")]

use std::{fs, path::Path, process::Command};

use gaeb_core::{
    Document, DocumentItem, DocumentKind, ExchangePhase, ParseOptions, SourceVersion, WriteOptions,
    parse_bytes,
};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/official");

const OFFICIAL_FIXTURES: &[&str] = &[
    "gaeb-31-2007-11-samples/Musterdatei_3.1_2008-001.X81",
    "gaeb-31-2007-11-samples/Musterdatei_3.1_2008-001.X83",
    "gaeb-31-2007-11-samples/Musterdatei_3.1_2008-001.X84",
    "gaeb-31-2007-11-samples/Musterdatei_3.1_2008-001.X86",
    "bvbs-33-ava-x81/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X81",
    "bvbs-33-ava-x84/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X84",
    "bvbs-33-ava-x86/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X86",
    "bvbs-33-construction-x83/Pruefdatei GAEB DA XML 3.3 - Bauausfuehrung - V 04 04 2024.x83",
    "bvbs-33-construction-x84/BVBS_Pruefdatei GAEB DA XML 3.3 - Bauausfuehrung - V 11 06 2021.x84",
    "bvbs-33-quantity-x31/Pruefdatei GAEB DA XML 3.3 - Mengenermittlung - V 11 06 2021.x31",
    "bvbs-33-quantity-x86/Pruefdatei GAEB DA XML 3.3 - Mengenermittlung - V 11 06 2021.x86",
    "bvbs-33-text-x81/GAEB Pruefdatei GAEB DA XML 3.3 - Texterstellung V 11 06 2021.x81",
    "bvbs-33-text-x82/GAEB Pruefdatei GAEB DA XML 3.3 - Texterstellung V 11 06 2021.x82",
    "gaeb-32-direct-samples/Beispiel_01_31_3.2_2013-07-31.x31.xml",
    "gaeb-32-direct-samples/Beispiel_01_83Z_3.2_2013-09-02.X83Z.xml",
    "gaeb-32-direct-samples/Beispiel_01_84Z_3.2_2013-09-02.X84Z.xml",
    "gaeb-32-direct-samples/Beispiel_01_86ZE_3.2_2013-09-02.X86ZE.xml",
    "gaeb-32-direct-samples/Beispiel_01_86ZR_3.2_2013-09-02.X86ZR.xml",
    "gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_83Z_3.2_2014-03-24.X83Z",
    "gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_84Z_3.2_2014-03-24.X84Z",
    "gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_86ZR_3.2_2014-03-24.X86ZR",
    "gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_86ZE_3.2_2014-03-24.X86ZE",
];

const XML_VERSIONS: &[SourceVersion] = &[
    SourceVersion::DaXml20,
    SourceVersion::DaXml21,
    SourceVersion::DaXml30,
    SourceVersion::DaXml31,
    SourceVersion::DaXml32,
    SourceVersion::DaXml33,
];

const PROCUREMENT_PHASES: &[ExchangePhase] = &[
    ExchangePhase::X80,
    ExchangePhase::X81,
    ExchangePhase::X82,
    ExchangePhase::X83,
    ExchangePhase::X84,
    ExchangePhase::X85,
    ExchangePhase::X86,
    ExchangePhase::X87,
    ExchangePhase::X83Z,
    ExchangePhase::X84Z,
    ExchangePhase::X86ZR,
    ExchangePhase::X86ZE,
];

const PROCUREMENT_PHASES_WITH_X88: &[ExchangePhase] = &[
    ExchangePhase::X80,
    ExchangePhase::X81,
    ExchangePhase::X82,
    ExchangePhase::X83,
    ExchangePhase::X84,
    ExchangePhase::X85,
    ExchangePhase::X86,
    ExchangePhase::X87,
    ExchangePhase::X88,
    ExchangePhase::X83Z,
    ExchangePhase::X84Z,
    ExchangePhase::X86ZR,
    ExchangePhase::X86ZE,
];

#[test]
fn every_official_document_converts_across_xml_versions_and_same_kind_phases() {
    for relative in OFFICIAL_FIXTURES {
        let path = Path::new(ROOT).join(relative);
        assert!(
            path.exists(),
            "missing official fixture: {}",
            path.display()
        );
        let original = parse_bytes(&fs::read(&path).unwrap(), &ParseOptions::default())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        for &version in XML_VERSIONS {
            let phases = phases_for(original.kind(), version);
            for &phase in phases {
                convert_and_compare(&original, version, phase, &path);
            }
        }
        if original.kind() == DocumentKind::Procurement {
            convert_and_compare(&original, SourceVersion::Gaeb90, ExchangePhase::X83, &path);
        }
    }
}

fn phases_for(kind: DocumentKind, version: SourceVersion) -> &'static [ExchangePhase] {
    match kind {
        DocumentKind::Procurement => {
            if matches!(version, SourceVersion::DaXml32 | SourceVersion::DaXml33) {
                PROCUREMENT_PHASES
            } else {
                PROCUREMENT_PHASES_WITH_X88
            }
        }
        DocumentKind::Trade => &[
            ExchangePhase::X93,
            ExchangePhase::X94,
            ExchangePhase::X96,
            ExchangePhase::X97,
        ],
        DocumentKind::Cost => &[ExchangePhase::X50, ExchangePhase::X51],
        DocumentKind::Quantity => &[ExchangePhase::X31],
        DocumentKind::Structured => &[],
    }
}

fn convert_and_compare(
    original: &Document,
    version: SourceVersion,
    phase: ExchangePhase,
    path: &Path,
) {
    let mut output = Vec::new();
    let report = original
        .write(
            &mut output,
            &WriteOptions {
                version,
                phase,
                indent: false,
            },
        )
        .unwrap_or_else(|error| {
            panic!(
                "{} -> {} {} write failed: {error}",
                path.display(),
                version.number(),
                phase.code()
            )
        });
    assert!(
        report
            .omitted_fields
            .iter()
            .all(|field| !field.trim().is_empty()),
        "{} -> {} {} contains blank omission",
        path.display(),
        version.number(),
        phase.code()
    );
    let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap_or_else(|error| {
        panic!(
            "{} -> {} {} reparse failed: {error}\n{}",
            path.display(),
            version.number(),
            phase.code(),
            String::from_utf8_lossy(&output)
        )
    });
    if version == SourceVersion::DaXml33 {
        assert_output_valid_against_official_schema(&output, phase, path);
    }
    assert_eq!(reparsed.kind(), original.kind(), "{}", path.display());
    assert_eq!(reparsed.phase(), phase, "{}", path.display());
    assert_core_fields_preserved(original, &reparsed, &report, path, version, phase);
}

fn assert_output_valid_against_official_schema(output: &[u8], phase: ExchangePhase, path: &Path) {
    let (schema_dir, schema) = schema_for_phase(phase).unwrap_or_else(|| {
        panic!(
            "no official XSD mapping configured for DA XML 3.3 phase {}",
            phase.code()
        )
    });
    let output_path = std::env::temp_dir().join(format!(
        "gaeb-core-conversion-{}-{}.xml",
        std::process::id(),
        phase.code()
    ));
    fs::write(&output_path, output).unwrap();
    let result = Command::new("xmllint")
        .current_dir(Path::new(ROOT).join(schema_dir))
        .args(["--noout", "--schema", schema, output_path.to_str().unwrap()])
        .output()
        .unwrap();
    let _ = fs::remove_file(&output_path);
    assert!(
        result.status.success(),
        "{} -> 3.3 {} failed official XSD validation: {}",
        path.display(),
        phase.code(),
        String::from_utf8_lossy(&result.stderr)
    );
}

fn schema_for_phase(phase: ExchangePhase) -> Option<(&'static str, &'static str)> {
    match phase {
        ExchangePhase::X31 => Some(("gaeb-33-quantity-2023-01", "GAEB_DA_XML_31_3.3_2023-01.xsd")),
        ExchangePhase::X52 => Some(("gaeb-33-cost-2021-05", "GAEB_DA_XML_52_3.3_2021-05.xsd")),
        ExchangePhase::X80
        | ExchangePhase::X81
        | ExchangePhase::X82
        | ExchangePhase::X83
        | ExchangePhase::X84
        | ExchangePhase::X85
        | ExchangePhase::X86
        | ExchangePhase::X87 => Some((
            "gaeb-33-lv-2021-05",
            match phase {
                ExchangePhase::X80 => "GAEB_DA_XML_80_3.3_2021-05.xsd",
                ExchangePhase::X81 => "GAEB_DA_XML_81_3.3_2021-05.xsd",
                ExchangePhase::X82 => "GAEB_DA_XML_82_3.3_2021-05.xsd",
                ExchangePhase::X83 => "GAEB_DA_XML_83_3.3_2021-05.xsd",
                ExchangePhase::X84 => "GAEB_DA_XML_84_3.3_2021-05.xsd",
                ExchangePhase::X85 => "GAEB_DA_XML_85_3.3_2021-05.xsd",
                ExchangePhase::X86 => "GAEB_DA_XML_86_3.3_2021-05.xsd",
                ExchangePhase::X87 => "GAEB_DA_XML_87_3.3_2021-05.xsd",
                _ => unreachable!(),
            },
        )),
        ExchangePhase::X89 => Some(("gaeb-33-invoice-2021-05", "GAEB_DA_XML_89_3.3_2021-05.xsd")),
        ExchangePhase::X89B => Some(("gaeb-33-invoice-2021-05", "GAEB_DA_XML_89B_3.3_2021-05.xsd")),
        ExchangePhase::X83Z => Some((
            "gaeb-33-time-contract-2021-05",
            "GAEB_DA_XML_83Z_3.3_2021-05.xsd",
        )),
        ExchangePhase::X84Z => Some((
            "gaeb-33-time-contract-2021-05",
            "GAEB_DA_XML_84Z_3.3_2021-05.xsd",
        )),
        ExchangePhase::X86ZR => Some((
            "gaeb-33-time-contract-2021-05",
            "GAEB_DA_XML_86ZR_3.3_2021-05.xsd",
        )),
        ExchangePhase::X86ZE => Some((
            "gaeb-33-time-contract-2021-05",
            "GAEB_DA_XML_86ZE_3.3_2021-05.xsd",
        )),
        ExchangePhase::X93 => Some(("gaeb-33-trade-2021-05", "GAEB_DA_XML_93_3.3_2021-05.xsd")),
        ExchangePhase::X94 => Some(("gaeb-33-trade-2021-05", "GAEB_DA_XML_94_3.3_2021-05.xsd")),
        ExchangePhase::X96 => Some(("gaeb-33-trade-2021-05", "GAEB_DA_XML_96_3.3_2021-05.xsd")),
        ExchangePhase::X97 => Some(("gaeb-33-trade-2021-05", "GAEB_DA_XML_97_3.3_2021-05.xsd")),
        ExchangePhase::X50
        | ExchangePhase::X51
        | ExchangePhase::X88
        | ExchangePhase::X95
        | ExchangePhase::X32
        | ExchangePhase::X33
        | ExchangePhase::X34
        | ExchangePhase::X35
        | ExchangePhase::X36
        | ExchangePhase::X37
        | ExchangePhase::X38
        | ExchangePhase::X39
        | ExchangePhase::X60
        | ExchangePhase::X61
        | ExchangePhase::X62
        | ExchangePhase::X63
        | ExchangePhase::X64
        | ExchangePhase::X65
        | ExchangePhase::X66
        | ExchangePhase::X67
        | ExchangePhase::X68
        | ExchangePhase::X69 => None,
    }
}

fn assert_core_fields_preserved(
    original: &Document,
    reparsed: &Document,
    report: &gaeb_core::WriteReport,
    path: &Path,
    version: SourceVersion,
    phase: ExchangePhase,
) {
    let before: Vec<_> = original.items().collect();
    let after: Vec<_> = reparsed.items().collect();
    assert_eq!(before.len(), after.len(), "{}", path.display());
    for (index, (before, after)) in before.into_iter().zip(after).enumerate() {
        match (before, after) {
            (DocumentItem::Procurement(before), DocumentItem::Procurement(after)) => {
                assert_eq!(before.oz, after.oz, "{path:?} item {index}");
                if !before.short_text.is_empty() && before.short_text != after.short_text {
                    assert!(
                        (has_item_omission(report, &before.oz, "description")
                            || has_item_omission(report, &before.oz, "short text")),
                        "{path:?} -> {} {} item {index} lost short text without report",
                        version.number(),
                        phase.code()
                    );
                }
                if before.quantity.is_some() && before.quantity != after.quantity {
                    assert!(
                        has_item_omission(report, &before.oz, "quantity"),
                        "{path:?} -> {} {} item {index} lost quantity without report",
                        version.number(),
                        phase.code()
                    );
                }
                if before.unit.is_some() && before.unit != after.unit {
                    assert!(
                        has_item_omission(report, &before.oz, "unit"),
                        "{path:?} -> {} {} item {index} lost unit without report",
                        version.number(),
                        phase.code()
                    );
                }
            }
            (DocumentItem::Quantity(before), DocumentItem::Quantity(after)) => {
                assert_eq!(before.oz, after.oz, "{path:?} item {index}");
                if before.quantity.is_some() {
                    assert_eq!(before.quantity, after.quantity, "{path:?} item {index}");
                }
                assert_eq!(
                    before.takeoff_rows, after.takeoff_rows,
                    "{path:?} item {index}"
                );
            }
            (DocumentItem::Trade(before), DocumentItem::Trade(after)) => {
                assert_eq!(before.id, after.id, "{path:?} item {index}");
                if !before.short_text.is_empty() {
                    assert_eq!(before.short_text, after.short_text, "{path:?} item {index}");
                }
                if before.quantity.is_some() {
                    assert_eq!(before.quantity, after.quantity, "{path:?} item {index}");
                }
                if before.unit.is_some() {
                    assert_eq!(before.unit, after.unit, "{path:?} item {index}");
                }
            }
            (DocumentItem::Cost(before), DocumentItem::Cost(after)) => {
                assert_eq!(before.id, after.id, "{path:?} item {index}");
                if !before.short_text.is_empty() {
                    assert_eq!(before.short_text, after.short_text, "{path:?} item {index}");
                }
                if before.quantity.is_some() {
                    assert_eq!(before.quantity, after.quantity, "{path:?} item {index}");
                }
                if before.unit.is_some() {
                    assert_eq!(before.unit, after.unit, "{path:?} item {index}");
                }
            }
            _ => panic!(
                "{} -> {} {} changed item kind at {index}",
                path.display(),
                version.number(),
                phase.code()
            ),
        }
    }
}

fn has_item_omission(report: &gaeb_core::WriteReport, oz: &str, field: &str) -> bool {
    report
        .omitted_fields
        .iter()
        .any(|omission| omission.contains(&format!("Item {oz}: {field}")))
}
