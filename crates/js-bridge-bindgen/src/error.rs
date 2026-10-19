use std::path::PathBuf;

/// `js-bridge-bindgen` 的错误类型
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// TS 入口文件不存在
    #[error("入口文件不存在: {0}")]
    EntryNotFound(PathBuf),

    /// TS 入口文件读取失败
    #[error("读取入口文件失败: {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// TS 源码解析失败（oxc_parser 报错）
    #[error("TS 源码解析失败:\n{0}")]
    ParseFailed(String),

    /// 模式校验失败：TS 源码不符合工厂模式要求
    #[error("TS 源码不符合工厂模式要求:\n{0}")]
    ValidationFailed(String),

    /// 代码生成失败
    #[error("代码生成失败: {0}")]
    CodegenFailed(String),

    /// 输出目录创建失败
    #[error("创建输出目录失败: {path}")]
    OutDirCreateFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// 输出文件写入失败
    #[error("写入输出文件失败: {path}")]
    OutputWriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// 校验错误详情，用于构建结构化的错误报告
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// 错误级别
    pub level: ValidationLevel,
    /// 错误规则编号
    pub rule: &'static str,
    /// 错误消息
    pub message: String,
    /// 可选的源码位置（行号，从 1 开始）
    pub line: Option<usize>,
    /// 可选的修复提示
    pub hint: Option<String>,
}

/// 校验错误级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLevel {
    /// 错误：必须修复，否则无法生成绑定代码
    Error,
    /// 警告：建议修复，不影响代码生成
    Warning,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self.level {
            ValidationLevel::Error => "error",
            ValidationLevel::Warning => "warning",
        };
        write!(
            f,
            "{level}[js-bridge-bindgen]: {rule} — {message}",
            rule = self.rule,
            message = self.message
        )?;
        if let Some(line) = self.line {
            write!(f, " (line {line})")?;
        }
        if let Some(hint) = &self.hint {
            write!(f, "\n  = hint: {hint}")?;
        }
        Ok(())
    }
}

/// 校验结果
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn add_error(&mut self, rule: &'static str, message: impl Into<String>) {
        self.errors.push(ValidationError {
            level: ValidationLevel::Error,
            rule,
            message: message.into(),
            line: None,
            hint: None,
        });
    }

    pub fn add_error_with_hint(
        &mut self,
        rule: &'static str,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) {
        self.errors.push(ValidationError {
            level: ValidationLevel::Error,
            rule,
            message: message.into(),
            line: None,
            hint: Some(hint.into()),
        });
    }

    pub fn add_warning(&mut self, rule: &'static str, message: impl Into<String>) {
        self.warnings.push(ValidationError {
            level: ValidationLevel::Warning,
            rule,
            message: message.into(),
            line: None,
            hint: None,
        });
    }

    pub fn to_error_message(&self) -> String {
        let mut msg = String::new();
        for err in &self.errors {
            msg.push_str(&err.to_string());
            msg.push('\n');
        }
        for warn in &self.warnings {
            msg.push_str(&warn.to_string());
            msg.push('\n');
        }
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_display_with_hint() {
        let err = ValidationError {
            level: ValidationLevel::Error,
            rule: "E001",
            message: "缺少 declare global 块".into(),
            line: None,
            hint: Some("添加 declare global 块".into()),
        };
        let s = err.to_string();
        assert!(s.contains("error[js-bridge-bindgen]: E001"));
        assert!(s.contains("缺少 declare global 块"));
        assert!(s.contains("hint: 添加 declare global 块"));
    }

    #[test]
    fn test_validation_error_display_with_line() {
        let err = ValidationError {
            level: ValidationLevel::Warning,
            rule: "W001",
            message: "无法关联实例变量".into(),
            line: Some(42),
            hint: None,
        };
        let s = err.to_string();
        assert!(s.contains("warning[js-bridge-bindgen]: W001"));
        assert!(s.contains("无法关联实例变量"));
        assert!(s.contains("line 42"));
    }

    #[test]
    fn test_validation_result_is_valid() {
        let result = ValidationResult::default();
        assert!(result.is_valid());

        let mut result = ValidationResult::default();
        result.add_error("E001", "some error");
        assert!(!result.is_valid());
    }

    #[test]
    fn test_validation_result_to_error_message() {
        let mut result = ValidationResult::default();
        result.add_error_with_hint("E001", "缺少 declare global", "添加 declare global");
        result.add_warning("W001", "无法关联实例变量");

        let msg = result.to_error_message();
        assert!(msg.contains("E001"));
        assert!(msg.contains("W001"));
        assert!(msg.contains("hint:"));
    }
}