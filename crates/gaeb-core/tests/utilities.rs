use gaeb_core::{
    Boq, BoqCategory, Document, ExchangePhase, Item, ItemType, ParseOptions, ProcurementDocument,
    ProjectInfo, SourceVersion, WriteOptions,
    converter::convert_bytes,
    quality::quality_score,
    transition::tender_to_bid,
    units::{normalize_unit, normalize_units_in_document},
};

fn tender() -> Document {
    Document::Procurement(ProcurementDocument {
        meta: gaeb_core::DocumentMeta {
            version: SourceVersion::DaXml33,
            phase: ExchangePhase::X83,
            ..Default::default()
        },
        project: ProjectInfo::default(),
        boq: Boq {
            categories: vec![BoqCategory {
                items: vec![Item {
                    oz: "001".to_owned(),
                    short_text: "Concrete".to_owned(),
                    quantity: Some("2".parse().unwrap()),
                    unit: Some("m2".to_owned()),
                    item_type: ItemType::Normal,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        },
        invoice: None,
    })
}

#[test]
fn normalizes_units_and_scores_documents() {
    assert_eq!(normalize_unit(Some(" qm ")), Some("m²".to_owned()));
    let mut document = tender();
    assert_eq!(normalize_units_in_document(&mut document), 1);
    assert_eq!(document.grand_total(), 0.into());
    let score = quality_score(&document);
    assert_eq!(score.completeness, 100);
}

#[test]
fn tender_transition_clears_prices_and_changes_phase() {
    let mut source = tender();
    if let Document::Procurement(document) = &mut source {
        document.boq.categories[0].items[0].unit_price = Some("5".parse().unwrap());
        document.boq.categories[0].items[0].total_price = Some("10".parse().unwrap());
    }
    let bid = tender_to_bid(&source).unwrap();
    assert_eq!(bid.phase(), ExchangePhase::X84);
    assert!(bid.items().all(|item| match item {
        gaeb_core::DocumentItem::Procurement(item) => item.unit_price.is_none(),
        _ => false,
    }));
}

#[test]
fn conversion_returns_data_loss_report() {
    let source = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Item RNoPart="001"><Qty>2</Qty><QU>m</QU><Description><CompleteText><DetailTxt><Text><p><span>Concrete</span></p></Text></DetailTxt></CompleteText></Description></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#;
    let (_, report) = convert_bytes(
        source,
        &ParseOptions::default(),
        &WriteOptions {
            version: SourceVersion::DaXml33,
            phase: ExchangePhase::X83,
            indent: false,
        },
    )
    .unwrap();
    assert_eq!(report.items_converted, 1);
    assert!(!report.is_upgrade());
}
