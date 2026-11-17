use gaeb_core::{
    BoqBuilder, Document, ExchangePhase, ItemType, ParseOptions, SourceVersion, WriteOptions,
    parse_bytes,
};
use rust_decimal::Decimal;

#[test]
fn builds_nested_procurement_document_and_round_trips() {
    let mut builder = BoqBuilder::new();
    builder
        .project_id("PRJ-001")
        .project_name("School Renovation")
        .currency("EUR");
    let lot = builder.add_lot("1", "Structural").expect("lot");
    let category = lot.add_category("01", "Concrete");
    let subcategory = category.add_subcategory("01.01", "Foundations");
    subcategory
        .add_item("Foundation")
        .quantity(Decimal::from(120))
        .unit("m3")
        .unit_price(Decimal::from(85));

    let built = builder.build().expect("build");
    let Document::Procurement(document) = &built else {
        panic!("procurement document expected")
    };
    let item = document.boq.items().next().expect("item");
    assert_eq!(item.oz, "01.01.0010");
    assert_eq!(item.total_price, Some(Decimal::from(10_200)));
    assert_eq!(document.boq.totals, Some(Decimal::from(10_200)));
    assert_eq!(document.project.id.as_deref(), Some("PRJ-001"));

    let mut output = Vec::new();
    built
        .write(&mut output, &WriteOptions::default())
        .expect("write");
    let reparsed = parse_bytes(&output, &ParseOptions::default()).expect("parse");
    let Document::Procurement(reparsed) = reparsed else {
        panic!("procurement document expected")
    };
    assert_eq!(reparsed.boq.lots.len(), 1);
    assert_eq!(reparsed.boq.lots[0].label, "Structural");
    assert_eq!(reparsed.boq.items().count(), 1);
}

#[test]
fn implicit_lot_generates_ozs_and_computed_totals() {
    let mut builder = BoqBuilder::new();
    let category = builder.add_category("01", "Section").expect("category");
    category.add_item("First");
    category
        .add_item("Second")
        .quantity(Decimal::new(10, 1))
        .unit("m2")
        .unit_price(Decimal::new(4599, 2));

    let document = builder.build().expect("build");
    let Document::Procurement(document) = document else {
        panic!("procurement document expected")
    };
    let items: Vec<_> = document.boq.items().collect();
    assert_eq!(items[0].oz, "01.0010");
    assert_eq!(items[1].oz, "01.0020");
    assert_eq!(items[1].total_price, Some(Decimal::new(4599, 2)));
    assert_eq!(document.boq.lots[0].rno, "1");
}

#[test]
fn strict_build_rejects_duplicate_oz_but_allows_same_oz_in_different_lots() {
    let mut duplicate = BoqBuilder::new();
    let category = duplicate.add_category("01", "Section").expect("category");
    category.add_item_with_oz("01.0010", "First");
    category.add_item_with_oz("01.0010", "Duplicate");
    let error = duplicate.build_strict().expect_err("duplicate should fail");
    assert!(error.to_string().contains("duplicate OZ"));

    let mut separate_lots = BoqBuilder::new();
    separate_lots
        .add_lot("1", "One")
        .expect("lot")
        .add_category("01", "Section")
        .add_item_with_oz("01.0010", "First")
        .unit_price(Decimal::ONE);
    separate_lots
        .add_lot("2", "Two")
        .expect("lot")
        .add_category("01", "Section")
        .add_item_with_oz("01.0010", "Second")
        .unit_price(Decimal::ONE);
    separate_lots
        .build_strict()
        .expect("different lots are valid");
}

#[test]
fn lenient_build_reports_phase_and_version_compatibility() {
    let mut builder = BoqBuilder::new();
    builder
        .phase(ExchangePhase::X80)
        .version(SourceVersion::DaXml32);
    let item = builder
        .add_category("01", "Section")
        .expect("category")
        .add_item("Item")
        .quantity(Decimal::ONE)
        .unit_price(Decimal::ONE)
        .bim_guid("guid");
    item.item_type(ItemType::Normal);

    let document = builder.build().expect("build");
    let warnings = &document.meta().warnings;
    assert!(warnings.iter().any(|warning| warning.contains("X80")));
    assert!(warnings.iter().any(|warning| warning.contains("bim_guid")));
}

#[test]
fn fluent_item_handle_sets_text_and_attachment() {
    let mut builder = BoqBuilder::new();
    builder
        .add_category("01", "Section")
        .expect("category")
        .add_item("Item")
        .set_long_text("Details")
        .add_attachment("plan.pdf", b"PDF".to_vec(), "application/pdf");

    let document = builder.build().expect("build");
    let Document::Procurement(document) = document else {
        panic!("procurement document expected")
    };
    let item = document.boq.items().next().expect("item");
    assert_eq!(item.long_text, "Details");
    assert_eq!(
        item.rich_text.as_ref().expect("rich text").plain_text,
        "Details"
    );
    assert_eq!(item.attachments[0].size_bytes, 3);
}
