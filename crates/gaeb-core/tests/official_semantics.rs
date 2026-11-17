#![cfg(feature = "reference-data")]

use std::{fs, path::Path};

use gaeb_core::{
    Document, DocumentItem, ExchangePhase, ParseOptions, SourceVersion, WriteOptions, parse_bytes,
};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/official");

const FIXTURES: &[(&str, SourceVersion, ExchangePhase)] = &[
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
        "bvbs-33-quantity-x86/Pruefdatei GAEB DA XML 3.3 - Mengenermittlung - V 11 06 2021.x86",
        SourceVersion::DaXml33,
        ExchangePhase::X86,
    ),
    (
        "bvbs-33-text-x81/GAEB Pruefdatei GAEB DA XML 3.3 - Texterstellung V 11 06 2021.x81",
        SourceVersion::DaXml33,
        ExchangePhase::X81,
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
        "gaeb-32-direct-samples/Beispiel_01_86ZE_3.2_2013-09-02.X86ZE.xml",
        SourceVersion::DaXml32,
        ExchangePhase::X86ZE,
    ),
    (
        "gaeb-32-direct-samples/Beispiel_01_86ZR_3.2_2013-09-02.X86ZR.xml",
        SourceVersion::DaXml32,
        ExchangePhase::X86ZR,
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
];

#[test]
fn official_fixtures_preserve_supported_semantics_through_write_reparse() {
    for &(relative, version, phase) in FIXTURES {
        let path = Path::new(ROOT).join(relative);
        assert!(
            path.exists(),
            "missing official fixture: {}",
            path.display()
        );
        let source = fs::read(&path).unwrap();
        let original = parse_bytes(&source, &ParseOptions::default())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(original.source_version(), version, "{}", path.display());
        assert_eq!(original.phase(), phase, "{}", path.display());

        let mut output = Vec::new();
        let report = original
            .write(
                &mut output,
                &WriteOptions {
                    version: SourceVersion::DaXml33,
                    phase,
                    indent: false,
                },
            )
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            report
                .omitted_fields
                .iter()
                .all(|field| expected_same_phase_omission(phase, field)),
            "{} unexplained omissions: {:?}",
            path.display(),
            report.omitted_fields
        );
        let reparsed = parse_bytes(&output, &ParseOptions::default())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(reparsed.phase(), phase, "{}", path.display());
        assert_eq!(reparsed.kind(), original.kind(), "{}", path.display());
        assert_eq!(
            reparsed.items().count(),
            original.items().count(),
            "{}",
            path.display()
        );
        assert_eq!(
            category_annotations(&original),
            category_annotations(&reparsed),
            "{}",
            path.display()
        );
        assert_item_semantics(&original, &reparsed, phase, path.to_string_lossy().as_ref());
    }
}

type CategoryAnnotations = (Vec<String>, Vec<String>, Option<String>, Option<String>);

fn category_annotations(document: &Document) -> Vec<CategoryAnnotations> {
    fn visit(category: &gaeb_core::BoqCategory, result: &mut Vec<CategoryAnnotations>) {
        result.push((
            category.remarks.clone(),
            category.performance_descriptions.clone(),
            category.declared_total.map(|total| total.to_string()),
            category.bid_up_down_request.clone(),
        ));
        for child in &category.categories {
            visit(child, result);
        }
    }
    match document {
        Document::Procurement(document) => {
            let mut result = Vec::new();
            for category in &document.boq.categories {
                visit(category, &mut result);
            }
            result
        }
        _ => Vec::new(),
    }
}

fn expected_same_phase_omission(phase: ExchangePhase, field: &str) -> bool {
    (phase == ExchangePhase::X83 && field.ends_with(": not applicable"))
        || (matches!(phase, ExchangePhase::X83Z | ExchangePhase::X86ZR)
            && field.ends_with(": quantity"))
        || (phase != ExchangePhase::X80
            && phase != ExchangePhase::X81
            && phase != ExchangePhase::X82
            && field.ends_with(": unit price range"))
}

