use crate::widget::{SignalSpan, text::span::SPOILER};
use presage::proto::{
    BodyRange,
    body_range::{AssociatedValue, Style},
};
use std::mem::take;

/// bold: **text**
/// italic:  *text*
/// spoiler: ||text||
/// strikethrough: ~~text~~
/// monospace: `text`
/// escaping: \*test\*
pub fn markdown_to_body_ranges(input: &str) -> (String, Vec<BodyRange>) {
    let mut bold = None;
    let mut italic = None;
    let mut spoiler = None;
    let mut strikethrough = None;
    let mut monospace = None;

    let mut iter = input.chars().peekable();

    let mut output = String::new();
    let mut count = 0; // count codepoints, not bytes
    let mut ranges = Vec::new();

    let mut push_range = |count, pos, style| {
        ranges.push(BodyRange {
            start: Some(pos),
            length: Some(count - pos),
            associated_value: Some(AssociatedValue::Style(style as i32)),
        });
    };

    while let Some(ch) = iter.next() {
        match ch {
            '*' if iter.peek() == Some(&'*') => {
                // we are starting or ending a bold range
                iter.next();

                if let Some(bold) = bold.take() {
                    push_range(count, bold, Style::Bold);
                } else {
                    bold = Some(count);
                }
            }
            '*' => {
                // we are starting or ending an italic range

                if let Some(italic) = italic.take() {
                    push_range(count, italic, Style::Italic);
                } else {
                    italic = Some(count);
                }
            }
            '|' if iter.peek() == Some(&'|') => {
                // we are starting or ending a spoiler range
                iter.next();

                if let Some(spoiler) = spoiler.take() {
                    push_range(count, spoiler, Style::Spoiler);
                } else {
                    spoiler = Some(count);
                }
            }
            '~' if iter.peek() == Some(&'~') => {
                // we are starting or ending a strikethrough range
                iter.next();

                if let Some(strikethrough) = strikethrough.take() {
                    push_range(count, strikethrough, Style::Strikethrough);
                } else {
                    strikethrough = Some(count);
                }
            }
            '`' => {
                // we are starting or ending a monospace range

                if let Some(monospace) = monospace.take() {
                    push_range(count, monospace, Style::Monospace);
                } else {
                    monospace = Some(count);
                }
            }
            '\\' if matches!(iter.peek(), Some(&'*' | &'|' | &'~' | &'¸' | &'\\')) => {
                // we are escaping a character
                output.push(iter.next().unwrap());
                count += 1;
            }
            _ => {
                output.push(ch);
                count += 1;
            }
        }
    }

    // we skipped characters that are part of unfinished spans
    // re-insert those, and update spans accordingly

    let mut insert_at = |nth, str| {
        let i = output
            .char_indices()
            .nth(nth as usize)
            .map_or(count as usize, |i| i.0);
        output.insert_str(i, str);

        let diff = str.chars().count() as u32;
        count += diff;
        diff
    };

    let update_pos = |last: Option<&mut _>, pos, diff| {
        if let Some(last) = last {
            if *last > pos {
                *last += diff;
            }
        }
    };

    let mut update_ranges = |pos, diff| {
        for range in &mut ranges {
            if range.start() >= pos {
                range.start = range.start.map(|start| start + diff);
            } else if range.start() + range.length() > pos {
                range.length = range.length.map(|length| length + diff);
            }
        }
    };

    if let Some(bold) = bold.take() {
        let diff = insert_at(bold, "**");
        update_pos(italic.as_mut(), bold, diff);
        update_pos(spoiler.as_mut(), bold, diff);
        update_pos(strikethrough.as_mut(), bold, diff);
        update_pos(monospace.as_mut(), bold, diff);
        update_ranges(bold, diff);
    }

    if let Some(italic) = italic.take() {
        let diff = insert_at(italic, "*");
        update_pos(spoiler.as_mut(), italic, diff);
        update_pos(strikethrough.as_mut(), italic, diff);
        update_pos(monospace.as_mut(), italic, diff);
        update_ranges(italic, diff);
    }

    if let Some(spoiler) = spoiler.take() {
        let diff = insert_at(spoiler, "||");
        update_pos(strikethrough.as_mut(), spoiler, diff);
        update_pos(monospace.as_mut(), spoiler, diff);
        update_ranges(spoiler, diff);
    }

    if let Some(strikethrough) = strikethrough.take() {
        let diff = insert_at(strikethrough, "~~");
        update_pos(monospace.as_mut(), strikethrough, diff);
        update_ranges(strikethrough, diff);
    }

    if let Some(monospace) = monospace.take() {
        let diff = insert_at(monospace, "`");
        update_ranges(monospace, diff);
    }

    ranges.retain(|range| range.length() != 0);

    (output, ranges)
}

