// Copyright 2013 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use keyboard_types::{Key, Modifiers, NamedKey};
use script::test::DOMString;
use script::test::textinput::{
    ClipboardProvider, Direction, Lines, Selection, SelectionDirection, TextInput, TextPoint,
    UTF8Bytes, UTF16CodeUnits,
};

pub struct DummyClipboardContext {
    content: String,
}

impl DummyClipboardContext {
    pub fn new(s: &str) -> DummyClipboardContext {
        DummyClipboardContext {
            content: s.to_owned(),
        }
    }
}

impl ClipboardProvider for DummyClipboardContext {
    fn get_text(&mut self) -> Result<String, String> {
        Ok(self.content.clone())
    }
    fn set_text(&mut self, s: String) {
        self.content = s;
    }
}

fn text_input(lines: Lines, s: &str) -> TextInput<DummyClipboardContext> {
    TextInput::new(
        lines,
        DOMString::from(s),
        DummyClipboardContext::new(""),
        None,
        None,
        SelectionDirection::None,
    )
}

#[test]
fn test_set_content_ignores_max_length() {
    let mut textinput = TextInput::new(
        Lines::Single,
        DOMString::from(""),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits::one()),
        None,
        SelectionDirection::None,
    );

    textinput.set_content(DOMString::from("mozilla rocks"));
    assert_eq!(textinput.get_content(), DOMString::from("mozilla rocks"));
}

#[test]
fn test_textinput_when_inserting_multiple_lines_over_a_selection_respects_max_length() {
    let mut textinput = TextInput::new(
        Lines::Multiple,
        DOMString::from("hello\nworld"),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits(17)),
        None,
        SelectionDirection::None,
    );

    textinput.adjust_horizontal(UTF8Bytes::one(), Direction::Forward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Forward, Selection::Selected);
    textinput.adjust_vertical(1, Selection::Selected);

    // Selection is now "hello\n
    //                    ------
    //                   world"
    //                   ----

    textinput.insert_string("cruel\nterrible\nbad".to_string());

    assert_eq!(textinput.get_content(), "hcruel\nterrible\nd");
}

#[test]
fn test_textinput_when_inserting_multiple_lines_still_respects_max_length() {
    let mut textinput = TextInput::new(
        Lines::Multiple,
        DOMString::from("hello\nworld"),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits(17)),
        None,
        SelectionDirection::None,
    );

    textinput.adjust_vertical(1, Selection::NotSelected);
    textinput.insert_string("cruel\nterrible".to_string());

    assert_eq!(textinput.get_content(), "hello\ncruel\nworld");
}

#[test]
fn test_textinput_when_content_is_already_longer_than_max_length_and_theres_no_selection_dont_insert_anything()
 {
    let mut textinput = TextInput::new(
        Lines::Single,
        DOMString::from("abc"),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits::one()),
        None,
        SelectionDirection::None,
    );

    textinput.insert_char('a');

    assert_eq!(textinput.get_content(), "abc");
}

#[test]
fn test_multi_line_textinput_with_maxlength_doesnt_allow_appending_characters_when_input_spans_lines()
 {
    let mut textinput = TextInput::new(
        Lines::Multiple,
        DOMString::from("abc\nd"),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits(5)),
        None,
        SelectionDirection::None,
    );

    textinput.insert_char('a');

    assert_eq!(textinput.get_content(), "abc\nd");
}

#[test]
fn test_single_line_textinput_with_max_length_doesnt_allow_appending_characters_when_replacing_a_selection()
 {
    let mut textinput = TextInput::new(
        Lines::Single,
        DOMString::from("abcde"),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits(5)),
        None,
        SelectionDirection::None,
    );

    textinput.adjust_horizontal(UTF8Bytes::one(), Direction::Forward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Forward, Selection::Selected);

    // Selection is now "abcde"
    //                    ---

    textinput.replace_selection(DOMString::from("too long"));

    assert_eq!(textinput.get_content(), "atooe");
}

