use gaeb_core::{
    Boq, BoqCategory, Document, DocumentMeta, ExchangePhase, Item, Lot, ProcurementDocument,
    ProjectInfo, SourceVersion, WriteOptions, compare_documents, diff_to_html_string, parse_bytes,
};
use rust_decimal::Decimal;

fn document(categories: Vec<BoqCategory>) -> Document {
    Document::Procurement(ProcurementDocument {
        meta: DocumentMeta {
            version: SourceVersion::DaXml33,
            phase: ExchangePhase::X83,
            ..DocumentMeta::default()
        },
        project: ProjectInfo {
            id: Some("P-1".to_owned()),
            name: Some("Project".to_owned()),
            currency: Some("EUR".to_owned()),
            ..ProjectInfo::default()
        },
        boq: Boq {
            categories,
            ..Boq::default()
        },
        invoice: None,
    })
}

fn category(rno: &str, label: &str, items: Vec<Item>) -> BoqCategory {
    BoqCategory {
        rno: rno.to_owned(),
        label: label.to_owned(),
        items,
        ..BoqCategory::default()
    }
}

fn item(oz: &str, text: &str, price: &str) -> Item {
    Item {
        oz: oz.to_owned(),
        short_text: text.to_owned(),
        quantity: Some(Decimal::TEN),
        unit_price: Some(price.parse().expect("decimal")),
        total_price: Some(Decimal::TEN * price.parse::<Decimal>().expect("decimal")),
        ..Item::default()
    }
}

#[test]
fn diff_reports_deltas_and_category_moves() {
    let before = document(vec![category(
        "01",
        "Old",
        vec![item("0010", "Old text", "10")],
    )]);
    let after = document(vec![category(
        "02",
        "New",
        vec![item("0010", "New text", "12")],
    )]);

    let result = compare_documents(&before, &after);
    assert_eq!(result.items.modified.len(), 1);
    assert_eq!(result.items.modified[0].oz, "0010");
    let price = result.items.modified[0]
        .changes
        .iter()
        .find(|change| change.field == "unit_price")
        .expect("price change");
    assert_eq!(price.absolute_delta, Some(Decimal::from(2)));
    assert_eq!(result.structure.items_moved.len(), 1);
    assert_eq!(result.structure.items_moved[0].old_category_rno, "01");
    assert_eq!(result.structure.items_moved[0].new_category_rno, "02");
}

#[test]
fn diff_preserves_lot_identity_for_repeated_ozs() {
    let before = document_with_lots(vec![
        Lot {
            rno: "1".to_owned(),
            label: "Lot one".to_owned(),
            categories: vec![category("01", "Work", vec![item("0010", "One", "1")])],
            ..Lot::default()
        },
        Lot {
            rno: "2".to_owned(),
            label: "Lot two".to_owned(),
            categories: vec![category("01", "Work", vec![item("0010", "Two", "1")])],
            ..Lot::default()
        },
    ]);
    let after = document_with_lots(vec![
        Lot {
            rno: "1".to_owned(),
            label: "Lot one".to_owned(),
            categories: vec![category("01", "Work", vec![item("0010", "One", "1")])],
            ..Lot::default()
        },
        Lot {
            rno: "2".to_owned(),
            label: "Lot two".to_owned(),
            categories: vec![category("01", "Work", vec![item("0010", "Changed", "1")])],
            ..Lot::default()
        },
    ]);

    let result = compare_documents(&before, &after);
    assert_eq!(result.items.modified.len(), 1);
    assert_eq!(result.items.modified[0].lot_rno, "2");
    assert_eq!(result.items.modified[0].oz, "0010");
}

#[test]
fn lot_structure_round_trips_and_reports_lot_on_moves() {
    let before = document_with_lots(vec![Lot {
        rno: "1".to_owned(),
        label: "Lot one".to_owned(),
        categories: vec![category("01", "Work", vec![item("0010", "Item", "1")])],
        ..Lot::default()
    }]);
    let after = document_with_lots(vec![Lot {
        rno: "2".to_owned(),
        label: "Lot two".to_owned(),
        categories: vec![category("01", "Work", vec![item("0010", "Item", "1")])],
        ..Lot::default()
    }]);

    let result = compare_documents(&before, &after);
    assert_eq!(result.structure.items_moved.len(), 1);
    assert_eq!(result.structure.items_moved[0].old_lot_rno, "1");
    assert_eq!(result.structure.items_moved[0].new_lot_rno, "2");

    let mut output = Vec::new();
    before
        .write(&mut output, &WriteOptions::default())
        .expect("write");
    let reparsed = parse_bytes(&output, &Default::default()).expect("reparse");
    let Document::Procurement(reparsed) = reparsed else {
        panic!("procurement")
    };
    assert_eq!(reparsed.boq.lots.len(), 1);
    assert_eq!(reparsed.boq.lots[0].rno, "1");
    assert_eq!(reparsed.boq.lots[0].label, "Lot one");
    assert_eq!(reparsed.boq.items().count(), 1);
}

