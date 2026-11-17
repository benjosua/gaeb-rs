use gaeb_core::{
    ExchangePhase, ParseOptions, SourceVersion, ValidationOptions, WriteOptions, parse_bytes,
    validate,
};

#[test]
fn rejects_external_entity_declarations_before_parsing() {
    let error = parse_bytes(
        b"<!DOCTYPE GAEB [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><GAEB>&x;</GAEB>",
        &ParseOptions::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("forbidden external-resource"));
}

#[test]
fn enforces_input_text_and_nesting_limits() {
    let error = parse_bytes(
        b"<GAEB><x>123456</x></GAEB>",
        &ParseOptions {
            max_text_bytes: 5,
            ..ParseOptions::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("text node"));
    let error = parse_bytes(
        b"<GAEB><a><b><c/></b></a></GAEB>",
        &ParseOptions {
            max_nesting: 2,
            ..ParseOptions::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("nesting"));
}

#[test]
fn rejects_unreleased_x95_and_current_x88_but_keeps_historical_x88() {
    let historical = parse_bytes(b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA88/3.1\"><GAEBInfo><Version>3.1</Version></GAEBInfo><Award><DP>88</DP><BoQ><BoQBody/></BoQ></Award></GAEB>", &ParseOptions::default()).unwrap();
    assert_eq!(historical.phase(), ExchangePhase::X88);
    assert!(validate(&historical, &ValidationOptions::default()).is_empty());

    let current = parse_bytes(b"<GAEB><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>88</DP><BoQ><BoQBody/></BoQ></Award></GAEB>", &ParseOptions::default()).unwrap();
    let error = current
        .write(
            Vec::new(),
            &WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X88,
                indent: false,
            },
        )
        .unwrap_err();
    assert!(error.to_string().contains("not defined"));

    let x95 = parse_bytes(b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA95/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><Order><DP>95</DP></Order></GAEB>", &ParseOptions::default()).unwrap();
    assert_eq!(x95.phase(), ExchangePhase::X95);
    assert!(
        validate(&x95, &ValidationOptions::default())
            .iter()
            .any(|issue| { issue.code == "unsupported-phase" })
    );
    let error = x95
        .write(
            Vec::new(),
            &WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X95,
                indent: false,
            },
        )
        .unwrap_err();
    assert!(error.to_string().contains("not a released"));
}