#[test]
fn test_single_line_textinput_with_max_length_allows_deletion_when_replacing_a_selection() {
    let mut textinput = TextInput::new(
        Lines::Single,
        DOMString::from("abcde"),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits(1)),
        None,
        SelectionDirection::None,
    );

    textinput.adjust_horizontal(UTF8Bytes::one(), Direction::Forward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Forward, Selection::Selected);

    // Selection is now "abcde"
    //                    --

    textinput.replace_selection(DOMString::from("only deletion should be applied"));

    assert_eq!(textinput.get_content(), "ade");
}

#[test]
fn test_single_line_textinput_with_max_length_multibyte() {
    let mut textinput = TextInput::new(
        Lines::Single,
        DOMString::from(""),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits(2)),
        None,
        SelectionDirection::None,
    );

    textinput.insert_char('á');
    assert_eq!(textinput.get_content(), "á");
    textinput.insert_char('é');
    assert_eq!(textinput.get_content(), "áé");
    textinput.insert_char('i');
    assert_eq!(textinput.get_content(), "áé");
}

#[test]
fn test_single_line_textinput_with_max_length_multi_code_unit() {
    let mut textinput = TextInput::new(
        Lines::Single,
        DOMString::from(""),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits(3)),
        None,
        SelectionDirection::None,
    );

    textinput.insert_char('\u{10437}');
    assert_eq!(textinput.get_content(), "\u{10437}");
    textinput.insert_char('\u{10437}');
    assert_eq!(textinput.get_content(), "\u{10437}");
    textinput.insert_char('x');
    assert_eq!(textinput.get_content(), "\u{10437}x");
    textinput.insert_char('x');
    assert_eq!(textinput.get_content(), "\u{10437}x");
}

#[test]
fn test_single_line_textinput_with_max_length_inside_char() {
    let mut textinput = TextInput::new(
        Lines::Single,
        DOMString::from("\u{10437}"),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits::one()),
        None,
        SelectionDirection::None,
    );

    textinput.insert_char('x');
    assert_eq!(textinput.get_content(), "\u{10437}");
}

#[test]
fn test_single_line_textinput_with_max_length_doesnt_allow_appending_characters_after_max_length_is_reached()
 {
    let mut textinput = TextInput::new(
        Lines::Single,
        DOMString::from("a"),
        DummyClipboardContext::new(""),
        Some(UTF16CodeUnits::one()),
        None,
        SelectionDirection::None,
    );

    textinput.insert_char('b');
    assert_eq!(textinput.get_content(), "a");
}

#[test]
fn test_textinput_delete_char() {
    let mut textinput = text_input(Lines::Single, "abcdefg");
    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Forward, Selection::NotSelected);
    textinput.delete_char(Direction::Backward);
    assert_eq!(textinput.get_content(), "acdefg");

    textinput.delete_char(Direction::Forward);
    assert_eq!(textinput.get_content(), "adefg");

    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Forward, Selection::Selected);
    textinput.delete_char(Direction::Forward);
    assert_eq!(textinput.get_content(), "afg");

    let mut textinput = text_input(Lines::Single, "a🌠b");
    // Same as "Right" key
    textinput.adjust_horizontal_by_one(Direction::Forward, Selection::NotSelected);
    textinput.delete_char(Direction::Forward);
    // Not splitting surrogate pairs.
    assert_eq!(textinput.get_content(), "ab");

    let mut textinput = text_input(Lines::Single, "abcdefg");
    textinput.set_selection_range(2, 2, SelectionDirection::None);
    textinput.delete_char(Direction::Backward);
    assert_eq!(textinput.get_content(), "acdefg");
}

