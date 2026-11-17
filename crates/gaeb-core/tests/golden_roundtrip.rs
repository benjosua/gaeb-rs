use gaeb_core::{
    Document, DocumentKind, ExchangePhase, ParseOptions, SourceVersion, WriteOptions, parse_bytes,
};

fn procurement_xml(phase: ExchangePhase) -> String {
    format!(
        "<?xml version=\"1.0\"?><GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA{}/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>{}</DP><BoQ><BoQBody><BoQCtgy RNoPart=\"01\"><LblTx>Work</LblTx><BoQBody><Itemlist><Item RNoPart=\"001\"><Qty>2</Qty><QU>m</QU><UP>4.50</UP><IT>9.00</IT><Description><ShortText>Concrete</ShortText></Description></Item></Itemlist></BoQBody></BoQCtgy></BoQBody></BoQ></Award></GAEB>",
        phase.code().trim_start_matches('X'),
        phase.code()
    )
}

fn xml_for(phase: ExchangePhase) -> String {
    match phase.kind() {
        DocumentKind::Procurement => procurement_xml(phase),
        DocumentKind::Trade => format!("<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA{}/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><Order><DP>{}</DP><OrderItem ID=\"A1\"><ArtNo>ART-1</ArtNo><ShortText>Brick</ShortText><Qty>2</Qty><QU>pc</QU><NetPrice>3.50</NetPrice></OrderItem></Order></GAEB>", phase.code().trim_start_matches('X'), phase.code()),
        DocumentKind::Cost => format!("<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA{}/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><ElementalCosting><DP>{}</DP><CostElement ID=\"C1\"><ShortText>Labour</ShortText><Qty>2</Qty><QU>h</QU><UP>10</UP><IT>20</IT></CostElement></ElementalCosting></GAEB>", phase.code().trim_start_matches('X'), phase.code()),
        DocumentKind::Quantity => "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA31/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><QtyDeterm><DP>31</DP><QtyItem RNoPart=\"001\"><Qty>2</Qty><QTakeoffRow>row</QTakeoffRow></QtyItem></QtyDeterm></GAEB>".to_owned(),
        DocumentKind::Structured => format!("<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA{}/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><PhaseData><DP>{}</DP><Value>preserved</Value></PhaseData></GAEB>", phase.code().trim_start_matches('X'), phase.code()),
    }
}

#[test]
fn phases_without_official_inputs_round_trip_through_their_domain_kind() {
    let phases = [
        ExchangePhase::X50,
        ExchangePhase::X51,
        ExchangePhase::X52,
        ExchangePhase::X80,
        ExchangePhase::X85,
        ExchangePhase::X87,
        ExchangePhase::X89,
        ExchangePhase::X89B,
        ExchangePhase::X93,
        ExchangePhase::X94,
        ExchangePhase::X96,
        ExchangePhase::X97,
    ];

    for phase in phases {
        let source = xml_for(phase);
        let document = parse_bytes(source.as_bytes(), &ParseOptions::default())
            .unwrap_or_else(|error| panic!("{}: {error}", phase.code()));
        assert_eq!(document.kind(), phase.kind(), "{}", phase.code());
        assert_eq!(document.phase(), phase, "{}", phase.code());

        let mut output = Vec::new();
        document
            .write(
                &mut output,
                &WriteOptions {
                    version: SourceVersion::DaXml33,
                    phase,
                    indent: false,
                },
            )
            .unwrap_or_else(|error| panic!("{}: {error}", phase.code()));
        let reparsed = parse_bytes(&output, &ParseOptions::default())
            .unwrap_or_else(|error| panic!("{}: {error}", phase.code()));
        assert_eq!(reparsed.kind(), document.kind(), "{}", phase.code());
        assert_eq!(
            reparsed.items().count(),
            document.items().count(),
            "{}",
            phase.code()
        );
    }
}

