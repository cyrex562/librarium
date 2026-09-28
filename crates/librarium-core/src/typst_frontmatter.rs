//! Frontmatter for Typst notes (#145).
//!
//! Typst has no YAML frontmatter, so a Typst note keeps its metadata in a
//! labelled metadata call, valid Typst that compiles and can be queried:
//!
//! ```typst
//! #metadata((title: "Paper", tags: ("draft", "physics"))) <frontmatter>
//! ```
//!
//! It works like Markdown frontmatter: reading takes the block out of the
//! text and returns it as a JSON value, and writing puts it back at the top.
//! Only literal values (strings, numbers, booleans, `none`, arrays,
//! dictionaries) count. A metadata call with anything computed in it is left
//! in the text as ordinary code, so nothing is ever dropped.

use serde_json::{Map, Number, Value};
use typst_syntax::ast::{self, ArrayItem, DictItem, Expr};
use typst_syntax::{LinkedNode, SyntaxKind};

const LABEL: &str = "<frontmatter>";

/// Split Typst source into its frontmatter (if any) and the rest of the text.
pub fn parse(source: &str) -> (Option<Value>, String) {
    let root = typst_syntax::parse(source);
    let linked = LinkedNode::new(&root);
    let children: Vec<LinkedNode> = linked.children().collect();

    for (i, child) in children.iter().enumerate() {
        if child.kind() != SyntaxKind::FuncCall {
            continue;
        }
        let Some(call) = child.get().cast::<ast::FuncCall>() else {
            continue;
        };
        let Expr::Ident(ident) = call.callee() else {
            continue;
        };
        if ident.get() != "metadata" {
            continue;
        }
        // `#` directly before, `<frontmatter>` after (spaces allowed).
        if i == 0 || children[i - 1].kind() != SyntaxKind::Hash {
            continue;
        }
        let mut j = i + 1;
        while j < children.len()
            && children[j].kind() == SyntaxKind::Space
            && !children[j].get().full_text().contains('\n')
        {
            j += 1;
        }
        let Some(label) = children.get(j) else {
            continue;
        };
        if label.kind() != SyntaxKind::Label || label.get().full_text() != LABEL {
            continue;
        }
        let mut args = call.args().items();
        let (Some(ast::Arg::Pos(expr)), None) = (args.next(), args.next()) else {
            continue;
        };
        let Some(value @ Value::Object(_)) = literal(expr) else {
            continue;
        };

        // Remove `#metadata(..) <frontmatter>`, the rest of its line, and one
        // blank line after it (what `serialize` writes).
        let start = children[i - 1].offset();
        let mut end = label.range().end;
        let bytes = source.as_bytes();
        while end < bytes.len() && (bytes[end] == b' ' || bytes[end] == b'\t') {
            end += 1;
        }
        for _ in 0..2 {
            if source[end..].starts_with("\r\n") {
                end += 2;
            } else if source[end..].starts_with('\n') {
                end += 1;
            }
        }
        let rest = format!("{}{}", &source[..start], &source[end..]);
        return (Some(value), rest);
    }
    (None, source.to_string())
}

/// Put frontmatter back at the top of a Typst note. `None` or an empty
/// object writes no block. Round-trips with [`parse`] exactly.
pub fn serialize(frontmatter: Option<&Value>, body: &str) -> String {
    match frontmatter {
        Some(fm @ Value::Object(map)) if !map.is_empty() => {
            let block = format!("#metadata({}) {LABEL}", typst_value(fm));
            if body.is_empty() {
                format!("{block}\n")
            } else {
                format!("{block}\n\n{body}")
            }
        }
        _ => body.to_string(),
    }
}

