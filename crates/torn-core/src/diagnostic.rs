use core::fmt;

/// The importance of a framework diagnostic.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSeverity {
    /// A recoverable condition that may deserve attention.
    Warning,
    /// A failed framework operation or invalid application input.
    Error,
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

/// Structured information about a recoverable framework failure.
///
/// Diagnostics are deliberately owned and dependency-free so applications can
/// collect them, log them, or turn them into test failures without depending on
/// a specific logging framework.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    severity: DiagnosticSeverity,
    component: String,
    message: String,
}

impl Diagnostic {
    /// Creates a diagnostic with the supplied severity, component, and message.
    #[must_use]
    pub fn new(
        severity: DiagnosticSeverity,
        component: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            component: component.into(),
            message: message.into(),
        }
    }

    /// Creates an error diagnostic.
    #[must_use]
    pub fn error(component: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Error, component, message)
    }

    /// Creates a warning diagnostic.
    #[must_use]
    pub fn warning(component: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Warning, component, message)
    }

    /// Returns the diagnostic severity.
    #[must_use]
    pub const fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    /// Returns the framework component that produced the diagnostic.
    #[must_use]
    pub fn component(&self) -> &str {
        &self.component
    }

    /// Returns the human-readable diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} [{}]: {}",
            self.severity, self.component, self.message
        )
    }
}

/// Receives diagnostics emitted by Torn components.
///
/// Implement this trait to forward diagnostics to an application logger or a
/// test assertion. `Vec<Diagnostic>` collects them. Any `FnMut(&Diagnostic)`
/// can be used directly as a reporter.
pub trait DiagnosticReporter {
    /// Receives one diagnostic.
    fn report(&mut self, diagnostic: &Diagnostic);
}

impl DiagnosticReporter for Vec<Diagnostic> {
    fn report(&mut self, diagnostic: &Diagnostic) {
        self.push(diagnostic.clone());
    }
}

impl<F> DiagnosticReporter for F
where
    F: FnMut(&Diagnostic),
{
    fn report(&mut self, diagnostic: &Diagnostic) {
        self(diagnostic);
    }
}

/// A reporter that turns every diagnostic into a panic.
///
/// This is useful in tests that treat framework diagnostics as failures.
#[derive(Clone, Copy, Debug, Default)]
pub struct PanicOnDiagnostic;

impl DiagnosticReporter for PanicOnDiagnostic {
    fn report(&mut self, diagnostic: &Diagnostic) {
        panic!("{diagnostic}");
    }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, DiagnosticReporter, DiagnosticSeverity};

    #[test]
    fn vec_reporter_collects_owned_diagnostics() {
        let mut diagnostics = Vec::new();
        diagnostics.report(&Diagnostic::error("test", "broken"));

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity(), DiagnosticSeverity::Error);
        assert_eq!(diagnostics[0].component(), "test");
        assert_eq!(diagnostics[0].message(), "broken");
    }
}