#[test]
fn cost_ecbody_hierarchy_parses_each_element_once() {
    let document = parse_bytes(
        br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA51/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><ElementalCosting><DP>51.2</DP><ECInfo><Name>Costing</Name></ECInfo><ECBody><ECCtgy><ECBody><CostElement ID="parent"><EleNo>parent</EleNo><Descr>Parent</Descr><QU>m</QU><CostElement ID="child"><EleNo>child</EleNo><Descr>Child</Descr><QU>m</QU></CostElement></CostElement></ECBody></ECCtgy></ECBody></ElementalCosting></GAEB>"#,
        &ParseOptions::default(),
    )
    .unwrap();
    let Document::Cost(cost) = document else {
        panic!("cost document expected");
    };
    assert_eq!(cost.items.len(), 1);
    assert_eq!(cost.items[0].children.len(), 1);
    assert_eq!(cost.items[0].id, "parent");
    assert_eq!(cost.items[0].children[0].id, "child");
}

#[test]
fn cost_element_fields_survive_write_reparse() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA51/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><ElementalCosting><DP>51.2</DP><ECBody><CostElement ID="C1"><EleNo>C1</EleNo><Descr>Labour</Descr><CatID>CAT</CatID><CtlgAssign><CtlgID>CTLG</CtlgID><CtlgCode>001</CtlgCode></CtlgAssign><Remark>Cost note</Remark><BillElement>Yes</BillElement><Qty>2</Qty><QU>h</QU><UP>10</UP><Markup>1.5</Markup><IT>20</IT><UPFrom>9</UPFrom><UPAvg>10</UPAvg><UPTo>11</UPTo></CostElement></ECBody></ElementalCosting></GAEB>"#;
    let original = parse_bytes(input, &ParseOptions::default()).unwrap();
    let mut output = Vec::new();
    let report = original
        .write(
            &mut output,
            &WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X51,
                indent: false,
            },
        )
        .unwrap();
    assert!(report.omitted_fields.is_empty());
    let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap();
    let Document::Cost(before) = original else {
        panic!("cost document expected");
    };
    let Document::Cost(after) = reparsed else {
        panic!("cost document expected");
    };
    assert_eq!(after.items, before.items);
}

#[test]
fn trade_fields_survive_write_reparse() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA96/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><PrjInfo><NamePrj>Project</NamePrj><Cur>CHF</Cur></PrjInfo><Order><DP>96</DP><OrderInfo><OrderNo>O-1</OrderNo><Cur>CHF</Cur><OrderDate>2026-01-15</OrderDate></OrderInfo><OrderItem ID="I1"><ItemChara>normal</ItemChara><ItemType>lump</ItemType><Service>Yes</Service><EAN>4001234567890</EAN><ArtNoID>ILN</ArtNoID><ArtNo>A-1</ArtNo><SupplierArtNoID>SID</SupplierArtNoID><SupplierArtNo>S-1</SupplierArtNo><CustomerArtNo>C-1</CustomerArtNo><CatalogArtNo>CAT-1</CatalogArtNo><CatalogNo>CAT</CatalogNo><Qty>2</Qty><QU>pc</QU><Description><CompleteText><OutlineText><OutlTxt><TextOutlTxt>Widget</TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description><OfferPrice>4.50</OfferPrice><NetPrice>4.00</NetPrice><PriceBasis>1</PriceBasis><AQU>box</AQU><ModeOfShipment>truck</ModeOfShipment><DeliveryDate>2026-02-01</DeliveryDate><DeliveryTime>12:00</DeliveryTime><DeliveryWeek>05</DeliveryWeek></OrderItem></Order></GAEB>"#;
    let original = parse_bytes(input, &ParseOptions::default()).unwrap();
    let mut output = Vec::new();
    original
        .write(
            &mut output,
            &WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X96,
                indent: false,
            },
        )
        .unwrap();
    let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap();
    let Document::Trade(before) = original else {
        panic!("trade document expected");
    };
    let Document::Trade(after) = reparsed else {
        panic!("trade document expected");
    };
    assert_eq!(after.project, before.project);
    assert_eq!(after.order_number, before.order_number);
    assert_eq!(after.currency, before.currency);
    assert_eq!(after.order_date, before.order_date);
    assert_eq!(after.items, before.items);
}

