use std::{
    cmp::min,
    fs::File,
    io::{self, Write},
    ops::Range,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

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

        if range.is_empty() && !matches!(kind, RichTextKind::PageBreak) {
            return;
        }

        self.marks.push(RichTextMark { range, kind });
        self.marks
            .sort_by_key(|mark| (mark.range.start, mark.range.end));
    }

    pub fn marks(&self) -> &[RichTextMark] {
        self.marks.as_slice()
    }

    pub fn clear(&mut self) {
        self.marks.clear();
    }

    pub fn apply_text_edit(&mut self, start: usize, removed_len: usize, inserted_len: usize) {
        let removed_range = start..start.saturating_add(removed_len);
        self.marks = self
            .marks
            .drain(..)
            .filter_map(|mut mark| {
                mark.range = transform_range(mark.range, &removed_range, inserted_len)?;
                Some(mark)
            })
            .collect();
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
    Ruby { text: String },
    Heading { level: u8 },
    PageBreak,
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
        if mark.range.start < offset && !matches!(mark.kind, RichTextKind::PageBreak) {
            continue;
        }

        output.push_str(&escape_xml(&text[offset..mark.range.start]));
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
            RichTextKind::PageBreak => {
                output.push_str(r#"<div class="page-break"></div>"#);
            }
        }
        offset = mark.range.end;
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
}
