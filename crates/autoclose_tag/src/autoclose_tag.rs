use std::{ops::Range, sync::Arc};

use multi_buffer::{Anchor, MultiBufferSnapshot};
use rope::Point;
use text::Selection;

pub fn compute_autoclose_tag(
    snapshot: &MultiBufferSnapshot,
    text_insert_loc: Point,
    text: &str,
) -> Option<Prediction> {
    if !text.ends_with(">") || text.ends_with("/>") {
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

    for ch in iter_range_chars_rev {
        if count_until_open == 1 && ch == '/' {
            return None;
        }
        count_until_open += 1;
        if ch == '>' {
            return None;
        } else if ch == '<' {
            open_tag_found = true;
            break;
        }
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
    let mut range_tag_name_offset_start = 0;
    let mut range_tag_name_offset_end = 1;

    let mut traversed = 0;

    while let Some(ch) = iter_range_chars.next() {
        let is_whitespace = ch.is_ascii_whitespace();
        let are_computing_start = range_tag_name_offset_start == 0;
        let are_computing_end = range_tag_name_offset_end == 1;

        if !is_whitespace && are_computing_start {
            range_tag_name_offset_start += traversed;
        } else if are_computing_end && (is_whitespace || ch == '>') {
            range_tag_name_offset_end += traversed;
            break;
        }
        traversed += 1;
    }

    let mut text_index_tag_name_start = 0;
    let mut text_index_tag_name_end = 0;

    let mut buf_offset_tag_name_start = dbg!(offset_start) + dbg!(range_tag_name_offset_start);
    let mut buf_offset_tag_name_end = dbg!(offset_start) + dbg!(range_tag_name_offset_end);

    if range_tag_name_offset_start > count_buf_range_chars {
        buf_offset_tag_name_start = 0;
        buf_offset_tag_name_end = 0;
        text_index_tag_name_start = range_tag_name_offset_start - count_buf_range_chars;
        text_index_tag_name_end = range_tag_name_offset_end - count_buf_range_chars;
    } else if range_tag_name_offset_end > count_buf_range_chars {
        buf_offset_tag_name_end = offset_start + count_buf_range_chars;
        text_index_tag_name_start = 0;
        text_index_tag_name_end = range_tag_name_offset_end - count_buf_range_chars;
    }

    let point_tag_name_start = snapshot.offset_to_point(dbg!(buf_offset_tag_name_start));
    let point_tag_name_end = snapshot.offset_to_point(dbg!(buf_offset_tag_name_end));

    let completion_start_offset = offset_insert + text.len();
    let completion_start_point = snapshot.offset_to_point(completion_start_offset);

    return Some(Prediction {
        tag_name_range_buf: point_tag_name_start..point_tag_name_end,
        tag_name_range_txt: text_index_tag_name_start..text_index_tag_name_end,
        text_start_point: completion_start_point,
    });
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
    return [
        (range_before.clone(), text),
        (range_after.clone(), "</".into()),
        (range_after.clone(), dbg!(tag_name_portion_buf)),
        (range_after.clone(), dbg!(tag_name_portion_txt)),
        (range_after.clone(), ">".into()),
    ];
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
