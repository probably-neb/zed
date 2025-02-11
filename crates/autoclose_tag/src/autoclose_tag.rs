use std::{ops::Range, sync::Arc};

use multi_buffer::{Anchor, MultiBufferSnapshot};
use rope::Point;
use text::Selection;

pub fn compute_autoclose_tag(
    snapshot: &MultiBufferSnapshot,
    text_insert_loc: Point,
    text: &str,
) -> Option<Prediction> {
    if !text.ends_with(">") || text.ends_with("/>") || text.starts_with("</") {
        return None;
    }
    let mut count_until_open = 1;
    let mut open_tag_found = false;
    let mut iter_range_chars_rev = text
        .chars()
        .rev()
        .chain(snapshot.reversed_chars_at(text_insert_loc));
    // dbg!(&iter_range_chars_rev.by_ref().collect::<String>());

    assert_eq!(iter_range_chars_rev.next(), Some('>'));

    let mut prev_was_slash = false;
    let mut prev_was_exclamation = false;

    let mut count_braces: u32 = 0;
    let mut count_quotes_double: u32 = 0;
    let mut count_quotes_single: u32 = 0;

    // TODO: also filter out `{<,>}` in quoted strings

    for ch in iter_range_chars_rev {
        if count_until_open == 1 && ch == '/' {
            return None;
        }
        count_until_open += 1;

        // ignore contents of jsx expresssions
        // example: `<Component attr={x < 1 && y > 2}>` should still complete just fine by ignoring `{<,>}` in expression
        if count_quotes_double == 0 && count_quotes_single == 0 {
            if ch == '}' {
                count_braces += 1;
            } else if ch == '{' {
                count_braces = count_braces.saturating_sub(1);
            }
        }

        if count_braces == 0 && count_quotes_double == 0 && count_quotes_single == 0 {
            if ch == '>' {
                return None;
            } else if ch == '<' {
                if prev_was_slash || prev_was_exclamation {
                    return None;
                }
                open_tag_found = true;
                break;
            }
        } else {
        }
        prev_was_exclamation = ch == '!';
        prev_was_slash = ch == '/';
    }
    if !open_tag_found {
        return None;
    }

    count_until_open -= text.len();

    let offset_insert = snapshot.point_to_offset(text_insert_loc);
    let offset_start = offset_insert - count_until_open;
    let point_start = snapshot.offset_to_point(offset_start);
    let point_end = text_insert_loc;

    let count_buf_range_chars = snapshot
        .text_for_range(point_start..point_end)
        .flat_map(|c| c.chars())
        .count();

    let mut iter_range_chars = snapshot
        .text_for_range(point_start..point_end)
        .flat_map(|c| c.chars())
        .chain(text.chars());
    assert_eq!(iter_range_chars.next(), Some('<'));
    let mut range_tag_name_offset_start = 1;
    let mut range_tag_name_offset_end = 1;

    let mut traversed = 0;

    let mut found_start = false;

    while let Some(ch) = iter_range_chars.next() {
        let is_whitespace = ch.is_ascii_whitespace();

        if !is_whitespace && !found_start {
            range_tag_name_offset_start += traversed;
            found_start = true;
        } else if found_start && (is_whitespace || ch == '>') {
            range_tag_name_offset_end += traversed;
            break;
        }
        traversed += 1;
    }

    dbg!(
        &snapshot
            .text_for_range(point_start..point_end)
            .flat_map(|c| c.chars())
            .chain(text.chars())
            .collect::<String>()[range_tag_name_offset_start..range_tag_name_offset_end]
    );

    let mut text_tag_name_index_start = 0;
    let mut text_tag_name_index_end = 0;

    let mut buf_tag_name_offset_start = offset_start + range_tag_name_offset_start;
    let mut buf_tag_name_offset_end = offset_start + range_tag_name_offset_end;

    if range_tag_name_offset_start > count_buf_range_chars {
        buf_tag_name_offset_start = 0;
        buf_tag_name_offset_end = 0;
        text_tag_name_index_start = range_tag_name_offset_start - count_buf_range_chars;
        text_tag_name_index_end = range_tag_name_offset_end - count_buf_range_chars;
    } else if range_tag_name_offset_end > count_buf_range_chars {
        buf_tag_name_offset_end = offset_start + count_buf_range_chars;
        text_tag_name_index_start = 0;
        text_tag_name_index_end = range_tag_name_offset_end - count_buf_range_chars;
    }

    let point_tag_name_start = snapshot.offset_to_point(buf_tag_name_offset_start);
    let point_tag_name_end = snapshot.offset_to_point(buf_tag_name_offset_end);

    let completion_start_offset = offset_insert;
    let completion_start_point = snapshot.offset_to_point(completion_start_offset);

    // check if tag is already closed
    // FIXME: add setting for max count of characters to check for closing tag
    const MAX_CHARS_TO_CHECK: usize = 1024;
    {
        let mut iter_trailing_chars = snapshot.chars_at(text_insert_loc);

        let mut iter_tag_name_chars_buf = snapshot
            .text_for_range(point_tag_name_start..point_tag_name_end)
            .flat_map(|s| s.chars());

        assert!(text.len() > 0);
        let mut iter_tag_name_chars_txt =
            text[text_tag_name_index_start..text_tag_name_index_end].chars();

        let mut next_tag_name_char_buf = iter_tag_name_chars_buf.next();
        let mut next_tag_name_char_txt = iter_tag_name_chars_txt.next();

        let mut trailing_state = TrailingState::Searching;

        let mut chars_checked = 0;

        while let Some(c) = iter_trailing_chars.next() {
            if chars_checked > MAX_CHARS_TO_CHECK {
                break;
            }
            chars_checked += 1;

            match trailing_state {
                TrailingState::Searching => {
                    if c == '<' {
                        trailing_state = TrailingState::CloseStartMaybe;
                    }
                }
                TrailingState::CloseStartMaybe => {
                    if c == '/' {
                        trailing_state = TrailingState::AwaitingTagName;
                    } else {
                        trailing_state = TrailingState::Searching;
                    }
                }
                TrailingState::AwaitingTagName if c.is_whitespace() => {}
                TrailingState::AwaitingTagName | TrailingState::CloseTagNamePortionBuf
                    if next_tag_name_char_buf.is_some() =>
                {
                    match (next_tag_name_char_buf, c) {
                        (Some(c_txt), c_trailing) if c_txt == c_trailing => {
                            trailing_state = TrailingState::CloseTagNamePortionBuf;
                            next_tag_name_char_buf = iter_tag_name_chars_buf.next();
                        }
                        (None, _) => unreachable!(),
                        _ => {
                            trailing_state = TrailingState::Searching;
                            continue;
                        }
                    }
                    if next_tag_name_char_buf.is_none() {
                        if next_tag_name_char_txt.is_some() {
                            trailing_state = TrailingState::CloseTagNamePortionText;
                        } else {
                            trailing_state = TrailingState::AwaitingClosingEnd;
                        }
                    }
                }
                TrailingState::CloseTagNamePortionBuf => unreachable!(),
                TrailingState::AwaitingTagName | TrailingState::CloseTagNamePortionText
                    if next_tag_name_char_txt.is_some() =>
                {
                    match (next_tag_name_char_txt, c) {
                        (Some(c_txt), c_trailing) if c_txt == c_trailing => {
                            trailing_state = TrailingState::CloseTagNamePortionText;
                            next_tag_name_char_txt = iter_tag_name_chars_txt.next();
                        }
                        (None, _) => unreachable!(),
                        _ => {
                            trailing_state = TrailingState::Searching;
                        }
                    }
                    if next_tag_name_char_txt.is_none() {
                        trailing_state = TrailingState::AwaitingClosingEnd;
                    }
                }
                TrailingState::CloseTagNamePortionText => unreachable!(),
                TrailingState::AwaitingTagName => unreachable!(),
                TrailingState::AwaitingClosingEnd if c.is_whitespace() => {
                    continue;
                }
                _ if c != '>' => {
                    trailing_state = TrailingState::Searching;
                }
                _ if c == '>' => {
                    // we matched the entire closing tag, implying the tag is already closed, and we
                    // should not suggest closing it twice
                    return None;
                }
                TrailingState::AwaitingClosingEnd => unreachable!(),
            }
        }
    }

    return Some(Prediction {
        tag_name_range_buf: point_tag_name_start..point_tag_name_end,
        tag_name_range_txt: text_tag_name_index_start..text_tag_name_index_end,
        text_start_point: completion_start_point,
    });
}

