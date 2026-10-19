//! 基础语法高亮 tokenizer。
//!
//! 提供正则驱动的 token 级高亮，支持 Typst 和 Markdown。
//! 后续将替换为 LSP 语义 token，此模块作为 fallback。

/// Token 语义类型。
#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
    Function,
    Operator,
    Punctuation,
    Type,
    Heading,
    Markup,
    Raw,
    Label,
    Error,
}

/// 单个 token。
#[derive(Clone, Debug)]
pub struct Token {
    pub text: String,
    pub kind: TokenKind,
}

/// 根据文件扩展名选择 tokenizer 并拆分行文本为 token。
pub fn tokenize(line: &str, file_ext: &str) -> Vec<Token> {
    match file_ext {
        "typ" => tokenize_typst(line),
        "md" => tokenize_markdown(line),
        _ => tokenize_plain(line),
    }
}

fn tokenize_plain(line: &str) -> Vec<Token> {
    vec![Token {
        text: line.to_string(),
        kind: TokenKind::Plain,
    }]
}

/// Typst 关键字列表。
const TYPST_KEYWORDS: &[&str] = &[
    "let", "set", "show", "import", "export", "if", "else", "for", "while", "return", "break",
    "continue", "context", "enum", "struct", "func",
];

/// Typst 内置类型列表。
const TYPST_TYPES: &[&str] = &[
    "int",
    "float",
    "bool",
    "str",
    "content",
    "array",
    "dict",
    "bytes",
    "label",
    "none",
    "auto",
    "length",
    "angle",
    "ratio",
    "relative",
    "fraction",
    "color",
    "datetime",
    "duration",
    "alignment",
    "direction",
];

/// Typst 行内 token 化。
///
/// 按优先级依次匹配：注释 → 字符串 → 数字 → 标识符 → 其他。
fn tokenize_typst(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // 行注释 //
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '/' {
            let start = i;
            while i < chars.len() {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Comment,
            });
            continue;
        }

        // 块注释 /* ... */
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < chars.len() {
                i += 2;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Comment,
            });
            continue;
        }

        // 字符串 "..."
        if chars[i] == '"' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                }
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::String,
            });
            continue;
        }

        // 数字
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Number,
            });
            continue;
        }

        // 标识符 / 关键字 / 类型 / 函数调用
        if chars[i] == '_' || chars[i].is_alphabetic() {
            let start = i;
            while i < chars.len()
                && (chars[i] == '_' || chars[i].is_alphanumeric() || chars[i] == '-')
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let kind = if TYPST_KEYWORDS.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if TYPST_TYPES.contains(&word.as_str()) {
                TokenKind::Type
            } else if i < chars.len() && chars[i] == '(' {
                TokenKind::Function
            } else if word.starts_with('<') && word.ends_with('>') {
                TokenKind::Label
            } else {
                TokenKind::Plain
            };
            tokens.push(Token { text: word, kind });
            continue;
        }

        // 标记语法 #heading, #let, #import, #show, #set, #context
        if chars[i] == '#' && i + 1 < chars.len() && chars[i + 1].is_alphabetic() {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i] == '_' || chars[i].is_alphanumeric() || chars[i] == '-')
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let after_hash: String = chars[start + 1..i].iter().collect();
            let kind = if after_hash == "heading" {
                TokenKind::Heading
            } else if TYPST_KEYWORDS.contains(&after_hash.as_str()) {
                TokenKind::Keyword
            } else if i < chars.len() && chars[i] == '(' {
                TokenKind::Function
            } else {
                TokenKind::Markup
            };
            tokens.push(Token { text: word, kind });
            continue;
        }

        // @label
        if chars[i] == '@' {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i] == '_' || chars[i].is_alphanumeric() || chars[i] == '-')
            {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Label,
            });
            continue;
        }

        // $...$ 数学模式（行内简化处理）
        if chars[i] == '$' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '$' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Markup,
            });
            continue;
        }

        // 运算符
        if is_operator_char(chars[i]) {
            let start = i;
            while i < chars.len() && is_operator_char(chars[i]) {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Operator,
            });
            continue;
        }

        // 标点
        if is_punctuation_char(chars[i]) {
            tokens.push(Token {
                text: chars[i].to_string(),
                kind: TokenKind::Punctuation,
            });
            i += 1;
            continue;
        }

        // 其他字符
        tokens.push(Token {
            text: chars[i].to_string(),
            kind: TokenKind::Plain,
        });
        i += 1;
    }

    tokens
}

