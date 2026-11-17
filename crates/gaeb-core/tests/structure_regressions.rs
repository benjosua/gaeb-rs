#[cfg(feature = "reference-data")]
use std::{fs, path::Path};

#[cfg(feature = "reference-data")]
use gaeb_core::{BoqCategory, DocumentItem};
use gaeb_core::{Document, ExchangePhase, ParseOptions, SourceVersion, WriteOptions, parse_bytes};

#[cfg(feature = "reference-data")]
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/official");

#[cfg(feature = "reference-data")]
#[test]
fn official_corpus_exercises_supported_complex_structures() {
    let files = [
        "bvbs-33-ava-x81/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X81",
        "bvbs-33-ava-x84/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X84",
        "bvbs-33-ava-x86/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X86",
        "bvbs-33-construction-x83/Pruefdatei GAEB DA XML 3.3 - Bauausfuehrung - V 04 04 2024.x83",
        "gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_83Z_3.2_2014-03-24.X83Z",
        "gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_86ZE_3.2_2014-03-24.X86ZE",
    ];
    let mut items = Vec::new();
    let mut remarks = 0;
    let mut performance_descriptions = 0;
    for relative in files {
        let path = Path::new(ROOT).join(relative);
        assert!(
            path.exists(),
            "missing official fixture: {}",
            path.display()
        );
        let document = parse_bytes(&fs::read(&path).unwrap(), &ParseOptions::default()).unwrap();
        if let Document::Procurement(document) = document {
            for category in &document.boq.categories {
                let (category_remarks, category_performance_descriptions) =
                    annotation_counts(category);
                remarks += category_remarks;
                performance_descriptions += category_performance_descriptions;
            }
            items.extend(document.boq.items().cloned());
        }
    }
    assert!(
        items
            .iter()
            .any(|item| item.item_type == gaeb_core::ItemType::Markup)
    );
    assert!(items.iter().any(|item| !item.text_complements.is_empty()));
    assert!(items.iter().any(|item| !item.sub_descriptions.is_empty()));
    assert!(items.iter().any(|item| !item.qty_splits.is_empty()));
    assert!(items.iter().any(|item| !item.ctlg_assigns.is_empty()));
    assert!(items.iter().any(|item| item.standard_service.is_some()));
    assert!(items.iter().any(|item| item.predicted_quantity.is_some()));
    assert!(
        items
            .iter()
            .any(|item| item.reference_performance.is_some())
    );
    assert!(remarks > 0);
    assert!(performance_descriptions > 0);
}

#[cfg(feature = "reference-data")]
fn annotation_counts(category: &BoqCategory) -> (usize, usize) {
    let mut remarks = category.remarks.len();
    let mut performance_descriptions = category.performance_descriptions.len();
    for child in &category.categories {
        let (child_remarks, child_performance_descriptions) = annotation_counts(child);
        remarks += child_remarks;
        performance_descriptions += child_performance_descriptions;
    }
    (remarks, performance_descriptions)
}

#[test]
fn boq_level_annotations_survive_write_reparse() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Remark ID="R1"><Description><CompleteText><DetailTxt><Text><p><span>BoQ note</span></p></Text></DetailTxt></CompleteText></Description></Remark><PerfDescr ID="P1"><PerfNo>1</PerfNo><PerfLbl>Execution</PerfLbl><Description><CompleteText><DetailTxt><Text><p><span>BoQ execution</span></p></Text></DetailTxt></CompleteText></Description></PerfDescr><BoQCtgy RNoPart="01"><LblTx>Work</LblTx><BoQBody><Itemlist><Item ID="I1" RNoPart="001"><Qty>1</Qty><QU>m</QU></Item></Itemlist></BoQBody></BoQCtgy></BoQBody></BoQ></Award></GAEB>"#;
    let original = parse_bytes(input, &ParseOptions::default()).unwrap();
    let Document::Procurement(original_procurement) = &original else {
        panic!("procurement expected");
    };
    assert_eq!(original_procurement.boq.remarks, ["BoQ note"]);
    assert_eq!(
        original_procurement.boq.performance_descriptions,
        ["BoQ execution"]
    );

    let mut output = Vec::new();
    original
        .write(
            &mut output,
            &gaeb_core::WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X83,
                indent: false,
            },
        )
        .unwrap();
    let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap();
    let Document::Procurement(reparsed_procurement) = reparsed else {
        panic!("procurement expected");
    };
    assert_eq!(reparsed_procurement.boq.remarks, ["BoQ note"]);
    assert_eq!(
        reparsed_procurement.boq.performance_descriptions,
        ["BoQ execution"]
    );
}

