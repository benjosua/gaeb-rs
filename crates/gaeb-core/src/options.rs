use crate::{ExchangePhase, SourceVersion};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseOptions {
    pub max_input_bytes: usize,
    pub max_nesting: usize,
    pub max_text_bytes: usize,
    pub max_attachment_bytes: usize,
    pub max_items: usize,
    pub strict: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            max_input_bytes: 32 * 1024 * 1024,
            max_nesting: 128,
            max_text_bytes: 1024 * 1024,
            max_attachment_bytes: 10 * 1024 * 1024,
            max_items: 1_000_000,
            strict: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteOptions {
    pub version: SourceVersion,
    pub phase: ExchangePhase,
    pub indent: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            version: SourceVersion::DaXml33,
            phase: ExchangePhase::X83,
            indent: true,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WriteReport {
    pub warnings: Vec<String>,
    pub omitted_fields: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ValidationOptions {
    pub check_totals: bool,
    pub strict_phase_profiles: bool,
    pub suppress: Vec<String>,
}
