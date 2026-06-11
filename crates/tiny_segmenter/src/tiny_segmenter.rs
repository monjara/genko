//! TinySegmenter 0.2 compatible Japanese tokenizer.
//!
//! Ported from TinySegmenter.js:
//! <http://chasen.org/~taku/software/TinySegmenter/>
//! (c) 2008 Taku Kudo. New BSD licence.

use std::ops::Range;

mod tables;

const BIAS: i32 = -332;

pub fn segment(input: &str) -> Vec<&str> {
    segment_ranges(input)
        .into_iter()
        .map(|range| &input[range])
        .collect()
}

pub fn segment_ranges(input: &str) -> Vec<Range<usize>> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut segment_text = vec!["B3".to_string(), "B2".to_string(), "B1".to_string()];
    let mut character_types = vec!["O", "O", "O"];
    let mut byte_offsets = vec![0, 0, 0];

    for (byte_offset, character) in input.char_indices() {
        segment_text.push(character.to_string());
        character_types.push(character_type(character));
        byte_offsets.push(byte_offset);
    }

    segment_text.extend(["E1".to_string(), "E2".to_string(), "E3".to_string()]);
    character_types.extend(["O", "O", "O"]);
    byte_offsets.extend([input.len(), input.len(), input.len()]);

    let mut ranges = Vec::new();
    let mut word_start = byte_offsets[3];
    let mut previous_boundary_1 = "U";
    let mut previous_boundary_2 = "U";
    let mut previous_boundary_3 = "U";

    for index in 4..segment_text.len() - 3 {
        let word_1 = segment_text[index - 3].as_str();
        let word_2 = segment_text[index - 2].as_str();
        let word_3 = segment_text[index - 1].as_str();
        let word_4 = segment_text[index].as_str();
        let word_5 = segment_text[index + 1].as_str();
        let word_6 = segment_text[index + 2].as_str();
        let class_1 = character_types[index - 3];
        let class_2 = character_types[index - 2];
        let class_3 = character_types[index - 1];
        let class_4 = character_types[index];
        let class_5 = character_types[index + 1];
        let class_6 = character_types[index + 2];

        let mut score = BIAS;
        score += tables::up1(previous_boundary_1);
        score += tables::up2(previous_boundary_2);
        score += tables::up3(previous_boundary_3);
        score += tables::bp1(&key2(previous_boundary_1, previous_boundary_2));
        score += tables::bp2(&key2(previous_boundary_2, previous_boundary_3));
        score += tables::uw1(word_1);
        score += tables::uw2(word_2);
        score += tables::uw3(word_3);
        score += tables::uw4(word_4);
        score += tables::uw5(word_5);
        score += tables::uw6(word_6);
        score += tables::bw1(&key2(word_2, word_3));
        score += tables::bw2(&key2(word_3, word_4));
        score += tables::bw3(&key2(word_4, word_5));
        score += tables::tw1(&key3(word_1, word_2, word_3));
        score += tables::tw2(&key3(word_2, word_3, word_4));
        score += tables::tw3(&key3(word_3, word_4, word_5));
        score += tables::tw4(&key3(word_4, word_5, word_6));
        score += tables::uc1(class_1);
        score += tables::uc2(class_2);
        score += tables::uc3(class_3);
        score += tables::uc4(class_4);
        score += tables::uc5(class_5);
        score += tables::uc6(class_6);
        score += tables::bc1(&key2(class_2, class_3));
        score += tables::bc2(&key2(class_3, class_4));
        score += tables::bc3(&key2(class_4, class_5));
        score += tables::tc1(&key3(class_1, class_2, class_3));
        score += tables::tc2(&key3(class_2, class_3, class_4));
        score += tables::tc3(&key3(class_3, class_4, class_5));
        score += tables::tc4(&key3(class_4, class_5, class_6));
        score += tables::uq1(&key2(previous_boundary_1, class_1));
        score += tables::uq2(&key2(previous_boundary_2, class_2));
        score += tables::uq3(&key2(previous_boundary_3, class_3));
        score += tables::bq1(&key3(previous_boundary_2, class_2, class_3));
        score += tables::bq2(&key3(previous_boundary_2, class_3, class_4));
        score += tables::bq3(&key3(previous_boundary_3, class_2, class_3));
        score += tables::bq4(&key3(previous_boundary_3, class_3, class_4));
        score += tables::tq1(&key4(previous_boundary_2, class_1, class_2, class_3));
        score += tables::tq2(&key4(previous_boundary_2, class_2, class_3, class_4));
        score += tables::tq3(&key4(previous_boundary_3, class_1, class_2, class_3));
        score += tables::tq4(&key4(previous_boundary_3, class_2, class_3, class_4));

        let boundary = if score > 0 {
            ranges.push(word_start..byte_offsets[index]);
            word_start = byte_offsets[index];
            "B"
        } else {
            "O"
        };

        previous_boundary_1 = previous_boundary_2;
        previous_boundary_2 = previous_boundary_3;
        previous_boundary_3 = boundary;
    }

    ranges.push(word_start..input.len());
    ranges
}

fn character_type(character: char) -> &'static str {
    match character {
        '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '百' | '千'
        | '万' | '億' | '兆' => "M",
        '一'..='龠' | '々' | '〆' | 'ヵ' | 'ヶ' => "H",
        'ぁ'..='ん' => "I",
        'ァ'..='ヴ' | 'ー' | 'ｱ'..='ﾝ' | 'ﾞ' => "K",
        'a'..='z' | 'A'..='Z' | 'ａ'..='ｚ' | 'Ａ'..='Ｚ' => "A",
        '0'..='9' | '０'..='９' => "N",
        _ => "O",
    }
}

fn key2(first: &str, second: &str) -> String {
    let mut key = String::with_capacity(first.len() + second.len());
    key.push_str(first);
    key.push_str(second);
    key
}

fn key3(first: &str, second: &str, third: &str) -> String {
    let mut key = String::with_capacity(first.len() + second.len() + third.len());
    key.push_str(first);
    key.push_str(second);
    key.push_str(third);
    key
}

fn key4(first: &str, second: &str, third: &str, fourth: &str) -> String {
    let mut key = String::with_capacity(first.len() + second.len() + third.len() + fourth.len());
    key.push_str(first);
    key.push_str(second);
    key.push_str(third);
    key.push_str(fourth);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_japanese_sentence() {
        assert_eq!(
            segment("私の名前は中野です"),
            vec!["私", "の", "名前", "は", "中野", "です"]
        );
    }

    #[test]
    fn returns_byte_ranges() {
        let input = "私の名前は中野です";
        let ranges = segment_ranges(input);
        let segments = ranges
            .into_iter()
            .map(|range| &input[range])
            .collect::<Vec<_>>();
        assert_eq!(segments, vec!["私", "の", "名前", "は", "中野", "です"]);
    }
}