#[test]
fn xml_entity_references_survive_parsing() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Item RNoPart="001"><Description><CompleteText><DetailTxt><Text>Steel &amp; concrete &lt;A&gt;</Text></DetailTxt></CompleteText></Description></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#;
    let document = parse_bytes(input, &ParseOptions::default()).unwrap();
    let Document::Procurement(document) = document else {
        panic!("procurement expected");
    };
    let item = document.boq.items().next().unwrap();
    assert_eq!(item.long_text, "Steel & concrete <A>");
}

#[test]
fn unsupported_item_element_fails_closed_instead_of_disappearing() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Surprise/></Itemlist></BoQBody></BoQ></Award></GAEB>"#;
    let error = parse_bytes(input, &ParseOptions::default()).unwrap_err();
    assert!(error.to_string().contains("unsupported Itemlist element"));
}

#[test]
fn unsupported_trade_item_element_fails_closed_instead_of_disappearing() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA96/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Order><DP>96</DP><OrderItem ID="I1"><Mystery>lost</Mystery></OrderItem></Order></GAEB>"#;
    let error = parse_bytes(input, &ParseOptions::default()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported OrderItem element: Mystery")
    );
}

#[test]
fn unsupported_cost_element_child_fails_closed_instead_of_disappearing() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA51/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><ElementalCosting><DP>51.2</DP><ECBody><CostElement ID="C1"><Descr>Labour</Descr><Unknown>lost</Unknown></CostElement></ECBody></ElementalCosting></GAEB>"#;
    let error = parse_bytes(input, &ParseOptions::default()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported CostElement element: Unknown")
    );
}

#[test]
fn supported_item_structures_survive_write_reparse() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA52/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>52</DP><BoQ><BoQBody><Itemlist><Item ID="I1" RNoPart="001"><Qty>2</Qty><PredQty>3</PredQty><BillQty>1</BillQty><NotAppl>Yes</NotAppl><HourIt>Yes</HourIt><MarkupIt>Yes</MarkupIt><UPBkdn>Yes</UPBkdn><RefDescr IDRef="D1"/><RefRNo IDRef="R1"/><RefPerfNo IDRef="P1"/><QU>h</QU><CtlgAssign><CtlgID>C</CtlgID><CtlgCode>01</CtlgCode><Quantity>2</Quantity></CtlgAssign><UP>1.25</UP><UPFrom>1</UPFrom><UPAvg>2</UPAvg><UPTo>3</UPTo><IT>2.50</IT><QtySplit><Label>half</Label><Qty>1</Qty><QU>h</QU></QtySplit><Description><CompleteText><DetailTxt><Text><p><span>Long text</span></p></Text><TextComplement Kind="Owner" MarkLbl="1"><ComplBody><span>extra</span></ComplBody></TextComplement></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>Short text</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description><CostApproach><CostType>Labour</CostType><Amount>4.50</Amount><Remark>remark</Remark></CostApproach><SubDescr><SubDNo>1</SubDNo><Description><CompleteText><DetailTxt><Text><p><span>sub long</span></p></Text></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>sub short</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description></SubDescr></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#;
    let original = parse_bytes(input, &ParseOptions::default()).unwrap();
    let mut output = Vec::new();
    let report = original
        .write(
            &mut output,
            &gaeb_core::WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X52,
                indent: false,
            },
        )
        .unwrap();
    assert!(
        report.omitted_fields.is_empty(),
        "{:?}",
        report.omitted_fields
    );
    let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap();
    let Document::Procurement(original) = original else {
        panic!("procurement expected")
    };
    let Document::Procurement(reparsed) = reparsed else {
        panic!("procurement expected")
    };
    let before = &original.boq.categories[0].items[0];
    let after = &reparsed.boq.categories[0].items[0];
    assert_eq!(after, before);
}

