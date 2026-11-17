use crate::{ExchangePhase, GaebError, ParseOptions, SourceVersion, WriteOptions};

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ConversionReport {
    pub source_version: SourceVersion,
    pub target_version: SourceVersion,
    pub source_phase: ExchangePhase,
    pub target_phase: ExchangePhase,
    pub items_converted: usize,
    pub fields_dropped: Vec<String>,
    pub warnings: Vec<String>,
}

impl ConversionReport {
    pub fn is_upgrade(&self) -> bool {
        version_rank(self.target_version) > version_rank(self.source_version)
    }

    pub fn is_downgrade(&self) -> bool {
        version_rank(self.target_version) < version_rank(self.source_version)
    }

    pub fn is_same_family(&self) -> bool {
        let source = version_rank(self.source_version);
        let target = version_rank(self.target_version);
        (source <= 2 && target <= 2) || (source > 2 && target > 2)
    }

    pub fn has_data_loss(&self) -> bool {
        !self.fields_dropped.is_empty()
    }
}

pub fn convert_bytes(
    source: &[u8],
    parse_options: &ParseOptions,
    write_options: &WriteOptions,
) -> Result<(Vec<u8>, ConversionReport), GaebError> {
    let document = crate::parse_bytes(source, parse_options)?;
    let target_phase = write_options.phase;
    let items_converted = document.items().count();
    let source_version = document.source_version();
    let source_phase = document.phase();
    let mut output = Vec::new();
    let write_report = document.write(&mut output, write_options)?;
    let fields_dropped = write_report.omitted_fields.clone();
    let mut warnings = write_report.warnings;
    if !fields_dropped.is_empty() {
        warnings.push(format!(
            "{} field(s) omitted during conversion",
            fields_dropped.len()
        ));
    }
    Ok((
        output,
        ConversionReport {
            source_version,
            target_version: write_options.version,
            source_phase,
            target_phase,
            items_converted,
            fields_dropped,
            warnings,
        },
    ))
}

fn version_rank(version: SourceVersion) -> u8 {
    match version {
        SourceVersion::DaXml20 => 0,
        SourceVersion::DaXml21 => 1,
        SourceVersion::DaXml30 => 2,
        SourceVersion::DaXml31 => 3,
        SourceVersion::DaXml32 => 4,
        SourceVersion::DaXml33 => 5,
        SourceVersion::Gaeb90 => 0,
    }
}
