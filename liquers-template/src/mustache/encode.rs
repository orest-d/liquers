//! Writing a Mustache template back as text. Every tag is written with the delimiters in force at
//! its position, a standalone tag gets its line back, and an `Invalid` node its source, so
//! `parse(encode(t)) == t`.

use crate::mustache::ast::{
    ApplyTarget, ClassicPiece, CloseForm, Element, Escape, MustacheTemplate, PartialTarget,
    QueryBody, QueryTag, SectionHead, Standalone,
};

struct Writer {
    out: String,
    open: String,
    close: String,
}

impl Writer {
    fn tag(&mut self, content: &str, standalone: &Option<Standalone>) {
        if let Some(s) = standalone {
            self.out.push_str(&s.indent);
        }
        self.out.push_str(&self.open);
        self.out.push_str(content);
        self.out.push_str(&self.close);
        if let Some(s) = standalone {
            self.out.push_str(&s.newline);
        }
    }

    fn query(&self, tag: &QueryTag) -> String {
        match &tag.body {
            QueryBody::Invalid(i) => i.source.clone(),
            QueryBody::Hygienic(t) => format!(
                "${} {}",
                apply(&tag.apply),
                t.encode_with(&self.open, &self.close)
            ),
            QueryBody::Classic(c) => {
                let mut body = String::new();
                for piece in &c.pieces {
                    match piece {
                        ClassicPiece::Text(t, _) => body.push_str(t),
                        ClassicPiece::Slot(s) => {
                            body.push_str(&self.open);
                            body.push_str(&s.name.encode());
                            body.push_str(&self.close);
                        }
                    }
                }
                format!("$${} {}", apply(&tag.apply), body)
            }
        }
    }

    fn elements(&mut self, elements: &[Element]) {
        for element in elements {
            match element {
                Element::Text(t) => self.out.push_str(&t.text),
                Element::Variable(v) => match v.escape {
                    Escape::Escaped => self.tag(&v.name.encode(), &None),
                    Escape::Ampersand => self.tag(&format!("&{}", v.name.encode()), &None),
                    Escape::Triple => self.tag(&format!("{{{}}}", v.name.encode()), &None),
                },
                Element::Section(s) => {
                    let head = match &s.head {
                        SectionHead::Name(n) => n.encode(),
                        SectionHead::Query(q) => self.query(q),
                        SectionHead::Invalid(i) => i.source.clone(),
                    };
                    let sigil = if s.inverted { '^' } else { '#' };
                    self.tag(&format!("{sigil}{head}"), &s.standalone);
                    self.elements(&s.elements);
                    let close = match &s.close.form {
                        CloseForm::Bare => "/".to_string(),
                        CloseForm::Name(n) => format!("/{}", n.encode()),
                        CloseForm::Hygienic => "/$".to_string(),
                        CloseForm::Classic => "/$$".to_string(),
                    };
                    self.tag(&close, &s.close.standalone);
                }
                Element::Partial(p) => {
                    let target = match &p.target {
                        PartialTarget::Key(k) => k.encode(),
                        PartialTarget::Query(q) => self.query(q),
                        PartialTarget::Invalid(i) => i.source.clone(),
                    };
                    self.tag(&format!(">{target}"), &p.standalone);
                }
                Element::Query(q) => {
                    let body = self.query(q);
                    match q.escape {
                        Escape::Escaped => self.tag(&body, &None),
                        Escape::Ampersand | Escape::Triple => self.tag(&format!("&{body}"), &None),
                    }
                }
                Element::Comment(c) => self.tag(&format!("!{}", c.text), &c.standalone),
                Element::SetDelimiter(d) => {
                    self.tag(&format!("={} {}=", d.open, d.close), &d.standalone);
                    self.open = d.open.clone();
                    self.close = d.close.clone();
                }
                Element::Invalid(i) => self.out.push_str(&i.source),
            }
        }
    }
}

fn apply(target: &Option<ApplyTarget>) -> String {
    match target {
        None => String::new(),
        Some(ApplyTarget::Current(_)) => ".".to_string(),
        Some(ApplyTarget::Name(n)) => format!(".{}", n.encode()),
    }
}

/// The canonical text of `template`.
pub(crate) fn encode(template: &MustacheTemplate) -> String {
    let mut writer = Writer {
        out: String::new(),
        open: "{{".to_string(),
        close: "}}".to_string(),
    };
    writer.elements(&template.elements);
    writer.out
}

#[cfg(test)]
mod tests {
    use crate::mustache::parse::parse_mustache;

    const EXAMPLE_3: &str = "{{items.0.title}} {{items[i].title}} {{items[-1].title}} {{m[\"a b\"]}} [{{items.5.title}}]\n{{items[5].title}} {{$ ns-fixture/fail}}\n{{#$ ns-fixture/fail}}never{{/}}\n";

    pub(crate) fn round_trip(text: &str) {
        let parsed = parse_mustache(text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        let encoded = parsed.encode();
        let again = parse_mustache(&encoded).unwrap_or_else(|e| panic!("{encoded:?}: {e}"));
        assert_eq!(again, parsed, "{text:?} encoded as {encoded:?}");
        assert_eq!(again.encode(), encoded, "encode is idempotent for {text:?}");
    }

    #[test]
    fn encode_round_trips() {
        for text in [
            EXAMPLE_3,
            "",
            "plain",
            "a {{x}} {{{y}}} {{&z}} {{! note }}\n",
            "  {{#list}}\n  - {{.}}\n  {{/list}}\n",
            "{{^empty}}none{{/empty}}",
            "{{=<% %>=}}<% x %> <%#s%>a<%/s%>\n<%={{ }}=%>{{y}}",
            "{{> partial}}\n  {{>other}}\n",
            "{{$ cmd-{{x}}/q}} {{$$ cmd-{{x}}/q}} {{&$ -R/a/b.txt}} {{$. to_text}} {{$.row.n to_text}}",
            "{{#$ -R-key/x/-/f-{{a}}}}{{title}}{{/$}}{{>$ to_text}}{{>$$.row q}}",
            "{{a..b}} {{a b}} {{#c..d}}x{{/}} {{> }} {{{$ q}}} {{/}} {{$ cmd/~}} {{$x}}",
            "\r\n{{#a}}\r\n{{/a}}\r\n",
        ] {
            round_trip(text);
        }
        for spec in ["comments", "delimiters", "interpolation", "inverted", "partials", "sections"] {
            let path = format!("{}/tests/spec/{spec}.json", env!("CARGO_MANIFEST_DIR"));
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let json: serde_json::Value = serde_json::from_str(&text).expect("spec json");
            for test in json["tests"].as_array().into_iter().flatten() {
                if let Some(t) = test["template"].as_str() {
                    round_trip(t);
                }
                for partial in test["partials"].as_object().into_iter().flatten() {
                    if let Some(t) = partial.1.as_str() {
                        round_trip(t);
                    }
                }
            }
        }
    }
}