#[test]
fn cost_totals_use_decimal_rounding_invariants() {
    let document = parse_bytes(
        br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA51/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><ElementalCosting><DP>51.2</DP><ECBody><CostElement ID="C1"><EleNo>C1</EleNo><Descr>Labour</Descr><Qty>2</Qty><QU>h</QU><UP>1.265</UP><IT>2.53</IT></CostElement><CostElement ID="C2"><EleNo>C2</EleNo><Descr>Wrong</Descr><Qty>2</Qty><QU>h</QU><UP>1.265</UP><IT>2.50</IT></CostElement></ECBody></ElementalCosting></GAEB>"#,
        &ParseOptions::default(),
    )
    .unwrap();
    assert_eq!(
        document
            .validation_issues()
            .iter()
            .filter(|issue| issue.code == "cost-total-mismatch")
            .count(),
        1
    );
}

#[test]
fn legacy_xml_uses_mapping_table_not_tag_rewriting() {
    let document = parse_bytes(b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/200407\"><GAEBInfo><Version>2.0</Version></GAEBInfo><Vergabe><Datum>83</Datum><Leistungsverzeichnis><LVBereich><LVGruppe OZ=\"01\"><Bezeichnung>Work</Bezeichnung><LVBereich><Positionsliste><Position OZ=\"001\"><Menge>2</Menge><Mengeneinheit>m</Mengeneinheit><Einheitspreis>4.5</Einheitspreis><Gesamtbetrag>9</Gesamtbetrag><Kurztext>Concrete</Kurztext></Position></Positionsliste></LVBereich></LVGruppe></LVBereich></Leistungsverzeichnis></Vergabe></GAEB>", &ParseOptions::default()).unwrap();
    assert_eq!(document.source_version(), SourceVersion::DaXml20);
    assert_eq!(document.items().count(), 1);
    let Document::Procurement(document) = document else {
        panic!("procurement document expected");
    };
    assert_eq!(document.boq.categories[0].items[0].short_text, "Concrete");
}

#[test]
fn gaeb_90_parses_real_d83_d86_record_shapes() {
    let input = b"T0                                                                        000001\r\n70                      Created with Dangl GAEB Tool                      000002\r\nT9                                                                        000003\r\n00        86L                                                 1122PPP0090 000004\r\n01                                        07.01.19                     X  000005\r\n02Neubau Sportheim                                                        000006\r\n03Georg Dangl                                                             000007\r\n04                                                                        000008\r\n06                                                                    000009\r\n08EUR   Euro                                                              000010\r\n11                                                                           000011\r\n12Hauptgebaeude                                                          000012\r\n210101001  NNN         00000001000Psch                                    000013\r\n230101001   0000080000  000000080000                                      000014\r\n25Baustelleneinrichtung                                                   000015\r\n26   Einrichtung der Baustelle inkl. Kran                                 000016\r\n31                                                                        000017\r\n32                                                                        000018\r\n33                                                                        000019\r\n99                                                                        000020\r\n";
    let document = parse_bytes(input, &ParseOptions::default()).unwrap();
    assert_eq!(document.source_version(), SourceVersion::Gaeb90);
    assert_eq!(document.phase(), ExchangePhase::X86);
    let Document::Procurement(document) = document else {
        panic!("procurement document expected");
    };
    assert_eq!(document.project.name.as_deref(), Some("Neubau Sportheim"));
    assert_eq!(document.project.currency.as_deref(), Some("EUR"));
    assert_eq!(document.project.currency_label.as_deref(), Some("Euro"));
    assert_eq!(document.meta.date.as_deref(), Some("07.01.19"));
    assert_eq!(
        document.meta.program.as_deref(),
        Some("Created with Dangl GAEB Tool")
    );
    assert_eq!(document.boq.categories.len(), 1);
    assert_eq!(document.boq.categories[0].label, "Hauptgebaeude");
    assert_eq!(document.boq.items().count(), 1);
    let item = &document.boq.categories[0].items[0];
    assert_eq!(item.oz, "0101001");
    assert_eq!(item.quantity, Some(rust_decimal::Decimal::new(1, 0)));
    assert_eq!(item.unit.as_deref(), Some("Psch"));
    assert_eq!(item.unit_price, Some(rust_decimal::Decimal::new(800, 0)));
    assert_eq!(item.total_price, Some(rust_decimal::Decimal::new(800, 0)));
    assert_eq!(item.short_text, "Baustelleneinrichtung");
    assert_eq!(item.long_text, "Einrichtung der Baustelle inkl. Kran");
    assert!(
        document
            .meta
            .warnings
            .iter()
            .any(|warning| warning.contains("04"))
    );
}

