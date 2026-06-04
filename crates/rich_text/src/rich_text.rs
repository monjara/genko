use std::{
    cmp::min,
    fs::File,
    io::{self, Write},
    ops::Range,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use xmlwriter::{Options as XmlOptions, XmlWriter};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RichTextDocumentMeta {
    marks: Vec<RichTextMark>,
}

impl RichTextDocumentMeta {
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    pub fn add_mark(&mut self, range: Range<usize>, kind: RichTextKind) {
        if range.start > range.end {
            return;
        }

        if range.is_empty() && !matches!(kind, RichTextKind::PageBreak { .. }) {
            return;
        }

        self.marks.push(RichTextMark { range, kind });
        self.marks
            .sort_by_key(|mark| (mark.range.start, mark.range.end));
    }

    pub fn toggle_mark(&mut self, range: Range<usize>, kind: RichTextKind) {
        if range.start > range.end {
            return;
        }

        if let RichTextKind::PageBreak { column } = kind {
            self.toggle_page_break_column(column, range.start);
            return;
        }

        if range.is_empty() {
            return;
        }

        if self.range_is_fully_marked(&range, &kind) {
            self.remove_mark_kind_from_range(&range, &kind);
        } else {
            self.remove_mark_kind_from_range(&range, &kind);
            self.add_mark(range, kind);
        }
    }

    pub fn set_ruby(&mut self, range: Range<usize>, text: String) {
        if range.is_empty() {
            return;
        }

        self.marks.retain(|mark| {
            !matches!(mark.kind, RichTextKind::Ruby { .. }) || !ranges_overlap(&mark.range, &range)
        });
        if !text.is_empty() {
            self.add_mark(range, RichTextKind::Ruby { text });
        }
    }

    pub fn marks(&self) -> &[RichTextMark] {
        self.marks.as_slice()
    }

    pub fn clear(&mut self) {
        self.marks.clear();
    }

    pub fn set_page_break_column(&mut self, column: usize, offset: usize) {
        self.marks
            .retain(|mark| !is_page_break_column(&mark.kind, column));
        self.add_mark(offset..offset, RichTextKind::PageBreak { column });
    }

    pub fn toggle_page_break_column(&mut self, column: usize, offset: usize) {
        let original_len = self.marks.len();
        self.marks
            .retain(|mark| !is_page_break_column(&mark.kind, column));
        if self.marks.len() == original_len {
            self.add_mark(offset..offset, RichTextKind::PageBreak { column });
        }
    }

    pub fn remove_page_break_column(&mut self, column: usize) {
        self.marks
            .retain(|mark| !is_page_break_column(&mark.kind, column));
    }

    pub fn move_page_break_column(&mut self, from_column: usize, to_column: usize, offset: usize) {
        let mut moved = false;
        self.marks.retain_mut(|mark| {
            if let RichTextKind::PageBreak { column } = &mut mark.kind {
                if *column == from_column {
                    if moved {
                        return false;
                    }
                    *column = to_column;
                    mark.range = offset..offset;
                    moved = true;
                    return true;
                }
                return *column != to_column;
            }
            true
        });
        if !moved {
            self.add_mark(
                offset..offset,
                RichTextKind::PageBreak { column: to_column },
            );
        }
        self.marks
            .sort_by_key(|mark| (mark.range.start, mark.range.end));
    }

    pub fn apply_text_edit(&mut self, start: usize, removed_len: usize, inserted_len: usize) {
        let removed_range = start..start.saturating_add(removed_len);
        self.marks = self
            .marks
            .drain(..)
            .filter_map(|mut mark| {
                if matches!(mark.kind, RichTextKind::PageBreak { .. }) {
                    return Some(mark);
                }
                mark.range = transform_range(mark.range, &removed_range, inserted_len)?;
                Some(mark)
            })
            .collect();
        self.marks
            .sort_by_key(|mark| (mark.range.start, mark.range.end));
    }

    fn range_is_fully_marked(&self, range: &Range<usize>, kind: &RichTextKind) -> bool {
        let mut covered_until = range.start;
        let mut marks = self
            .marks
            .iter()
            .filter(|mark| {
                mark_kind_matches(&mark.kind, kind) && ranges_overlap(&mark.range, range)
            })
            .collect::<Vec<_>>();
        marks.sort_by_key(|mark| mark.range.start);

        for mark in marks {
            if mark.range.start > covered_until {
                return false;
            }
            covered_until = covered_until.max(mark.range.end.min(range.end));
            if covered_until >= range.end {
                return true;
            }
        }

        false
    }

    fn remove_mark_kind_from_range(&mut self, range: &Range<usize>, kind: &RichTextKind) {
        let mut replacement = Vec::with_capacity(self.marks.len());
        for mark in self.marks.drain(..) {
            if !mark_kind_matches(&mark.kind, kind) || !ranges_overlap(&mark.range, range) {
                replacement.push(mark);
                continue;
            }

            if mark.range.start < range.start {
                replacement.push(RichTextMark {
                    range: mark.range.start..range.start,
                    kind: mark.kind.clone(),
                });
            }
            if range.end < mark.range.end {
                replacement.push(RichTextMark {
                    range: range.end..mark.range.end,
                    kind: mark.kind,
                });
            }
        }
        self.marks = replacement;
        self.marks
            .sort_by_key(|mark| (mark.range.start, mark.range.end));
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RichTextEdit {
    start: usize,
    removed_text: String,
    inserted_text: String,
    affects_rich_text: bool,
}

impl RichTextEdit {
    pub fn new(
        start: usize,
        removed_text: String,
        inserted_text: String,
        affects_rich_text: bool,
    ) -> Self {
        Self {
            start,
            removed_text,
            inserted_text,
            affects_rich_text,
        }
    }
}

pub fn sync_meta_after_text_change(
    meta: &mut RichTextDocumentMeta,
    previous_text: &str,
    current_text: &str,
    edits: &[RichTextEdit],
) {
    if previous_text == current_text {
        return;
    }

    if can_apply_edit_batch(previous_text, edits, current_text) {
        for edit in edits {
            if edit.affects_rich_text {
                meta.apply_text_edit(
                    edit.start,
                    edit.removed_text.len(),
                    edit.inserted_text.len(),
                );
            }
        }
        return;
    }

    if let Some((range, replacement)) = single_change(previous_text, current_text) {
        meta.apply_text_edit(range.start, range.end - range.start, replacement.len());
        return;
    }

    meta.clear();
}

fn can_apply_edit_batch(text: &str, edits: &[RichTextEdit], expected_text: &str) -> bool {
    let mut scratch = text.to_string();
    for edit in edits {
        let range = edit.start..edit.start.saturating_add(edit.removed_text.len());
        if range.end > scratch.len() {
            return false;
        }
        if !scratch.is_char_boundary(range.start) || !scratch.is_char_boundary(range.end) {
            return false;
        }
        if scratch.get(range.clone()) != Some(edit.removed_text.as_str()) {
            return false;
        }
        scratch.replace_range(range, edit.inserted_text.as_str());
    }
    scratch == expected_text
}

pub fn single_change(old_text: &str, new_text: &str) -> Option<(Range<usize>, String)> {
    if old_text == new_text {
        return None;
    }

    let mut prefix = 0;
    let max_prefix = min(old_text.len(), new_text.len());
    while prefix < max_prefix && old_text.as_bytes()[prefix] == new_text.as_bytes()[prefix] {
        prefix += 1;
    }

    let mut old_suffix = old_text.len();
    let mut new_suffix = new_text.len();
    while old_suffix > prefix
        && new_suffix > prefix
        && old_text.as_bytes()[old_suffix - 1] == new_text.as_bytes()[new_suffix - 1]
    {
        old_suffix -= 1;
        new_suffix -= 1;
    }

    while prefix > 0 && (!old_text.is_char_boundary(prefix) || !new_text.is_char_boundary(prefix)) {
        prefix -= 1;
    }

    while old_suffix < old_text.len() && !old_text.is_char_boundary(old_suffix) {
        old_suffix += 1;
    }

    while new_suffix < new_text.len() && !new_text.is_char_boundary(new_suffix) {
        new_suffix += 1;
    }

    Some((prefix..old_suffix, new_text[prefix..new_suffix].to_string()))
}

fn transform_range(
    range: Range<usize>,
    removed_range: &Range<usize>,
    inserted_len: usize,
) -> Option<Range<usize>> {
    if range.end <= removed_range.start {
        return Some(range);
    }

    if range.start >= removed_range.end {
        let start = shift_after_edit(range.start, removed_range, inserted_len);
        let end = shift_after_edit(range.end, removed_range, inserted_len);
        return Some(start..end);
    }

    let start = if range.start < removed_range.start {
        range.start
    } else {
        removed_range.start + inserted_len
    };
    let end = if range.end > removed_range.end {
        shift_after_edit(range.end, removed_range, inserted_len)
    } else {
        removed_range.start
    };

    if start >= end { None } else { Some(start..end) }
}

fn ranges_overlap(first: &Range<usize>, second: &Range<usize>) -> bool {
    first.start < second.end && second.start < first.end
}

fn mark_kind_matches(left: &RichTextKind, right: &RichTextKind) -> bool {
    match (left, right) {
        (RichTextKind::Bold, RichTextKind::Bold) => true,
        (RichTextKind::Emphasis, RichTextKind::Emphasis) => true,
        (RichTextKind::Ruby { .. }, RichTextKind::Ruby { .. }) => true,
        (RichTextKind::Heading { level: left }, RichTextKind::Heading { level: right }) => {
            left == right
        }
        (RichTextKind::PageBreak { .. }, RichTextKind::PageBreak { .. }) => true,
        _ => false,
    }
}

fn is_page_break_column(kind: &RichTextKind, column: usize) -> bool {
    matches!(kind, RichTextKind::PageBreak { column: existing_column } if *existing_column == column)
}

fn shift_after_edit(offset: usize, removed_range: &Range<usize>, inserted_len: usize) -> usize {
    if inserted_len >= removed_range.end - removed_range.start {
        offset + (inserted_len - (removed_range.end - removed_range.start))
    } else {
        offset.saturating_sub((removed_range.end - removed_range.start) - inserted_len)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RichTextMark {
    range: Range<usize>,
    kind: RichTextKind,
}

impl RichTextMark {
    pub fn range(&self) -> &Range<usize> {
        &self.range
    }

    pub fn kind(&self) -> &RichTextKind {
        &self.kind
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RichTextKind {
    Bold,
    Emphasis,
    Ruby {
        text: String,
    },
    Heading {
        level: u8,
    },
    PageBreak {
        #[serde(default)]
        column: usize,
    },
}

pub fn meta_path_for_text_path(text_path: &Path) -> PathBuf {
    let file_stem = text_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("untitled");
    text_path.with_file_name(format!("{file_stem}.meta.json"))
}

pub fn load_meta_for_text_path(text_path: &Path) -> io::Result<RichTextDocumentMeta> {
    let meta_path = meta_path_for_text_path(text_path);
    match std::fs::read_to_string(meta_path) {
        Ok(text) => serde_json::from_str(text.as_str())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Ok(RichTextDocumentMeta::default())
        }
        Err(error) => Err(error),
    }
}

pub fn save_meta_for_text_path(text_path: &Path, meta: &RichTextDocumentMeta) -> io::Result<()> {
    let meta_path = meta_path_for_text_path(text_path);
    let text = serde_json::to_string_pretty(meta)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    std::fs::write(meta_path, text)
}

pub fn parse_ruby_markup(text: &str) -> Option<ParsedRuby> {
    let text = text.strip_prefix('｜').unwrap_or(text);
    let open = text.find('《')?;
    let ruby = text.strip_suffix('》')?.get(open + '《'.len_utf8()..)?;
    let base = text.get(..open)?;

    if base.is_empty() || ruby.is_empty() {
        return None;
    }

    Some(ParsedRuby {
        base: base.to_string(),
        ruby: ruby.to_string(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedRuby {
    pub base: String,
    pub ruby: String,
}

pub fn export_epub(
    path: &Path,
    title: &str,
    plain_text: &str,
    meta: &RichTextDocumentMeta,
) -> io::Result<()> {
    let mut writer = StoredZipWriter::new(File::create(path)?);
    writer.add_file("mimetype", b"application/epub+zip")?;
    writer.add_file("META-INF/container.xml", container_xml().as_bytes())?;
    writer.add_file("OEBPS/content.opf", package_document(title).as_bytes())?;
    writer.add_file("OEBPS/nav.xhtml", nav_document(title).as_bytes())?;
    writer.add_file(
        "OEBPS/text.xhtml",
        text_document(title, plain_text, meta).as_bytes(),
    )?;
    writer.finish()
}

pub fn export_word(path: &Path, plain_text: &str, meta: &RichTextDocumentMeta) -> io::Result<()> {
    let mut writer = StoredZipWriter::new(File::create(path)?);
    writer.add_file(
        "word/document.xml",
        word_document(plain_text, meta).as_bytes(),
    )?;
    writer.add_file("word/styles.xml", word_styles().as_bytes())?;
    writer.add_file(
        "word/_rels/document.xml.rels",
        word_document_rels().as_bytes(),
    )?;
    writer.add_file("_rels/.rels", root_relationships().as_bytes())?;
    writer.add_file("[Content_Types].xml", word_content_types().as_bytes())?;
    writer.finish()
}

fn container_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#
}

fn package_document(title: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" unique-identifier="book-id" xmlns="http://www.idpf.org/2007/opf">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="book-id">urn:uuid:00000000-0000-0000-0000-000000000000</dc:identifier>
    <dc:title>{}</dc:title>
    <dc:language>ja</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="text" href="text.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="text"/>
  </spine>
</package>"#,
        escape_xml(title)
    )
}

fn nav_document(title: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" lang="ja" xml:lang="ja">
  <head><title>{}</title></head>
  <body>
    <nav epub:type="toc" xmlns:epub="http://www.idpf.org/2007/ops">
      <h1>{}</h1>
      <ol><li><a href="text.xhtml">本文</a></li></ol>
    </nav>
  </body>
</html>"#,
        escape_xml(title),
        escape_xml(title)
    )
}

fn text_document(title: &str, plain_text: &str, meta: &RichTextDocumentMeta) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" lang="ja" xml:lang="ja">
  <head>
    <title>{}</title>
    <style>
      body {{ writing-mode: vertical-rl; line-height: 1.8; }}
      .emphasis {{ text-emphasis: filled sesame; -webkit-text-emphasis: filled sesame; }}
      .page-break {{ break-before: page; page-break-before: always; }}
    </style>
  </head>
  <body>{}</body>
</html>"#,
        escape_xml(title),
        render_body(plain_text, meta)
    )
}

fn render_body(text: &str, meta: &RichTextDocumentMeta) -> String {
    let mut output = String::new();
    let mut offset = 0;
    let mut marks = meta.marks.clone();
    marks.sort_by_key(|mark| (mark.range.start, mark.range.end));

    for mark in marks {
        if mark.range.start > text.len() || mark.range.end > text.len() {
            continue;
        }
        if !text.is_char_boundary(mark.range.start) || !text.is_char_boundary(mark.range.end) {
            continue;
        }
        if mark.range.start < offset && !matches!(mark.kind, RichTextKind::PageBreak { .. }) {
            continue;
        }

        if mark.range.start >= offset {
            output.push_str(&escape_xml(&text[offset..mark.range.start]));
        }
        match mark.kind {
            RichTextKind::Bold => {
                output.push_str("<strong>");
                output.push_str(&escape_xml(&text[mark.range.clone()]));
                output.push_str("</strong>");
            }
            RichTextKind::Emphasis => {
                output.push_str(r#"<span class="emphasis">"#);
                output.push_str(&escape_xml(&text[mark.range.clone()]));
                output.push_str("</span>");
            }
            RichTextKind::Ruby { text: ruby_text } => {
                output.push_str("<ruby>");
                output.push_str(&escape_xml(&text[mark.range.clone()]));
                output.push_str("<rt>");
                output.push_str(&escape_xml(ruby_text.as_str()));
                output.push_str("</rt></ruby>");
            }
            RichTextKind::Heading { level } => {
                let level = level.clamp(1, 6);
                output.push_str(&format!("<h{level}>"));
                output.push_str(&escape_xml(&text[mark.range.clone()]));
                output.push_str(&format!("</h{level}>"));
            }
            RichTextKind::PageBreak { .. } => {
                output.push_str(r#"<div class="page-break"></div>"#);
            }
        }
        offset = offset.max(mark.range.end);
    }

    output.push_str(&escape_xml(&text[offset..]));
    output.replace('\n', "<br/>")
}

fn escape_xml(text: &str) -> String {
    text.chars()
        .flat_map(|character| match character {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect::<Vec<_>>(),
            '>' => "&gt;".chars().collect::<Vec<_>>(),
            '"' => "&quot;".chars().collect::<Vec<_>>(),
            '\'' => "&apos;".chars().collect::<Vec<_>>(),
            _ => vec![character],
        })
        .collect()
}

#[derive(Clone, Debug)]
struct WordParagraph {
    heading_level: Option<u8>,
    runs: Vec<WordRun>,
}

#[derive(Clone, Debug, Default)]
struct WordRunStyle {
    bold: bool,
    emphasis: bool,
}

#[derive(Clone, Debug)]
struct WordRun {
    text: String,
    style: WordRunStyle,
}

fn word_document(plain_text: &str, meta: &RichTextDocumentMeta) -> String {
    let paragraphs = word_paragraphs(plain_text, meta);
    let mut xml = XmlWriter::new(XmlOptions::default());
    xml.start_element("w:document");
    xml.write_attribute(
        "xmlns:w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    );
    xml.write_attribute(
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    );
    xml.start_element("w:body");

    for paragraph in paragraphs {
        xml.start_element("w:p");
        if let Some(level) = paragraph.heading_level {
            xml.start_element("w:pPr");
            xml.start_element("w:pStyle");
            xml.write_attribute("w:val", if level <= 1 { "Heading1" } else { "Heading2" });
            xml.end_element();
            xml.end_element();
        }

        for run in paragraph.runs {
            xml.start_element("w:r");
            if run.style.bold || run.style.emphasis {
                xml.start_element("w:rPr");
                if run.style.bold {
                    xml.start_element("w:b");
                    xml.end_element();
                }
                if run.style.emphasis {
                    xml.start_element("w:em");
                    xml.write_attribute("w:val", "dot");
                    xml.end_element();
                }
                xml.end_element();
            }
            xml.start_element("w:t");
            if run.text.starts_with(' ') || run.text.ends_with(' ') {
                xml.write_attribute("xml:space", "preserve");
            }
            xml.write_text(run.text.as_str());
            xml.end_element();
            xml.end_element();
        }
        xml.end_element();
    }

    xml.start_element("w:sectPr");
    xml.start_element("w:pgSz");
    xml.write_attribute("w:w", "16838");
    xml.write_attribute("w:h", "11906");
    xml.write_attribute("w:orient", "landscape");
    xml.end_element();
    xml.start_element("w:pgMar");
    xml.write_attribute("w:top", "1440");
    xml.write_attribute("w:right", "1440");
    xml.write_attribute("w:bottom", "1440");
    xml.write_attribute("w:left", "1440");
    xml.write_attribute("w:header", "708");
    xml.write_attribute("w:footer", "708");
    xml.write_attribute("w:gutter", "0");
    xml.end_element();
    xml.start_element("w:textDirection");
    xml.write_attribute("w:val", "tbRl");
    xml.end_element();
    xml.end_element();
    xml.end_element();
    xml.end_element();
    xml.end_document()
}

fn word_paragraphs(plain_text: &str, meta: &RichTextDocumentMeta) -> Vec<WordParagraph> {
    let mut paragraphs = Vec::new();
    let mut start = 0;

    loop {
        let end = plain_text[start..]
            .find('\n')
            .map(|index| start + index)
            .unwrap_or(plain_text.len());
        let paragraph_text = &plain_text[start..end];
        paragraphs.push(WordParagraph {
            heading_level: word_heading_level(start, end, meta),
            runs: word_runs(start, paragraph_text, meta),
        });

        if end == plain_text.len() {
            break;
        }
        start = end + 1;
    }

    paragraphs
}

fn word_heading_level(start: usize, end: usize, meta: &RichTextDocumentMeta) -> Option<u8> {
    meta.marks()
        .iter()
        .filter(|mark| ranges_overlap(mark.range(), &(start..end.max(start + 1))))
        .find_map(|mark| match mark.kind() {
            RichTextKind::Heading { level } => Some(*level),
            _ => None,
        })
}

fn word_runs(start_offset: usize, text: &str, meta: &RichTextDocumentMeta) -> Vec<WordRun> {
    if text.is_empty() {
        return vec![WordRun {
            text: String::new(),
            style: WordRunStyle::default(),
        }];
    }

    let end_offset = start_offset + text.len();
    let mut boundaries = vec![start_offset, end_offset];
    for mark in meta.marks() {
        if !ranges_overlap(mark.range(), &(start_offset..end_offset)) {
            continue;
        }
        boundaries.push(mark.range().start.max(start_offset).min(end_offset));
        boundaries.push(mark.range().end.max(start_offset).min(end_offset));
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut runs = Vec::new();
    for window in boundaries.windows(2) {
        let local_start = window[0] - start_offset;
        let local_end = window[1] - start_offset;
        if !text.is_char_boundary(local_start) || !text.is_char_boundary(local_end) {
            continue;
        }
        let slice = &text[local_start..local_end];
        if slice.is_empty() {
            continue;
        }

        let mut style = WordRunStyle::default();
        for mark in meta.marks() {
            if !ranges_overlap(mark.range(), &(window[0]..window[1])) {
                continue;
            }
            match mark.kind() {
                RichTextKind::Bold => style.bold = true,
                RichTextKind::Emphasis => style.emphasis = true,
                RichTextKind::Ruby { .. }
                | RichTextKind::Heading { .. }
                | RichTextKind::PageBreak { .. } => {}
            }
        }

        runs.push(WordRun {
            text: slice.to_string(),
            style,
        });
    }

    runs
}

fn word_styles() -> String {
    let mut xml = XmlWriter::new(XmlOptions::default());
    xml.start_element("w:styles");
    xml.write_attribute(
        "xmlns:w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    );
    write_word_style(&mut xml, "Normal", "Normal", None, 22, false);
    write_word_style(&mut xml, "Heading1", "大見出し", Some("Normal"), 36, true);
    write_word_style(&mut xml, "Heading2", "小見出し", Some("Normal"), 28, true);
    xml.end_element();
    xml.end_document()
}

fn write_word_style(
    xml: &mut XmlWriter,
    style_id: &str,
    name: &str,
    based_on: Option<&str>,
    size_half_points: u32,
    bold: bool,
) {
    xml.start_element("w:style");
    xml.write_attribute("w:type", "paragraph");
    xml.write_attribute("w:styleId", style_id);
    xml.start_element("w:name");
    xml.write_attribute("w:val", name);
    xml.end_element();
    if let Some(parent) = based_on {
        xml.start_element("w:basedOn");
        xml.write_attribute("w:val", parent);
        xml.end_element();
    }
    xml.start_element("w:rPr");
    if bold {
        xml.start_element("w:b");
        xml.end_element();
    }
    xml.start_element("w:sz");
    xml.write_attribute("w:val", &size_half_points.to_string());
    xml.end_element();
    xml.start_element("w:rFonts");
    xml.write_attribute("w:eastAsia", "Zen Old Mincho");
    xml.write_attribute("w:ascii", "Zen Old Mincho");
    xml.write_attribute("w:hAnsi", "Zen Old Mincho");
    xml.end_element();
    xml.end_element();
    xml.end_element();
}

fn word_document_rels() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#
}

fn root_relationships() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#
}

fn word_content_types() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#
}

struct StoredZipWriter<W: Write> {
    writer: W,
    offset: u32,
    entries: Vec<StoredZipEntry>,
}

struct StoredZipEntry {
    name: String,
    crc32: u32,
    size: u32,
    local_header_offset: u32,
}

impl<W: Write> StoredZipWriter<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            offset: 0,
            entries: Vec::new(),
        }
    }

    fn add_file(&mut self, name: &str, contents: &[u8]) -> io::Result<()> {
        let name_bytes = name.as_bytes();
        let size = u32::try_from(contents.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "EPUB file is too large"))?;
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "EPUB path is too long"))?;
        let crc32 = crc32fast::hash(contents);
        let local_header_offset = self.offset;

        self.write_u32(0x0403_4b50)?;
        self.write_u16(20)?;
        self.write_u16(0)?;
        self.write_u16(0)?;
        self.write_u16(0)?;
        self.write_u16(0)?;
        self.write_u32(crc32)?;
        self.write_u32(size)?;
        self.write_u32(size)?;
        self.write_u16(name_len)?;
        self.write_u16(0)?;
        self.write_all(name_bytes)?;
        self.write_all(contents)?;

        self.entries.push(StoredZipEntry {
            name: name.to_string(),
            crc32,
            size,
            local_header_offset,
        });
        Ok(())
    }

    fn finish(mut self) -> io::Result<()> {
        let central_directory_offset = self.offset;
        for entry_index in 0..self.entries.len() {
            let (name, crc32, size, local_header_offset) = {
                let entry = &self.entries[entry_index];
                (
                    entry.name.clone(),
                    entry.crc32,
                    entry.size,
                    entry.local_header_offset,
                )
            };
            let name_bytes = name.as_bytes();
            let name_len = u16::try_from(name_bytes.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "EPUB path is too long")
            })?;
            self.write_u32(0x0201_4b50)?;
            self.write_u16(20)?;
            self.write_u16(20)?;
            self.write_u16(0)?;
            self.write_u16(0)?;
            self.write_u16(0)?;
            self.write_u16(0)?;
            self.write_u32(crc32)?;
            self.write_u32(size)?;
            self.write_u32(size)?;
            self.write_u16(name_len)?;
            self.write_u16(0)?;
            self.write_u16(0)?;
            self.write_u16(0)?;
            self.write_u16(0)?;
            self.write_u32(0)?;
            self.write_u32(local_header_offset)?;
            self.write_all(name_bytes)?;
        }
        let central_directory_size = self.offset - central_directory_offset;
        let entry_count = u16::try_from(self.entries.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "too many EPUB entries"))?;

        self.write_u32(0x0605_4b50)?;
        self.write_u16(0)?;
        self.write_u16(0)?;
        self.write_u16(entry_count)?;
        self.write_u16(entry_count)?;
        self.write_u32(central_directory_size)?;
        self.write_u32(central_directory_offset)?;
        self.write_u16(0)?;
        Ok(())
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(bytes)?;
        self.offset = self
            .offset
            .checked_add(u32::try_from(bytes.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "EPUB file is too large")
            })?)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "EPUB file is too large"))?;
        Ok(())
    }

    fn write_u16(&mut self, value: u16) -> io::Result<()> {
        self.write_all(&value.to_le_bytes())
    }

    fn write_u32(&mut self, value: u32) -> io::Result<()> {
        self.write_all(&value.to_le_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_adjacent_meta_path_for_text_file() {
        assert_eq!(
            meta_path_for_text_path(Path::new("/notes/foo.txt")),
            PathBuf::from("/notes/foo.meta.json")
        );
    }

    #[test]
    fn serializes_rich_text_metadata() {
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark(0..6, RichTextKind::Bold);
        meta.add_mark(
            6..12,
            RichTextKind::Ruby {
                text: "よみ".to_string(),
            },
        );

        let text = serde_json::to_string(&meta).expect("serialize rich text metadata");
        let decoded: RichTextDocumentMeta =
            serde_json::from_str(text.as_str()).expect("deserialize rich text metadata");

        assert_eq!(decoded, meta);
    }

    #[test]
    fn parses_ruby_markup() {
        assert_eq!(
            parse_ruby_markup("｜草稿《そうこう》"),
            Some(ParsedRuby {
                base: "草稿".to_string(),
                ruby: "そうこう".to_string()
            })
        );
    }

    #[test]
    fn shifts_marks_after_insert_before_them() {
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark(10..20, RichTextKind::Bold);

        meta.apply_text_edit(3, 0, 4);

        assert_eq!(meta.marks()[0].range(), &(14..24));
    }

    #[test]
    fn shrinks_marks_overlapping_deleted_text() {
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark(10..20, RichTextKind::Bold);

        meta.apply_text_edit(14, 3, 0);

        assert_eq!(meta.marks()[0].range(), &(10..17));
    }

    #[test]
    fn drops_marks_fully_deleted() {
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark(10..20, RichTextKind::Bold);

        meta.apply_text_edit(8, 20, 0);

        assert!(meta.marks().is_empty());
    }

    #[test]
    fn toggle_removes_style_when_selection_is_fully_marked() {
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark(10..20, RichTextKind::Bold);

        meta.toggle_mark(12..18, RichTextKind::Bold);

        assert_eq!(meta.marks().len(), 2);
        assert_eq!(meta.marks()[0].range(), &(10..12));
        assert_eq!(meta.marks()[1].range(), &(18..20));
    }

    #[test]
    fn toggle_applies_style_when_selection_has_unmarked_text() {
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark(10..14, RichTextKind::Bold);

        meta.toggle_mark(10..20, RichTextKind::Bold);

        assert_eq!(meta.marks().len(), 1);
        assert_eq!(meta.marks()[0].range(), &(10..20));
    }

    #[test]
    fn toggle_removes_style_covered_by_multiple_marks() {
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark(10..14, RichTextKind::Bold);
        meta.add_mark(14..20, RichTextKind::Bold);

        meta.toggle_mark(10..20, RichTextKind::Bold);

        assert!(meta.marks().is_empty());
    }

    #[test]
    fn toggle_page_break_removes_existing_break_at_column() {
        let mut meta = RichTextDocumentMeta::default();
        meta.toggle_mark(10..10, RichTextKind::PageBreak { column: 3 });
        assert_eq!(meta.marks().len(), 1);

        meta.toggle_mark(10..10, RichTextKind::PageBreak { column: 3 });

        assert!(meta.marks().is_empty());
    }

    #[test]
    fn text_edit_does_not_shift_page_break_columns() {
        let mut meta = RichTextDocumentMeta::default();
        meta.set_page_break_column(3, 10);

        meta.apply_text_edit(0, 0, 5);

        assert_eq!(
            meta.marks(),
            &[RichTextMark {
                range: 10..10,
                kind: RichTextKind::PageBreak { column: 3 },
            }]
        );
    }

    #[test]
    fn syncs_meta_with_valid_edit_batch() {
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark("前置".len().."前置テスト".len(), RichTextKind::Bold);

        sync_meta_after_text_change(
            &mut meta,
            "前置テスト",
            "入力前置テスト",
            &[RichTextEdit::new(
                0,
                String::new(),
                "入力".to_string(),
                true,
            )],
        );

        assert_eq!(
            meta.marks()[0].range(),
            &("入力前置".len().."入力前置テスト".len())
        );
    }

    #[test]
    fn syncs_meta_with_single_change_fallback() {
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark("前置".len().."前置テスト".len(), RichTextKind::Bold);

        sync_meta_after_text_change(&mut meta, "前置テスト", "入力前置テスト", &[]);

        assert_eq!(
            meta.marks()[0].range(),
            &("入力前置".len().."入力前置テスト".len())
        );
    }

    #[test]
    fn single_change_keeps_utf8_boundaries() {
        assert_eq!(
            single_change("何かを入力", "何かを入力する"),
            Some(("何かを入力".len().."何かを入力".len(), "する".to_string()))
        );
    }

    #[test]
    fn writes_epub_zip_with_required_mimetype_first() {
        let directory = std::env::temp_dir();
        let path = directory.join(format!("soukou-rich-text-test-{}.epub", std::process::id()));
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark(0..6, RichTextKind::Emphasis);

        export_epub(path.as_path(), "題名", "本文です", &meta).expect("export epub");
        let bytes = std::fs::read(path.as_path()).expect("read exported epub");
        std::fs::remove_file(path).expect("remove exported epub");

        assert!(bytes.starts_with(b"PK\x03\x04"));
        assert!(
            bytes
                .windows(b"mimetype".len())
                .any(|window| window == b"mimetype")
        );
        assert!(
            bytes
                .windows(b"application/epub+zip".len())
                .any(|window| window == b"application/epub+zip")
        );
    }

    #[test]
    fn writes_word_zip_with_required_parts() {
        let directory = std::env::temp_dir();
        let path = directory.join(format!("soukou-rich-text-test-{}.docx", std::process::id()));
        let mut meta = RichTextDocumentMeta::default();
        meta.add_mark(0.."見出し".len(), RichTextKind::Heading { level: 1 });
        meta.add_mark(
            "見出し\n".len().."見出し\n本文".len(),
            RichTextKind::Emphasis,
        );

        export_word(path.as_path(), "見出し\n本文", &meta).expect("export word");
        let bytes = std::fs::read(path.as_path()).expect("read exported word");
        std::fs::remove_file(path).expect("remove exported word");

        assert!(bytes.starts_with(b"PK\x03\x04"));
        assert!(
            bytes
                .windows(b"word/document.xml".len())
                .any(|window| window == b"word/document.xml")
        );
        assert!(
            bytes
                .windows(b"[Content_Types].xml".len())
                .any(|window| window == b"[Content_Types].xml")
        );
    }
}