#[test]
fn x83_reports_price_fields_that_x83_cannot_encode() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Item RNoPart="001"><Qty>2</Qty><QU>m</QU><UP>1.25</UP><IT>2.50</IT><UPComp1>1</UPComp1></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#;
    let document = parse_bytes(input, &ParseOptions::default()).unwrap();
    let mut output = Vec::new();
    let report = document
        .write(
            &mut output,
            &gaeb_core::WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X83,
                indent: false,
            },
        )
        .unwrap();
    assert_eq!(
        report.omitted_fields,
        [
            "Item 001: unit price",
            "Item 001: total price",
            "Item 001: unit price components",
        ]
    );
    assert!(!String::from_utf8(output).unwrap().contains("<UP>"));
}

#[test]
fn decimal_finance_and_total_validation_are_exact() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQInfo><Totals><Total>2.53</Total></Totals></BoQInfo><BoQBody><Itemlist><Item RNoPart="001"><Qty>2.00</Qty><UP>1.265</UP><IT>2.53</IT></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#;
    let document = parse_bytes(input, &ParseOptions::default()).unwrap();
    let Document::Procurement(procurement) = &document else {
        panic!("procurement expected")
    };
    let item = &procurement.boq.categories[0].items[0];
    assert_eq!(item.computed_total().unwrap().to_string(), "2.53000");
    assert!(
        document
            .validation_issues()
            .iter()
            .all(|issue| issue.code != "total-mismatch")
    );

    let mismatch = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Item RNoPart="001"><Qty>2</Qty><UP>1.265</UP><IT>2.50</IT></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#;
    let mismatch = parse_bytes(mismatch, &ParseOptions::default()).unwrap();
    assert!(
        mismatch
            .validation_issues()
            .iter()
            .any(|issue| issue.code == "total-mismatch")
    );
}

#[test]
fn bidder_prices_are_reported_when_target_profile_has_no_bidder_price_element() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Item RNoPart="001"><BidderUP><BidderName>Bidder</BidderName><UP>1.25</UP></BidderUP></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#;
    let document = parse_bytes(input, &ParseOptions::default()).unwrap();
    let mut output = Vec::new();
    let report = document
        .write(
            &mut output,
            &gaeb_core::WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X83,
                indent: false,
            },
        )
        .unwrap();
    assert_eq!(report.omitted_fields, ["Item 001: bidder prices"]);
}

#[cfg(feature = "reference-data")]
#[test]
fn official_quantity_takeoff_rows_survive_write_reparse() {
    let path = Path::new(ROOT).join(
        "bvbs-33-quantity-x31/Pruefdatei GAEB DA XML 3.3 - Mengenermittlung - V 11 06 2021.x31",
    );
    assert!(
        path.exists(),
        "missing official fixture: {}",
        path.display()
    );
    let original = parse_bytes(&fs::read(&path).unwrap(), &ParseOptions::default()).unwrap();
    let rows_before: Vec<_> = original
        .items()
        .filter_map(|item| match item {
            DocumentItem::Quantity(item) => Some(item.takeoff_rows.clone()),
            _ => None,
        })
        .collect();
    assert!(rows_before.iter().any(|rows| !rows.is_empty()));
    let mut output = Vec::new();
    original
        .write(
            &mut output,
            &WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X31,
                indent: false,
            },
        )
        .unwrap();
    let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap();
    let rows_after: Vec<_> = reparsed
        .items()
        .filter_map(|item| match item {
            DocumentItem::Quantity(item) => Some(item.takeoff_rows.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(rows_after, rows_before);
}