#[test]
fn gaeb_90_writer_emits_fixed_width_extended_records() {
    let input = b"00        86L                                                 1122PPP0090 000001\r\n01                                        07.01.19                     X  000002\r\n02Neubau Sportheim                                                        000003\r\n08EUR   Euro                                                              000004\r\n11                                                                           000005\r\n12Hauptgebaeude                                                          000006\r\n21 1       NNN         00000001000m                                       000007\r\n23 1          0000000100  000000000100                                      000008\r\n25Rohr                                                                     000009\r\n26   Mit Anschluss                                                        000010\r\n99                                                                        000011\r\n";
    let document = parse_bytes(input, &ParseOptions::default()).unwrap();
    let mut output = Vec::new();
    document
        .write(
            &mut output,
            &WriteOptions {
                version: SourceVersion::Gaeb90,
                phase: ExchangePhase::X86,
                indent: false,
            },
        )
        .unwrap();
    assert!(
        output
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .all(|line| line
                .strip_suffix(b"\r")
                .is_some_and(|line| line.len() == 80))
    );
    assert!(output.windows(2).any(|record| record == b"21"));
    assert!(output.windows(2).any(|record| record == b"23"));
    let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap();
    assert_eq!(reparsed.items().count(), 1);
    let Document::Procurement(reparsed) = reparsed else {
        panic!("procurement document expected");
    };
    assert_eq!(
        reparsed.boq.items().next().unwrap().long_text,
        "Mit Anschluss"
    );
}

#[test]
fn gaeb_90_uses_cp850_for_german_text() {
    let input = b"00        83L                                                 1122PPP0090 000001\r\n11                                                                           000002\r\n12Baustelle                                                              000003\r\n21 1       NNN         00000001000m                                       000004\r\n25D\x84mmung                                                                  000005\r\n99                                                                        000006\r\n";
    let document = parse_bytes(input, &ParseOptions::default()).unwrap();
    let Document::Procurement(document) = document else {
        panic!("procurement document expected");
    };
    assert_eq!(document.boq.items().next().unwrap().short_text, "Dämmung");
    let document = Document::Procurement(document);
    let mut output = Vec::new();
    document
        .write(
            &mut output,
            &WriteOptions {
                version: SourceVersion::Gaeb90,
                phase: ExchangePhase::X83,
                indent: false,
            },
        )
        .unwrap();
    assert!(output.contains(&0x84));
    let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap();
    assert_eq!(reparsed.items().next().unwrap().identifier(), "1");
}

#[test]
fn gaeb_90_round_trips_basic_records() {
    let input = b"00PRJ-1               Road work\r\n01A       Earthworks\r\n02001           \r\n03Excavation\r\n0512            m3\r\n0610.50\r\n07126.00\r\n99\r\n";
    let document = parse_bytes(input, &ParseOptions::default()).unwrap();
    let mut output = Vec::new();
    document
        .write(
            &mut output,
            &WriteOptions {
                version: SourceVersion::Gaeb90,
                phase: ExchangePhase::X83,
                indent: false,
            },
        )
        .unwrap();
    let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap();
    assert_eq!(reparsed.source_version(), SourceVersion::Gaeb90);
    assert_eq!(reparsed.items().count(), 1);
}

#[cfg(feature = "reference-data")]
#[test]
fn reference_fixture_has_stable_core_semantics() {
    let input = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/reference/gaebxml.x83"
    ))
    .unwrap();
    let document = parse_bytes(&input, &ParseOptions::default()).unwrap();
    assert_eq!(document.phase(), ExchangePhase::X83);
    assert_eq!(document.source_version(), SourceVersion::DaXml32);
    assert_eq!(document.items().count(), 6);
}
