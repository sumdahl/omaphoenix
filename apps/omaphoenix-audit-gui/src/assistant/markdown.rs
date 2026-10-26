use crate::ui::AiBlock;

pub(crate) fn md_blocks(md: &str) -> Vec<AiBlock> {
    let clean = |t: &str| t.replace("**", "").replace('`', "");
    let mut out = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let flush = |para: &mut Vec<String>, out: &mut Vec<AiBlock>| {
        if !para.is_empty() {
            out.push(AiBlock {
                kind: "p".into(),
                text: clean(&para.join(" ")).into(),
            });
            para.clear();
        }
    };
    let mut lines = md.lines();
    while let Some(raw) = lines.next() {
        let t = raw.trim();
        if t.starts_with("```") {
            flush(&mut para, &mut out);
            let code: Vec<&str> = lines
                .by_ref()
                .take_while(|l| !l.trim_start().starts_with("```"))
                .collect();
            out.push(AiBlock {
                kind: "code".into(),
                text: code.join("\n").into(),
            });
        } else if t.is_empty() {
            flush(&mut para, &mut out);
        } else if t.starts_with('#') {
            flush(&mut para, &mut out);
            out.push(AiBlock {
                kind: "h".into(),
                text: clean(t.trim_start_matches('#').trim()).into(),
            });
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            flush(&mut para, &mut out);
            out.push(AiBlock {
                kind: "li".into(),
                text: format!("•  {}", clean(item)).into(),
            });
        } else if t
            .split_once(". ")
            .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        {
            flush(&mut para, &mut out);
            out.push(AiBlock {
                kind: "li".into(),
                text: clean(t).into(),
            });
        } else {
            para.push(t.to_owned());
        }
    }
    flush(&mut para, &mut out);
    out
}
