//! UI error 型
//!
//! [`UiLaw`](crate::UiLaw) 違反 + 内部 layout エラー の 2 系統 共通型

/// UI primitive / layout / law の共通 error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiError {
    /// layout 引数が invalid (子要素 0 個の grid 等)
    LayoutInvalid(&'static str),

    /// primitive 引数が invalid (負の width / 極小 radius 等)
    PrimitiveInvalid(&'static str),
}

impl core::fmt::Display for UiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::LayoutInvalid(msg) => write!(f, "layout invalid: {msg}"),
            Self::PrimitiveInvalid(msg) => write!(f, "primitive invalid: {msg}"),
        }
    }
}

impl std::error::Error for UiError {}

/// 個別 UI law 違反
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiViolation {
    /// 違反した rule 名 (`TouchTargetTooSmall` / `ContrastTooLow` / `RadiusOversized`)
    pub rule: &'static str,
    /// 詳細メッセージ (実測値 vs 閾値)
    pub detail: String,
}

impl core::fmt::Display for UiViolation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "UI rule '{}' violated: {}", self.rule, self.detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_invalid_display() {
        let e = UiError::LayoutInvalid("empty children");
        assert_eq!(format!("{e}"), "layout invalid: empty children");
    }

    #[test]
    fn primitive_invalid_display() {
        let e = UiError::PrimitiveInvalid("width must be > 0");
        assert_eq!(format!("{e}"), "primitive invalid: width must be > 0");
    }

    #[test]
    fn violation_display() {
        let v = UiViolation {
            rule: "TouchTargetTooSmall",
            detail: "32.0 px < 44.0 px min".to_string(),
        };
        assert!(format!("{v}").contains("TouchTargetTooSmall"));
        assert!(format!("{v}").contains("32.0 px"));
    }
}