#[test]
fn test_textinput_insert_char() {
    let mut textinput = text_input(Lines::Single, "abcdefg");
    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Forward, Selection::NotSelected);
    textinput.insert_char('a');
    assert_eq!(textinput.get_content(), "abacdefg");

    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Forward, Selection::Selected);
    textinput.insert_char('b');
    assert_eq!(textinput.get_content(), "ababefg");

    let mut textinput = text_input(Lines::Single, "a🌠c");
    // Same as "Right" key
    textinput.adjust_horizontal_by_one(Direction::Forward, Selection::NotSelected);
    textinput.adjust_horizontal_by_one(Direction::Forward, Selection::NotSelected);
    textinput.insert_char('b');
    // Not splitting surrogate pairs.
    assert_eq!(textinput.get_content(), "a🌠bc");
}

#[test]
fn test_textinput_get_sorted_selection() {
    let mut textinput = text_input(Lines::Single, "abcdefg");
    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Forward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Forward, Selection::Selected);
    let (start, end) = textinput.sorted_selection_bounds();
    assert_eq!(start.index, UTF8Bytes(2));
    assert_eq!(end.index, UTF8Bytes(4));

    textinput.clear_selection();

    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Backward, Selection::Selected);
    let (start, end) = textinput.sorted_selection_bounds();
    assert_eq!(start.index, UTF8Bytes(2));
    assert_eq!(end.index, UTF8Bytes(4));
}

#[test]
fn test_textinput_replace_selection() {
    let mut textinput = text_input(Lines::Single, "abcdefg");
    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Forward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Forward, Selection::Selected);

    textinput.replace_selection(DOMString::from("xyz"));
    assert_eq!(textinput.get_content(), "abxyzefg");
}

#[test]
fn test_textinput_replace_selection_multibyte_char() {
    let mut textinput = text_input(Lines::Single, "é");
    textinput.adjust_horizontal_by_one(Direction::Forward, Selection::Selected);

    textinput.replace_selection(DOMString::from("e"));
    assert_eq!(textinput.get_content(), "e");
}

#[test]
fn test_textinput_current_line_length() {
    let mut textinput = text_input(Lines::Multiple, "abc\nde\nf");
    assert_eq!(textinput.current_line_length(), UTF8Bytes(3));

    textinput.adjust_vertical(1, Selection::NotSelected);
    assert_eq!(textinput.current_line_length(), UTF8Bytes(2));

    textinput.adjust_vertical(1, Selection::NotSelected);
    assert_eq!(textinput.current_line_length(), UTF8Bytes::one());
}

#[test]
fn test_textinput_adjust_vertical() {
    let mut textinput = text_input(Lines::Multiple, "abc\nde\nf");
    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Forward, Selection::NotSelected);
    textinput.adjust_vertical(1, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 1);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(2));

    textinput.adjust_vertical(-1, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(2));

    textinput.adjust_vertical(2, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 2);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(1));

    textinput.adjust_vertical(-1, Selection::Selected);
    assert_eq!(textinput.edit_point().line, 1);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(1));
}

#[test]
fn test_textinput_adjust_vertical_multibyte() {
    let mut textinput = text_input(Lines::Multiple, "áé\nae");

    textinput.adjust_horizontal_by_one(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(2));

    textinput.adjust_vertical(1, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 1);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(1));
}

#[test]
fn test_textinput_adjust_horizontal() {
    let mut textinput = text_input(Lines::Multiple, "abc\nde\nf");
    textinput.adjust_horizontal(UTF8Bytes(4), Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 1);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());

    textinput.adjust_horizontal(UTF8Bytes::one(), Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 1);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(1));

    textinput.adjust_horizontal(UTF8Bytes(2), Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 2);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());

    textinput.adjust_horizontal(
        UTF8Bytes::one(),
        Direction::Backward,
        Selection::NotSelected,
    );
    assert_eq!(textinput.edit_point().line, 1);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(2));
}

