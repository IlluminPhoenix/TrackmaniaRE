//! Minimal JSON writer (pretty printing, stable key order).

pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn obj() -> Json {
        Json::Obj(Vec::new())
    }

    pub fn set(&mut self, key: &str, value: Json) -> &mut Self {
        if let Json::Obj(items) = self {
            items.push((key.to_string(), value));
        }
        self
    }

    pub fn str(s: impl Into<String>) -> Json {
        Json::Str(s.into())
    }

    pub fn opt_str(s: Option<impl Into<String>>) -> Json {
        match s {
            Some(s) => Json::Str(s.into()),
            None => Json::Null,
        }
    }

    pub fn opt_int<T: Into<i64>>(v: Option<T>) -> Json {
        match v {
            Some(v) => Json::Int(v.into()),
            None => Json::Null,
        }
    }

    pub fn opt_f32(v: Option<f32>) -> Json {
        match v {
            Some(v) => Json::Float(v as f64),
            None => Json::Null,
        }
    }

    pub fn floats(v: &[f32]) -> Json {
        Json::Arr(v.iter().map(|f| Json::Float(*f as f64)).collect())
    }

    pub fn to_pretty(&self) -> String {
        let mut s = String::new();
        write_value(&mut s, self, 0);
        s.push('\n');
        s
    }
}

fn write_value(out: &mut String, v: &Json, indent: usize) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Int(i) => out.push_str(&i.to_string()),
        Json::UInt(u) => out.push_str(&u.to_string()),
        Json::Float(f) => out.push_str(&fmt_float(*f)),
        Json::Str(s) => write_string(out, s),
        Json::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                push_indent(out, indent + 1);
                write_value(out, item, indent + 1);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(out, indent);
            out.push(']');
        }
        Json::Obj(items) => {
            if items.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (i, (k, item)) in items.iter().enumerate() {
                push_indent(out, indent + 1);
                write_string(out, k);
                out.push_str(": ");
                write_value(out, item, indent + 1);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(out, indent);
            out.push('}');
        }
    }
}

fn fmt_float(f: f64) -> String {
    if !f.is_finite() {
        // JSON has no NaN/Infinity; emitting the string "NaN" would silently
        // change the value's type, so write null.
        return "null".to_string();
    }
    // Trim trailing zeros but keep at least one decimal.
    let s = format!("{f:.6}");
    let s = s.trim_end_matches('0');
    let s = s.strip_suffix('.').unwrap_or(s);
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
