//! Source-preserving removal, including the dependent calls the map editor updates.
use super::*;
use std::ops::Range;

fn call_range(
    source: &str,
    tokens: &[Token],
    before: &Program,
    index: usize,
) -> Result<Range<usize>, String> {
    let old = &before.statements[index];
    let Statement::Call(call) = old else {
        return Err("Cannot remove a definition".into());
    };
    let name = call.assigns.first().unwrap_or(&call.name);
    for (i, token) in tokens.iter().enumerate().filter(|(_, t)| t.text == *name) {
        let Some(end) = tokens[i..].iter().find(|t| t.kind == TokenKind::Semicolon) else {
            continue;
        };
        let range = token.span.offset..end.span.offset + 1;
        if parse(&source[..range.start])
            .ok()
            .is_some_and(|p| p.statements.len() == index)
            && parse(&source[range.clone()])
                .ok()
                .is_some_and(|p| p.statements == vec![old.clone()])
        {
            return Ok(range);
        }
    }
    Err("Could not locate the authored road call".into())
}

pub(super) fn removed_source(
    source: &str,
    before: &Program,
    after: &Program,
) -> Result<String, String> {
    if parse(source).map_err(|e| e.to_string())? != *before {
        return Err("Map source changed".into());
    }
    let (a, b) = (&before.statements, &after.statements);
    let (n, m) = (a.len(), b.len());
    let mut lengths = vec![vec![0_u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lengths[i][j] = if a[i] == b[j] {
                1 + lengths[i + 1][j + 1]
            } else {
                lengths[i + 1][j].max(lengths[i][j + 1])
            };
        }
    }
    let mut anchors = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            anchors.push((i, j));
            i += 1;
            j += 1;
        } else if lengths[i + 1][j] >= lengths[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    anchors.push((n, m));
    let tokens = tokenize(source).map_err(|e| e.to_string())?;
    let mut edits = Vec::new();
    let (mut old, mut new) = (0, 0);
    for (next_old, next_new) in anchors {
        if old != next_old || new != next_new {
            let replacement = Program {
                statements: b[new..next_new].to_vec(),
            }
            .to_road_source();
            if old == next_old {
                let start = if old == n {
                    source.len()
                } else {
                    call_range(source, &tokens, before, old)?.start
                };
                edits.push((start..start, format!("\n{replacement}")));
            } else {
                for index in old..next_old {
                    edits.push((
                        call_range(source, &tokens, before, index)?,
                        if index == old {
                            replacement.clone()
                        } else {
                            String::new()
                        },
                    ));
                }
            }
        }
        old = next_old + 1;
        new = next_new + 1;
    }
    edits.sort_by_key(|(r, _)| r.start);
    let mut output = source.to_owned();
    for (range, value) in edits.into_iter().rev() {
        output.replace_range(range, &value);
    }
    if parse(&output).map_err(|e| e.to_string())? != *after {
        return Err("Road deletion source validation failed".into());
    }
    Ok(output)
}