#[test]
fn test_textinput_adjust_horizontal_by_word() {
    // Test basic case of movement word by word based on UAX#29 rules
    let mut textinput = text_input(Lines::Single, "abc def");
    textinput.adjust_horizontal_by_word(Direction::Forward, Selection::NotSelected);
    textinput.adjust_horizontal_by_word(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(7));
    textinput.adjust_horizontal_by_word(Direction::Backward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(4));
    textinput.adjust_horizontal_by_word(Direction::Backward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());

    // Test new line case of movement word by word based on UAX#29 rules
    let mut textinput_2 = text_input(Lines::Multiple, "abc\ndef");
    textinput_2.adjust_horizontal_by_word(Direction::Forward, Selection::NotSelected);
    textinput_2.adjust_horizontal_by_word(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput_2.edit_point().line, 1);
    assert_eq!(textinput_2.edit_point().index, UTF8Bytes(3));
    textinput_2.adjust_horizontal_by_word(Direction::Backward, Selection::NotSelected);
    assert_eq!(textinput_2.edit_point().line, 1);
    assert_eq!(textinput_2.edit_point().index, UTF8Bytes::zero());
    textinput_2.adjust_horizontal_by_word(Direction::Backward, Selection::NotSelected);
    assert_eq!(textinput_2.edit_point().line, 0);
    assert_eq!(textinput_2.edit_point().index, UTF8Bytes::zero());

    // Test non-standard sized characters case of movement word by word based on UAX#29 rules
    let mut textinput_3 = text_input(Lines::Single, "áéc d🌠bc");
    textinput_3.adjust_horizontal_by_word(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput_3.edit_point().line, 0);
    assert_eq!(textinput_3.edit_point().index, UTF8Bytes(5));
    textinput_3.adjust_horizontal_by_word(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput_3.edit_point().line, 0);
    assert_eq!(textinput_3.edit_point().index, UTF8Bytes(7));
    textinput_3.adjust_horizontal_by_word(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput_3.edit_point().line, 0);
    assert_eq!(textinput_3.edit_point().index, UTF8Bytes(13));
    textinput_3.adjust_horizontal_by_word(Direction::Backward, Selection::NotSelected);
    assert_eq!(textinput_3.edit_point().line, 0);
    assert_eq!(textinput_3.edit_point().index, UTF8Bytes(11));
    textinput_3.adjust_horizontal_by_word(Direction::Backward, Selection::NotSelected);
    assert_eq!(textinput_3.edit_point().line, 0);
    assert_eq!(textinput_3.edit_point().index, UTF8Bytes(6));
}

#[test]
fn test_textinput_adjust_horizontal_to_line_end() {
    // Test standard case of movement to end based on UAX#29 rules
    let mut textinput = text_input(Lines::Single, "abc def");
    textinput.adjust_horizontal_to_line_end(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(7));

    // Test new line case of movement to end based on UAX#29 rules
    let mut textinput_2 = text_input(Lines::Multiple, "abc\ndef");
    textinput_2.adjust_horizontal_to_line_end(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput_2.edit_point().line, 0);
    assert_eq!(textinput_2.edit_point().index, UTF8Bytes(3));
    textinput_2.adjust_horizontal_to_line_end(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput_2.edit_point().line, 0);
    assert_eq!(textinput_2.edit_point().index, UTF8Bytes(3));
    textinput_2.adjust_horizontal_to_line_end(Direction::Backward, Selection::NotSelected);
    assert_eq!(textinput_2.edit_point().line, 0);
    assert_eq!(textinput_2.edit_point().index, UTF8Bytes::zero());

    // Test non-standard sized characters case of movement to end based on UAX#29 rules
    let mut textinput_3 = text_input(Lines::Single, "áéc d🌠bc");
    textinput_3.adjust_horizontal_to_line_end(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput_3.edit_point().line, 0);
    assert_eq!(textinput_3.edit_point().index, UTF8Bytes(13));
    textinput_3.adjust_horizontal_to_line_end(Direction::Backward, Selection::NotSelected);
    assert_eq!(textinput_3.edit_point().line, 0);
    assert_eq!(textinput_3.edit_point().index, UTF8Bytes::zero());
}