fn is_operator_char(c: char) -> bool {
    matches!(
        c,
        '+' | '-' | '*' | '/' | '%' | '=' | '!' | '<' | '>' | '&' | '|' | '^' | '~'
    )
}

fn is_punctuation_char(c: char) -> bool {
    matches!(
        c,
        '(' | ')' | '[' | ']' | '{' | '}' | ':' | ';' | ',' | '.' | '@'
    )
}

/// Markdown 行内 token 化。
fn tokenize_markdown(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    // 检测标题行
    let heading_level = leading_hashes(&chars);
    if heading_level > 0 {
        let hash_end = heading_level;
        tokens.push(Token {
            text: chars[..hash_end].iter().collect(),
            kind: TokenKind::Heading,
        });
        if hash_end < chars.len() && chars[hash_end] == ' ' {
            tokens.push(Token {
                text: " ".to_string(),
                kind: TokenKind::Heading,
            });
            i = hash_end + 1;
        } else {
            i = hash_end;
        }

        while i < chars.len() {
            tokens.push(Token {
                text: chars[i].to_string(),
                kind: TokenKind::Heading,
            });
            i += 1;
        }
        return tokens;
    }

    while i < chars.len() {
        // 行注释（Markdown 中不常见，但某些方言支持）
        // HTML 注释 <!-- ... -->
        if i + 3 < chars.len()
            && chars[i] == '<'
            && chars[i + 1] == '!'
            && chars[i + 2] == '-'
            && chars[i + 3] == '-'
        {
            let start = i;
            i += 4;
            while i + 2 < chars.len()
                && !(chars[i] == '-' && chars[i + 1] == '-' && chars[i + 2] == '>')
            {
                i += 1;
            }
            if i + 2 < chars.len() {
                i += 3;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Comment,
            });
            continue;
        }

        // 行内代码 `...`
        if chars[i] == '`' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '`' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Raw,
            });
            continue;
        }

        // 粗体/斜体标记 **...** 或 *...*
        if chars[i] == '*' {
            let start = i;
            while i < chars.len() && chars[i] == '*' {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Markup,
            });
            continue;
        }

        // 链接 [text](url) 简化处理
        if chars[i] == '[' {
            let start = i;
            while i < chars.len() && chars[i] != ']' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Markup,
            });
            continue;
        }

        if chars[i] == ']' && i + 1 < chars.len() && chars[i + 1] == '(' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != ')' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Markup,
            });
            continue;
        }

        // 数字
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            tokens.push(Token {
                text: chars[start..i].iter().collect(),
                kind: TokenKind::Number,
            });
            continue;
        }

        tokens.push(Token {
            text: chars[i].to_string(),
            kind: TokenKind::Plain,
        });
        i += 1;
    }

    tokens
}

fn leading_hashes(chars: &[char]) -> usize {
    let mut count = 0;
    for &c in chars {
        if c == '#' {
            count += 1;
        } else {
            break;
        }
    }
    if count > 0 && count <= 6 && count < chars.len() && chars[count] == ' ' {
        count
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typst_keyword() {
        let tokens = tokenize_typst("#let x = 1");
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Keyword && t.text == "#let")
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Number && t.text == "1")
        );
    }

    #[test]
    fn test_typst_comment() {
        let tokens = tokenize_typst("// hello");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Comment));
    }

    #[test]
    fn test_typst_string() {
        let tokens = tokenize_typst(r#""hello""#);
        assert!(tokens.iter().any(|t| t.kind == TokenKind::String));
    }

    #[test]
    fn test_typst_function() {
        let tokens = tokenize_typst("text(red)[hi]");
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Function && t.text == "text")
        );
    }

    #[test]
    fn test_markdown_heading() {
        let tokens = tokenize_markdown("## Hello");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Heading));
    }

    #[test]
    fn test_markdown_code() {
        let tokens = tokenize_markdown("`code`");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Raw));
    }
}