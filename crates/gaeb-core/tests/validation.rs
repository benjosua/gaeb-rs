use gaeb_core::{
    ExchangePhase, ParseOptions, SourceVersion, ValidationOptions, parse_bytes, validate,
};

#[test]
fn strict_phase_profiles_reject_unpublished_legacy_phase() {
    let document = parse_bytes(
        br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA80/3.1"><GAEBInfo><Version>3.1</Version></GAEBInfo><Award><DP>80</DP><BoQ><BoQBody/></BoQ></Award></GAEB>"#,
        &ParseOptions::default(),
    )
    .unwrap();

    assert_eq!(document.source_version(), SourceVersion::DaXml31);
    assert_eq!(document.phase(), ExchangePhase::X80);
    assert!(validate(&document, &ValidationOptions::default()).is_empty());
    let issues = validate(
        &document,
        &ValidationOptions {
            strict_phase_profiles: true,
            ..ValidationOptions::default()
        },
    );
    assert!(issues.iter().any(|issue| {
        issue.code == "unsupported-phase-profile"
            && issue.message.contains("X80")
            && issue.message.contains("3.1")
    }));
}

#[cfg(feature = "reference-data")]
#[test]
fn strict_phase_profiles_accept_published_time_contract_profile() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/official/gaeb-32-direct-samples/Beispiel_01_83Z_3.2_2013-09-02.X83Z.xml"
    );
    let document = parse_bytes(&std::fs::read(path).unwrap(), &ParseOptions::default()).unwrap();

    let issues = validate(
        &document,
        &ValidationOptions {
            strict_phase_profiles: true,
            ..ValidationOptions::default()
        },
    );
    assert!(
        !issues
            .iter()
            .any(|issue| issue.code == "unsupported-phase-profile")
    );
}
