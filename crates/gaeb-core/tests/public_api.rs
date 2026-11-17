use gaeb_core::{DocumentKind, ParseOptions, SourceVersion, parse_bytes};

#[test]
fn parses_a_da_xml_procurement_document() {
    let document = parse_bytes(
        br#"<?xml version="1.0"?><GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Item RNoPart="001"><Qty>2</Qty><QU>m</QU><UP>4.50</UP><IT>9.00</IT></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#,
        &ParseOptions::default(),
    )
    .expect("document parses");

    assert_eq!(document.kind(), DocumentKind::Procurement);
    assert_eq!(document.source_version(), SourceVersion::DaXml33);
    assert_eq!(document.items().count(), 1);
}