pub fn body_ranges_to_signal_spans(
    body: Option<String>,
    body_ranges: Vec<BodyRange>,
) -> Option<Vec<SignalSpan<'static>>> {
    let body = body.filter(|body| !body.is_empty())?;

    let mut flags = vec![0u8; body.chars().count()];

    for range in body_ranges {
        let start = range.start() as usize;
        let end = start + range.length() as usize;

        let Some(style) = range
            .associated_value
            .as_ref()
            .and_then(|value| match value {
                AssociatedValue::MentionAci(_) => Some(0),
                AssociatedValue::Style(style @ 1..=5) => Some(*style),
                AssociatedValue::Style(_) => None,
            })
        else {
            continue;
        };

        for flag in &mut flags[start..end] {
            *flag |= 1 << style;
        }
    }

    let mut spans: Vec<SignalSpan<'static>> = vec![];
    let mut last_flag = flags[0];
    let in_progress_span = &mut String::new();
    let mut spoiler_tag = None;
    let mut next_spoiler_tag = 0;

    for (flag, c) in flags.iter().zip(body.chars()) {
        if last_flag != *flag {
            if last_flag & SPOILER != 0 {
                spoiler_tag = Some(next_spoiler_tag);

                if flag & SPOILER == 0 {
                    next_spoiler_tag += 1;
                }
            }

            spans.push(
                SignalSpan::new(take(in_progress_span))
                    .flags(last_flag)
                    .spoiler_tag_maybe(spoiler_tag.take()),
            );
            last_flag = *flag;
        }

        in_progress_span.push(c);
    }

    if last_flag & SPOILER != 0 {
        spoiler_tag = Some(next_spoiler_tag);
    }

    spans.push(
        SignalSpan::new(take(in_progress_span))
            .flags(last_flag)
            .spoiler_tag_maybe(spoiler_tag),
    );

    Some(spans)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::widget::text::span::{BOLD, ITALIC, MONOSPACE, SPOILER, STRIKETHROUGH};
    use std::borrow::Cow;

    #[test]
    fn test_happy() {
        let (output, ranges) = markdown_to_body_ranges(
            r"testing ***rich text*** ~~(fancy \\\*\* escaping)~~ ||this is a `monospace spoiler||` ||*italic* **bold** ~~strikethrough~~ spoiler||",
        );

        assert_eq!(
            output,
            r"testing rich text (fancy \** escaping) this is a monospace spoiler italic bold strikethrough spoiler"
        );

        assert_eq!(
            ranges,
            [
                BodyRange {
                    start: Some(8),
                    length: Some(9),
                    associated_value: Some(AssociatedValue::Style(Style::Bold as i32))
                },
                BodyRange {
                    start: Some(8),
                    length: Some(9),
                    associated_value: Some(AssociatedValue::Style(Style::Italic as i32))
                },
                BodyRange {
                    start: Some(18),
                    length: Some(20),
                    associated_value: Some(AssociatedValue::Style(Style::Strikethrough as i32))
                },
                BodyRange {
                    start: Some(39),
                    length: Some(27),
                    associated_value: Some(AssociatedValue::Style(Style::Spoiler as i32))
                },
                BodyRange {
                    start: Some(49),
                    length: Some(17),
                    associated_value: Some(AssociatedValue::Style(Style::Monospace as i32))
                },
                BodyRange {
                    start: Some(67),
                    length: Some(6),
                    associated_value: Some(AssociatedValue::Style(Style::Italic as i32))
                },
                BodyRange {
                    start: Some(74),
                    length: Some(4),
                    associated_value: Some(AssociatedValue::Style(Style::Bold as i32))
                },
                BodyRange {
                    start: Some(79),
                    length: Some(13),
                    associated_value: Some(AssociatedValue::Style(Style::Strikethrough as i32))
                },
                BodyRange {
                    start: Some(67),
                    length: Some(33),
                    associated_value: Some(AssociatedValue::Style(Style::Spoiler as i32))
                },
            ]
        );

        let output = body_ranges_to_signal_spans(Some(output), ranges).unwrap();

        assert_eq!(
            output,
            [
                SignalSpan {
                    text: Cow::Borrowed(r"testing "),
                    flags: 0,
                    link: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r"rich text"),
                    flags: BOLD | ITALIC,
                    link: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r" "),
                    flags: 0,
                    link: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r"(fancy \** escaping)"),
                    flags: STRIKETHROUGH,
                    link: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r" "),
                    flags: 0,
                    link: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r"this is a "),
                    flags: SPOILER,
                    link: None,
                    spoiler_tag: Some(0)
                },
                SignalSpan {
                    text: Cow::Borrowed(r"monospace spoiler"),
                    flags: SPOILER | MONOSPACE,
                    link: None,
                    spoiler_tag: Some(0)
                },
                SignalSpan {
                    text: Cow::Borrowed(r" "),
                    flags: 0,
                    link: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r"italic"),
                    flags: SPOILER | ITALIC,
                    link: None,
                    spoiler_tag: Some(1)
                },
                SignalSpan {
                    text: Cow::Borrowed(r" "),
                    flags: SPOILER,
                    link: None,
                    spoiler_tag: Some(1)
                },
                SignalSpan {
                    text: Cow::Borrowed(r"bold"),
                    flags: SPOILER | BOLD,
                    link: None,
                    spoiler_tag: Some(1)
                },
                SignalSpan {
                    text: Cow::Borrowed(r" "),
                    flags: SPOILER,
                    link: None,
                    spoiler_tag: Some(1)
                },
                SignalSpan {
                    text: Cow::Borrowed(r"strikethrough"),
                    flags: SPOILER | STRIKETHROUGH,
                    link: None,
                    spoiler_tag: Some(1)
                },
                SignalSpan {
                    text: Cow::Borrowed(r" spoiler"),
                    flags: SPOILER,
                    link: None,
                    spoiler_tag: Some(1)
                },
            ]
        )
    }
}
