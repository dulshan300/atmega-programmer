use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A validation or build message with an optional location that the UI can
/// navigate to (a graph node, a pin, a configuration section or a source line).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

impl Diagnostic {
    pub fn new(severity: Severity, code: &str, message: impl Into<String>) -> Self {
        Diagnostic {
            severity,
            code: code.to_string(),
            message: message.into(),
            node_id: None,
            pin: None,
            section: None,
            file: None,
            line: None,
            column: None,
        }
    }
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, code, message)
    }
    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, code, message)
    }
    pub fn info(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Info, code, message)
    }
    pub fn at_node(mut self, node_id: &str) -> Self {
        self.node_id = Some(node_id.to_string());
        self
    }
    pub fn at_pin(mut self, pin: &str) -> Self {
        self.pin = Some(pin.to_string());
        self
    }
    pub fn in_section(mut self, section: &str) -> Self {
        self.section = Some(section.to_string());
        self
    }
}

pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity == Severity::Error)
}

/// Stable ordering so that identical inputs always produce identical output.
pub fn sort_diagnostics(diags: &mut [Diagnostic]) {
    diags.sort_by(|a, b| {
        (a.severity, &a.section, &a.node_id, &a.pin, &a.code, &a.message)
            .cmp(&(b.severity, &b.section, &b.node_id, &b.pin, &b.code, &b.message))
    });
}