#[test]
fn test_navigation_keyboard_shortcuts() {
    let mut textinput = text_input(Lines::Multiple, "hello áéc");

    // Test that CMD + Right moves to the end of the current line.
    textinput.handle_keydown_aux(Key::Named(NamedKey::ArrowRight), Modifiers::META, true);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(11));
    // Test that CMD + Right moves to the beginning of the current line.
    textinput.handle_keydown_aux(Key::Named(NamedKey::ArrowLeft), Modifiers::META, true);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());
    // Test that CTRL + ALT + E moves to the end of the current line also.
    textinput.handle_keydown_aux(
        Key::Character("e".to_owned()),
        Modifiers::CONTROL | Modifiers::ALT,
        true,
    );
    assert_eq!(textinput.edit_point().index, UTF8Bytes(11));
    // Test that CTRL + ALT + A moves to the beginning of the current line also.
    textinput.handle_keydown_aux(
        Key::Character("a".to_owned()),
        Modifiers::CONTROL | Modifiers::ALT,
        true,
    );
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());

    // Test that ALT + Right moves to the end of the word.
    textinput.handle_keydown_aux(Key::Named(NamedKey::ArrowRight), Modifiers::ALT, true);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(5));
    // Test that CTRL + ALT + F moves to the end of the word also.
    textinput.handle_keydown_aux(
        Key::Character("f".to_owned()),
        Modifiers::CONTROL | Modifiers::ALT,
        true,
    );
    assert_eq!(textinput.edit_point().index, UTF8Bytes(11));
    // Test that ALT + Left moves to the end of the word.
    textinput.handle_keydown_aux(Key::Named(NamedKey::ArrowLeft), Modifiers::ALT, true);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(6));
    // Test that CTRL + ALT + B moves to the end of the word also.
    textinput.handle_keydown_aux(
        Key::Character("b".to_owned()),
        Modifiers::CONTROL | Modifiers::ALT,
        true,
    );
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());
}

#[test]
fn test_textinput_handle_return() {
    let mut single_line_textinput = text_input(Lines::Single, "abcdef");
    single_line_textinput.adjust_horizontal(
        UTF8Bytes(3),
        Direction::Forward,
        Selection::NotSelected,
    );
    single_line_textinput.handle_return();
    assert_eq!(single_line_textinput.get_content(), "abcdef");

    let mut multi_line_textinput = text_input(Lines::Multiple, "abcdef");
    multi_line_textinput.adjust_horizontal(
        UTF8Bytes(3),
        Direction::Forward,
        Selection::NotSelected,
    );
    multi_line_textinput.handle_return();
    assert_eq!(multi_line_textinput.get_content(), "abc\ndef");
}

#[test]
fn test_textinput_select_all() {
    let mut textinput = text_input(Lines::Multiple, "abc\nde\nf");
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());

    textinput.select_all();
    assert_eq!(textinput.edit_point().line, 2);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(1));
}

#[test]
fn test_textinput_get_content() {
    let single_line_textinput = text_input(Lines::Single, "abcdefg");
    assert_eq!(single_line_textinput.get_content(), "abcdefg");

    let multi_line_textinput = text_input(Lines::Multiple, "abc\nde\nf");
    assert_eq!(multi_line_textinput.get_content(), "abc\nde\nf");
}

#[test]
fn test_textinput_set_content() {
    let mut textinput = text_input(Lines::Multiple, "abc\nde\nf");
    assert_eq!(textinput.get_content(), "abc\nde\nf");

    textinput.set_content(DOMString::from("abc\nf"));
    assert_eq!(textinput.get_content(), "abc\nf");

    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());

    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Forward, Selection::Selected);
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(3));
    textinput.set_content(DOMString::from("de"));
    assert_eq!(textinput.get_content(), "de");
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(2));
}