/// A literal Typst expression as JSON; `None` if it isn't a plain literal.
fn literal(expr: Expr) -> Option<Value> {
    Some(match expr {
        Expr::Str(s) => Value::String(s.get().to_string()),
        Expr::Int(i) => Value::Number(i.get().into()),
        Expr::Float(f) => Value::Number(Number::from_f64(f.get())?),
        Expr::Bool(b) => Value::Bool(b.get()),
        Expr::None(_) | Expr::Auto(_) => Value::Null,
        Expr::Unary(u) if u.op() == ast::UnOp::Neg => match literal(u.expr())? {
            Value::Number(n) if n.is_i64() => Value::Number((-n.as_i64()?).into()),
            Value::Number(n) => Value::Number(Number::from_f64(-n.as_f64()?)?),
            _ => return None,
        },
        Expr::Array(array) => Value::Array(
            array
                .items()
                .map(|item| match item {
                    ArrayItem::Pos(e) => literal(e),
                    ArrayItem::Spread(_) => None,
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        Expr::Dict(dict) => {
            let mut map = Map::new();
            for item in dict.items() {
                let (key, value) = match item {
                    DictItem::Named(n) => (n.name().get().to_string(), n.expr()),
                    DictItem::Keyed(k) => match k.key() {
                        Expr::Str(s) => (s.get().to_string(), k.expr()),
                        _ => return None,
                    },
                    DictItem::Spread(_) => return None,
                };
                map.insert(key, literal(value)?);
            }
            Value::Object(map)
        }
        Expr::Parenthesized(p) => literal(p.expr())?,
        _ => return None,
    })
}

/// A Typst string literal.
pub fn typst_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A JSON value as a Typst literal.
pub fn typst_value(v: &Value) -> String {
    match v {
        Value::Null => "none".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            if n.is_f64() {
                let f = n.as_f64().unwrap_or(0.0);
                if f.is_finite() {
                    format!("{f:?}")
                } else {
                    "none".into()
                }
            } else {
                n.to_string()
            }
        }
        Value::String(s) => typst_string(s),
        Value::Array(items) => match items.len() {
            0 => "()".into(),
            1 => format!("({},)", typst_value(&items[0])),
            _ => format!(
                "({})",
                items.iter().map(typst_value).collect::<Vec<_>>().join(", ")
            ),
        },
        Value::Object(map) if map.is_empty() => "(:)".into(),
        Value::Object(map) => format!(
            "({})",
            map.iter()
                .map(|(k, v)| format!("{}: {}", typst_string(k), typst_value(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_metadata_block() {
        let src = "#metadata((title: \"Paper\", \"tags\": (\"a\", \"b\"), count: 3, ratio: -0.5, draft: true, owner: none, nested: (k: (1, 2)))) <frontmatter>\n\n= Paper\n\nBody.\n";
        let (fm, body) = parse(src);
        assert_eq!(
            fm,
            Some(
                json!({"title": "Paper", "tags": ["a", "b"], "count": 3, "ratio": -0.5, "draft": true, "owner": null, "nested": {"k": [1, 2]}})
            )
        );
        assert_eq!(body, "= Paper\n\nBody.\n");
    }

    #[test]
    fn round_trips_exactly() {
        let fm = json!({"title": "Say \"hi\"", "tags": ["x"], "n": 1.5});
        for body in [
            "= T\n\nText\n",
            "",
            "\nstarts blank",
            "#set document(title: \"x\")\nrest",
        ] {
            let written = serialize(Some(&fm), body);
            let (read_fm, read_body) = parse(&written);
            assert_eq!(read_fm.as_ref(), Some(&fm), "{written}");
            assert_eq!(read_body, body, "{written:?}");
        }
        assert_eq!(serialize(None, "x"), "x");
        assert_eq!(serialize(Some(&json!({})), "x"), "x");
    }

    #[test]
    fn a_block_further_down_is_found_and_removed() {
        let src = "#set document(title: \"T\")\n#metadata((tags: (\"z\",))) <frontmatter>\n\nBody";
        let (fm, body) = parse(src);
        assert_eq!(fm, Some(json!({"tags": ["z"]})));
        assert_eq!(body, "#set document(title: \"T\")\nBody");
    }

    #[test]
    fn leaves_non_frontmatter_alone() {
        for src in [
            "#metadata((title: x)) <frontmatter>\nBody", // computed value
            "#metadata((title: \"a\")) <other>\nBody",   // different label
            "#metadata((title: \"a\"))\nBody",           // no label
            "#metadata(\"just a string\") <frontmatter>\nBody", // not a dictionary
            "No metadata at all",
        ] {
            let (fm, body) = parse(src);
            assert_eq!(fm, None, "{src}");
            assert_eq!(body, src);
        }
    }
}