fn document_with_lots(lots: Vec<Lot>) -> Document {
    let mut document = document(Vec::new());
    let Document::Procurement(document) = &mut document else {
        unreachable!()
    };
    document.boq.lots = lots;
    Document::Procurement(document.clone())
}

#[test]
fn diff_html_escapes_report_values() {
    let before = document(vec![category("01", "A", vec![])]);
    let after = document(vec![category(
        "01",
        "A",
        vec![item("0010", "<unsafe>", "1")],
    )]);
    let html = diff_to_html_string(&compare_documents(&before, &after));
    assert!(html.contains("&lt;unsafe&gt;"));
    assert!(!html.contains("<unsafe>"));
}

#[test]
fn rich_text_detail_excludes_outline_and_flags() {
    let xml = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Item RNoPart="0010"><Description><CompleteText><ComplTSA>No</ComplTSA><ComplTSB>No</ComplTSB><DetailTxt><Text><p><span>Detail text</span></p></Text></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>Short text</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#;
    let document = parse_bytes(xml, &Default::default()).expect("parse");
    let Document::Procurement(document) = document else {
        panic!("procurement")
    };
    let item = document.boq.items().next().expect("item");
    assert_eq!(item.short_text, "Short text");
    assert_eq!(item.long_text, "Detail text");
    assert_eq!(
        item.rich_text.as_ref().expect("rich text").paragraphs,
        vec!["Detail text".to_owned()]
    );
}

#[test]
fn x52_cost_approaches_and_types_survive_roundtrip() {
    let xml = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA52/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>52</DP><BoQ><BoQInfo><CostType><Name>Material</Name><Label>Mat.</Label></CostType></BoQInfo><BoQBody><BoQCtgy RNoPart="01"><Itemlist><Item RNoPart="0010"><Qty>2</Qty><QU>m</QU><UP>5</UP><IT>10</IT><UPComp1>4</UPComp1><DiscountPcnt>2.5</DiscountPcnt><CostApproach><CostType>Material</CostType><Amount>3</Amount><Remark>Mix</Remark></CostApproach></Item></Itemlist></BoQCtgy></BoQBody></BoQ></Award></GAEB>"#;
    let document = parse_bytes(xml, &Default::default()).expect("parse");
    let Document::Procurement(procurement) = &document else {
        panic!("procurement")
    };
    assert_eq!(procurement.boq.cost_types[0].name, "Material");
    let item = procurement.boq.items().next().expect("item");
    assert_eq!(item.cost_approaches[0].remark, "Mix");
    assert_eq!(item.up_components, vec![Decimal::from(4)]);
    let options = WriteOptions {
        phase: ExchangePhase::X52,
        ..WriteOptions::default()
    };
    let mut output = Vec::new();
    document.write(&mut output, &options).expect("write");
    let reparsed = parse_bytes(&output, &Default::default()).expect("reparse");
    let Document::Procurement(procurement) = reparsed else {
        panic!("procurement")
    };
    assert_eq!(procurement.boq.cost_types[0].label, "Mat.");
    assert_eq!(
        procurement
            .boq
            .items()
            .next()
            .expect("item")
            .cost_approaches
            .len(),
        1
    );
}

#[test]
fn cost_categories_and_nested_items_survive_roundtrip() {
    let xml = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA50/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><ElementalCosting><DP>50.1</DP><ECInfo><Name>Costs</Name></ECInfo><ECBody><ECCtgy><EleNo>300</EleNo><Descr>Building</Descr><ECBody><CostElement><EleNo>310</EleNo><Descr>Foundation</Descr><Qty>2</Qty><QU>m3</QU><UP>5</UP><IT>10</IT><CostElement><EleNo>310.1</EleNo><Descr>Concrete</Descr><UPFrom>4</UPFrom></CostElement></CostElement></ECBody></ECCtgy></ECBody></ElementalCosting></GAEB>"#;
    let document = parse_bytes(xml, &Default::default()).expect("parse");
    let Document::Cost(cost) = &document else {
        panic!("cost")
    };
    assert_eq!(cost.categories.len(), 1);
    assert_eq!(cost.categories[0].items[0].children[0].id, "310.1");
    assert_eq!(document.item_count(), 2);
    let mut output = Vec::new();
    let options = WriteOptions {
        phase: ExchangePhase::X50,
        ..WriteOptions::default()
    };
    document.write(&mut output, &options).expect("write");
    let reparsed = parse_bytes(&output, &Default::default()).expect("reparse");
    assert_eq!(reparsed.item_count(), 2);
}