fn assert_item_semantics(
    original: &Document,
    reparsed: &Document,
    phase: ExchangePhase,
    context: &str,
) {
    let original: Vec<_> = original.items().collect();
    let reparsed: Vec<_> = reparsed.items().collect();
    for (before, after) in original.into_iter().zip(reparsed) {
        match (before, after) {
            (DocumentItem::Procurement(before), DocumentItem::Procurement(after)) => {
                assert_eq!(after.id, before.id, "{context}");
                assert_eq!(after.oz, before.oz, "{context}");
                assert_eq!(
                    after.short_text, before.short_text,
                    "{context} item {} {:?}",
                    before.oz, before.source_item_kind
                );
                assert_eq!(
                    after.long_text, before.long_text,
                    "{context} item {} {:?}",
                    before.oz, before.source_item_kind
                );
                if !matches!(phase, ExchangePhase::X83Z | ExchangePhase::X86ZR) {
                    assert_eq!(after.quantity, before.quantity, "{context}");
                }
                assert_eq!(
                    after.predicted_quantity, before.predicted_quantity,
                    "{context}"
                );
                assert_eq!(after.bill_quantity, before.bill_quantity, "{context}");
                assert_eq!(after.quantity_tbd, before.quantity_tbd, "{context}");
                assert_eq!(after.unit, before.unit, "{context}");
                assert_eq!(after.unit_price, before.unit_price, "{context}");
                assert_eq!(after.total_price, before.total_price, "{context}");
                assert_eq!(after.item_type, before.item_type, "{context}");
                assert_eq!(after.source_item_kind, before.source_item_kind, "{context}");
                if before.not_applicable != after.not_applicable {
                    assert!(phase == ExchangePhase::X83, "{context}");
                }
                assert_eq!(after.hour_item, before.hour_item, "{context}");
                assert_eq!(after.markup_item, before.markup_item, "{context}");
                assert_eq!(after.up_breakdown, before.up_breakdown, "{context}");
                assert_eq!(after.sum_description, before.sum_description, "{context}");
                assert_eq!(
                    after.bid_up_down_request, before.bid_up_down_request,
                    "{context}"
                );
                assert_eq!(after.bid_up_down_pct, before.bid_up_down_pct, "{context}");
                assert_eq!(
                    after.reference_description, before.reference_description,
                    "{context}"
                );
                assert_eq!(after.reference_rno, before.reference_rno, "{context}");
                assert_eq!(
                    after.reference_performance, before.reference_performance,
                    "{context}"
                );
                if phase == ExchangePhase::X80
                    || phase == ExchangePhase::X81
                    || phase == ExchangePhase::X82
                {
                    assert_eq!(after.up_from, before.up_from, "{context}");
                    assert_eq!(after.up_avg, before.up_avg, "{context}");
                    assert_eq!(after.up_to, before.up_to, "{context}");
                }
                assert_eq!(after.catalog_number, before.catalog_number, "{context}");
                assert_eq!(after.markup_sub_qtys, before.markup_sub_qtys, "{context}");
                assert_eq!(after.ctlg_assigns, before.ctlg_assigns, "{context}");
                assert_eq!(after.text_complements, before.text_complements, "{context}");
                assert_eq!(after.sub_descriptions, before.sub_descriptions, "{context}");
                assert_eq!(after.standard_service, before.standard_service, "{context}");
                assert_eq!(after.cost_approaches, before.cost_approaches, "{context}");
                assert_eq!(after.bidder_prices, before.bidder_prices, "{context}");
            }
            (DocumentItem::Quantity(before), DocumentItem::Quantity(after)) => {
                assert_eq!(after.oz, before.oz, "{context}");
                assert_eq!(after.quantity, before.quantity, "{context}");
                assert_eq!(after.takeoff_rows, before.takeoff_rows, "{context}");
            }
            (DocumentItem::Trade(before), DocumentItem::Trade(after)) => {
                assert_eq!(after.id, before.id, "{context}");
                assert_eq!(after.article_number, before.article_number, "{context}");
                assert_eq!(after.short_text, before.short_text, "{context}");
                assert_eq!(after.quantity, before.quantity, "{context}");
                assert_eq!(after.unit, before.unit, "{context}");
                assert_eq!(after.net_price, before.net_price, "{context}");
            }
            (DocumentItem::Cost(before), DocumentItem::Cost(after)) => {
                assert_eq!(after.id, before.id, "{context}");
                assert_eq!(after.short_text, before.short_text, "{context}");
                assert_eq!(after.quantity, before.quantity, "{context}");
                assert_eq!(after.unit, before.unit, "{context}");
                assert_eq!(after.unit_price, before.unit_price, "{context}");
                assert_eq!(after.total_price, before.total_price, "{context}");
                assert_eq!(after.children.len(), before.children.len(), "{context}");
            }
            _ => panic!("document item kind changed: {context}"),
        }
    }
}
