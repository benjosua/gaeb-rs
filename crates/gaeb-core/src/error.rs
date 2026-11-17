use thiserror::Error;

#[derive(Debug, Error)]
pub enum GaebError {
    #[error("input exceeds configured {limit} byte limit")]
    InputTooLarge { limit: usize },
    #[error("XML contains forbidden external-resource declaration")]
    ExternalResource,
    #[error("XML nesting exceeds configured {limit} level limit")]
    NestingLimit { limit: usize },
    #[error("XML text node exceeds configured {limit} byte limit")]
    TextLimit { limit: usize },
    #[error("document exceeds configured {limit} item limit")]
    ItemLimit { limit: usize },
    #[error("malformed XML: {0}")]
    Xml(String),
    #[error("unsupported document: {0}")]
    Unsupported(String),
    #[error("strict validation failed: {0}")]
    Validation(String),
    #[error("invalid decimal in {field}: {value:?}")]
    InvalidDecimal { field: &'static str, value: String },
    #[error("invalid write profile: {0}")]
    InvalidWriteProfile(String),
    #[error("output failed: {0}")]
    Output(#[from] std::io::Error),
}