#[test]
fn test_clipboard_paste() {
    #[cfg(target_os = "macos")]
    const MODIFIERS: Modifiers = Modifiers::META;
    #[cfg(not(target_os = "macos"))]
    const MODIFIERS: Modifiers = Modifiers::CONTROL;

    let mut textinput = TextInput::new(
        Lines::Single,
        DOMString::from("defg"),
        DummyClipboardContext::new("abc"),
        None,
        None,
        SelectionDirection::None,
    );
    assert_eq!(textinput.get_content(), "defg");
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());
    textinput.handle_keydown_aux(Key::Character("v".to_owned()), MODIFIERS, false);
    assert_eq!(textinput.get_content(), "abcdefg");
}

#[test]
fn test_textinput_cursor_position_correct_after_clearing_selection() {
    let mut textinput = text_input(Lines::Single, "abcdef");

    // Single line - Forward
    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Forward, Selection::Selected);
    textinput.adjust_horizontal(UTF8Bytes::one(), Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(3));

    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Backward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Forward, Selection::Selected);
    textinput.adjust_horizontal_by_one(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(3));

    // Single line - Backward
    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Backward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Forward, Selection::Selected);
    textinput.adjust_horizontal(
        UTF8Bytes::one(),
        Direction::Backward,
        Selection::NotSelected,
    );
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());

    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Backward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(3), Direction::Forward, Selection::Selected);
    textinput.adjust_horizontal_by_one(Direction::Backward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());

    let mut textinput = text_input(Lines::Multiple, "abc\nde\nf");

    // Multiline - Forward
    textinput.adjust_horizontal(UTF8Bytes(4), Direction::Forward, Selection::Selected);
    textinput.adjust_horizontal(UTF8Bytes::one(), Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());
    assert_eq!(textinput.edit_point().line, 1);

    textinput.adjust_horizontal(UTF8Bytes(4), Direction::Backward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(4), Direction::Forward, Selection::Selected);
    textinput.adjust_horizontal_by_one(Direction::Forward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());
    assert_eq!(textinput.edit_point().line, 1);

    // Multiline - Backward
    textinput.adjust_horizontal(UTF8Bytes(4), Direction::Backward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(4), Direction::Forward, Selection::Selected);
    textinput.adjust_horizontal(
        UTF8Bytes::one(),
        Direction::Backward,
        Selection::NotSelected,
    );
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());
    assert_eq!(textinput.edit_point().line, 0);

    textinput.adjust_horizontal(UTF8Bytes(4), Direction::Backward, Selection::NotSelected);
    textinput.adjust_horizontal(UTF8Bytes(4), Direction::Forward, Selection::Selected);
    textinput.adjust_horizontal_by_one(Direction::Backward, Selection::NotSelected);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());
    assert_eq!(textinput.edit_point().line, 0);
}

#[test]
fn test_textinput_set_selection_with_direction() {
    let mut textinput = text_input(Lines::Single, "abcdef");
    textinput.set_selection_range(2, 6, SelectionDirection::Forward);
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(6));
    assert_eq!(textinput.selection_direction(), SelectionDirection::Forward);

    assert!(textinput.selection_origin().is_some());
    assert_eq!(textinput.selection_origin().unwrap().line, 0);
    assert_eq!(textinput.selection_origin().unwrap().index, UTF8Bytes(2));

    textinput.set_selection_range(2, 6, SelectionDirection::Backward);
    assert_eq!(textinput.edit_point().line, 0);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(2));
    assert_eq!(
        textinput.selection_direction(),
        SelectionDirection::Backward
    );

    assert!(textinput.selection_origin().is_some());
    assert_eq!(textinput.selection_origin().unwrap().line, 0);
    assert_eq!(textinput.selection_origin().unwrap().index, UTF8Bytes(6));

    textinput = text_input(Lines::Multiple, "\n\n");
    textinput.set_selection_range(0, 1, SelectionDirection::Forward);
    assert_eq!(textinput.edit_point().line, 1);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());
    assert_eq!(textinput.selection_direction(), SelectionDirection::Forward);

    assert!(textinput.selection_origin().is_some());
    assert_eq!(textinput.selection_origin().unwrap().line, 0);
    assert_eq!(
        textinput.selection_origin().unwrap().index,
        UTF8Bytes::zero()
    );

    textinput = text_input(Lines::Multiple, "\n");
    textinput.set_selection_range(0, 1, SelectionDirection::Forward);
    assert_eq!(textinput.edit_point().line, 1);
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());
    assert_eq!(textinput.selection_direction(), SelectionDirection::Forward);

    assert!(textinput.selection_origin().is_some());
    assert_eq!(textinput.selection_origin().unwrap().line, 0);
    assert_eq!(
        textinput.selection_origin().unwrap().index,
        UTF8Bytes::zero()
    );
}

