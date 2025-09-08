use crate::{
    message::Chat,
    widget::{
        SignalSpan,
        text::span::{MENTION, SPOILER},
    },
};
use presage::{
    libsignal_service::prelude::Uuid,
    proto::{
        BodyRange,
        body_range::{AssociatedValue, Style},
    },
    store::Thread,
};
use std::{cell::RefCell, collections::HashMap, mem::take};

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
            '*' if iter.next_if_eq(&'*').is_some() => {
                // we are starting or ending a bold range

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
            '|' if iter.next_if_eq(&'|').is_some() => {
                // we are starting or ending a spoiler range

                if let Some(spoiler) = spoiler.take() {
                    push_range(count, spoiler, Style::Spoiler);
                } else {
                    spoiler = Some(count);
                }
            }
            '~' if iter.next_if_eq(&'~').is_some() => {
                // we are starting or ending a strikethrough range

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
        if let Some(last) = last
            && *last > pos
        {
            *last += diff;
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
    cache: &RefCell<HashMap<Thread, Chat>>,
) -> Option<Vec<SignalSpan<'static>>> {
    let body = body.filter(|body| !body.is_empty())?;

    let mut flags = vec![0u8; body.chars().count()];
    let mut mentions = HashMap::new();
    let mut spoiler_tags = HashMap::new();
    let mut next_spoiler_tag = 0;

    for range in body_ranges {
        let start = range.start() as usize;
        let end = start + range.length() as usize;

        let mut mention: Option<Uuid> = None;

        let Some(style) = range
            .associated_value
            .as_ref()
            .and_then(|value| match value {
                AssociatedValue::MentionAci(aci) => {
                    mention = aci.parse().ok();

                    Some(0)
                }
                AssociatedValue::Style(style @ 1..=5) => Some(*style),
                AssociatedValue::Style(_) => None,
            })
        else {
            continue;
        };

        if style == 3 {
            spoiler_tags.insert(start, next_spoiler_tag);
            next_spoiler_tag += 1;
        }

        if let Some(uuid) = mention
            && let Some(name) = cache
                .borrow()
                .get(&Thread::Contact(uuid))?
                .contact()
                .map(|contact| contact.name.clone())
        {
            mentions.insert(start, (uuid, name));
        }

        for flag in &mut flags[start..end] {
            *flag |= 1 << style;
        }
    }

    let mut spans = vec![];
    let mut last_flag = flags[0];
    let in_progress_span = &mut String::new();
    let mut spoiler_tag = None;
    let mut mention = None;

    for ((index, flag), c) in flags.iter().enumerate().zip(body.chars()) {
        if last_flag != *flag {
            spans.push(
                SignalSpan::new(take(in_progress_span))
                    .flags(last_flag)
                    .set_mention_maybe(mention.take())
                    .spoiler_tag_maybe(spoiler_tag),
            );

            last_flag = *flag;
        }

        if let Some(tag) = spoiler_tags.get(&index) {
            spoiler_tag = Some(*tag);
        } else if flag & SPOILER == 0 {
            spoiler_tag = None;
        }

        if let Some((uuid, name)) = mentions.get(&index) {
            in_progress_span.push_str(name);
            mention = Some(*uuid);
        } else if flag & MENTION == 0 {
            in_progress_span.push(c);
        }
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
            r"testing ***rich text*** ~~(fancy \\\*\* escaping)~~ ||this is a `monospace spoiler||`||*italic* **bold** ~~strikethrough~~ spoiler||",
        );

        assert_eq!(
            output,
            r"testing rich text (fancy \** escaping) this is a monospace spoileritalic bold strikethrough spoiler"
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
                    start: Some(66),
                    length: Some(6),
                    associated_value: Some(AssociatedValue::Style(Style::Italic as i32))
                },
                BodyRange {
                    start: Some(73),
                    length: Some(4),
                    associated_value: Some(AssociatedValue::Style(Style::Bold as i32))
                },
                BodyRange {
                    start: Some(78),
                    length: Some(13),
                    associated_value: Some(AssociatedValue::Style(Style::Strikethrough as i32))
                },
                BodyRange {
                    start: Some(66),
                    length: Some(33),
                    associated_value: Some(AssociatedValue::Style(Style::Spoiler as i32))
                },
            ]
        );

        let output =
            body_ranges_to_signal_spans(Some(output), ranges, &RefCell::new(HashMap::new()))
                .unwrap();

        assert_eq!(
            output,
            [
                SignalSpan {
                    text: Cow::Borrowed(r"testing "),
                    flags: 0,
                    link: None,
                    mention: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r"rich text"),
                    flags: BOLD | ITALIC,
                    link: None,
                    mention: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r" "),
                    flags: 0,
                    link: None,
                    mention: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r"(fancy \** escaping)"),
                    flags: STRIKETHROUGH,
                    link: None,
                    mention: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r" "),
                    flags: 0,
                    link: None,
                    mention: None,
                    spoiler_tag: None
                },
                SignalSpan {
                    text: Cow::Borrowed(r"this is a "),
                    flags: SPOILER,
                    link: None,
                    mention: None,
                    spoiler_tag: Some(0)
                },
                SignalSpan {
                    text: Cow::Borrowed(r"monospace spoiler"),
                    flags: SPOILER | MONOSPACE,
                    link: None,
                    mention: None,
                    spoiler_tag: Some(0)
                },
                SignalSpan {
                    text: Cow::Borrowed(r"italic"),
                    flags: SPOILER | ITALIC,
                    link: None,
                    mention: None,
                    spoiler_tag: Some(1)
                },
                SignalSpan {
                    text: Cow::Borrowed(r" "),
                    flags: SPOILER,
                    link: None,
                    mention: None,
                    spoiler_tag: Some(1)
                },
                SignalSpan {
                    text: Cow::Borrowed(r"bold"),
                    flags: SPOILER | BOLD,
                    link: None,
                    mention: None,
                    spoiler_tag: Some(1)
                },
                SignalSpan {
                    text: Cow::Borrowed(r" "),
                    flags: SPOILER,
                    link: None,
                    mention: None,
                    spoiler_tag: Some(1)
                },
                SignalSpan {
                    text: Cow::Borrowed(r"strikethrough"),
                    flags: SPOILER | STRIKETHROUGH,
                    link: None,
                    mention: None,
                    spoiler_tag: Some(1)
                },
                SignalSpan {
                    text: Cow::Borrowed(r" spoiler"),
                    flags: SPOILER,
                    link: None,
                    mention: None,
                    spoiler_tag: Some(1)
                },
            ]
        );
    }
}