#[derive(PartialEq, Eq)]
enum TrailingState {
    /// Haven't entered a closing tag yet
    Searching,
    /// Found a `<` but not a `/`
    CloseStartMaybe,
    /// Found a `</`, haven't started checking tag name yet
    AwaitingTagName,
    /// Found a `</` and have started checking tag name against the end of the contents of the buffer
    /// (e.g. user input was `v>` where contents of buffer before the cursor where `<di` and we are checking for `di`)
    CloseTagNamePortionBuf,
    /// Found a `</` and have started checking tag name against the end of `text`
    /// (e.g. user input was `v>` where contents of buffer before the cursor where `<di` and we are checking for `v`)
    CloseTagNamePortionText,
    /// Awaiting `>`
    AwaitingClosingEnd,
}

#[derive(Clone, Debug)]
pub struct Prediction {
    pub tag_name_range_buf: Range<Point>,
    pub tag_name_range_txt: Range<usize>,
    text_start_point: Point,
}

pub fn edits_for(
    prediction: &Prediction,
    snapshot: &MultiBufferSnapshot,
    text: Arc<str>,
) -> [(Range<Anchor>, Arc<str>); 5] {
    let tag_name_portion_buf: Arc<str> = snapshot
        .text_for_range(prediction.tag_name_range_buf.clone())
        .flat_map(|s| s.chars())
        .collect::<String>()
        .into();
    let tag_name_portion_txt: Arc<str> = text[prediction.tag_name_range_txt.clone()].into();

    let text_insert_loc_before = snapshot.anchor_before(prediction.text_start_point);
    let text_insert_loc_after = snapshot.anchor_after(prediction.text_start_point);

    let range_before = text_insert_loc_before..text_insert_loc_before;
    let range_after = text_insert_loc_after..text_insert_loc_after;

    // PERF: 1 String::with_capacity that is filled for autoclose instead of so many individual allocations
    //       then return 2 edits instead of 5
    return dbg!([
        (range_before.clone(), text),
        (range_after.clone(), "</".into()),
        (range_after.clone(), dbg!(tag_name_portion_buf)),
        (range_after.clone(), dbg!(tag_name_portion_txt)),
        (range_after.clone(), ">".into()),
    ]);
}

pub fn selection_for(
    prediction: &Prediction,
    snapshot: &MultiBufferSnapshot,
    text_insert_loc: Point,
    text: &str,
    selection: &Selection<Point>,
) -> (Selection<Anchor>, usize) {
    // let offset_current = snapshot.point_to_offset(text_insert_loc);
    // let offset_final = offset_current + text.len();
    // let point_final = snapshot.offset_to_point(offset_final);
    // let anchor_final = snapshot.anchor_after(point_final);
    let anchor_final = snapshot.anchor_before(text_insert_loc);
    return (selection.map(|_| anchor_final), text.len());
}

// Hey just listened to one of my favorite tech podcasts while eating lunch
// and the guys on it were discussing Zed. Podcast is Tomorrow FM with Dax
// (creator of SST, @thdxr on X)  and Adam (creator of statmuse @adamdotdev on X),
// both part of the terminal.shop thing with ThePrimeagen.  Both are long time
// Neovim users and Dax does all work remotely and was discussing Zed as an option
// because of it's "future" AI integrations (doesn't know that he's correct it is what we're working on)