#[test]
fn test_textinput_unicode_handling() {
    let mut textinput = text_input(Lines::Single, "éèùµ$£");
    assert_eq!(textinput.edit_point().index, UTF8Bytes::zero());
    textinput.set_edit_point_index(1);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(2));
    textinput.set_edit_point_index(4);
    assert_eq!(textinput.edit_point().index, UTF8Bytes(8));
}

#[test]
fn test_selection_bounds() {
    let mut textinput = text_input(Lines::Single, "abcdef");

    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes::zero()
        },
        textinput.selection_origin_or_edit_point()
    );
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes::zero()
        },
        textinput.selection_start()
    );
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes::zero()
        },
        textinput.selection_end()
    );

    textinput.set_selection_range(2, 5, SelectionDirection::Forward);
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes(2)
        },
        textinput.selection_origin_or_edit_point()
    );
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes(2)
        },
        textinput.selection_start()
    );
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes(5)
        },
        textinput.selection_end()
    );
    assert_eq!(UTF8Bytes(2), textinput.selection_start_offset());
    assert_eq!(UTF8Bytes(5), textinput.selection_end_offset());

    textinput.set_selection_range(3, 6, SelectionDirection::Backward);
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes(6)
        },
        textinput.selection_origin_or_edit_point()
    );
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes(3)
        },
        textinput.selection_start()
    );
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes(6)
        },
        textinput.selection_end()
    );
    assert_eq!(UTF8Bytes(3), textinput.selection_start_offset());
    assert_eq!(UTF8Bytes(6), textinput.selection_end_offset());

    textinput = text_input(Lines::Multiple, "\n\n");
    textinput.set_selection_range(0, 1, SelectionDirection::Forward);
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes::zero()
        },
        textinput.selection_origin_or_edit_point()
    );
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes::zero()
        },
        textinput.selection_start()
    );
    assert_eq!(
        TextPoint {
            line: 1,
            index: UTF8Bytes::zero()
        },
        textinput.selection_end()
    );
}

#[test]
fn test_select_all() {
    let mut textinput = text_input(Lines::Single, "abc");
    textinput.set_selection_range(2, 3, SelectionDirection::Backward);
    textinput.select_all();
    assert_eq!(textinput.selection_direction(), SelectionDirection::Forward);
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes::zero()
        },
        textinput.selection_start()
    );
    assert_eq!(
        TextPoint {
            line: 0,
            index: UTF8Bytes(3)
        },
        textinput.selection_end()
    );
}

#[test]
fn test_backspace_in_textarea_at_beginning_of_line() {
    let mut textinput = text_input(Lines::Multiple, "first line\n");
    textinput.handle_keydown_aux(Key::Named(NamedKey::ArrowDown), Modifiers::empty(), false);
    textinput.handle_keydown_aux(Key::Named(NamedKey::Backspace), Modifiers::empty(), false);
    assert_eq!(textinput.get_content(), DOMString::from("first line"));
}
