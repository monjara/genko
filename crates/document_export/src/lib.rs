use std::collections::BTreeSet;
use std::path::Path;

use crc32fast::Hasher;
use richtext::{BlockKind, EpubMetadata, InlineStyle, RichDocument};
use xmlwriter::{Options as XmlOptions, XmlWriter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Word,
    Epub,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportWritingMode {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    pub writing_mode: ExportWritingMode,
    pub epub_metadata: Option<EpubMetadata>,
}

impl ExportFormat {
    pub fn file_extension(self) -> &'static str {
        match self {
            Self::Word => "docx",
            Self::Epub => "epub",
        }
    }
}

#[derive(Debug)]
pub enum ExportError {
    Io(std::io::Error),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<std::io::Error> for ExportError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn write_export(
    path: &Path,
    format: ExportFormat,
    document: &RichDocument,
    options: ExportOptions,
) -> Result<(), ExportError> {
    let bytes = match format {
        ExportFormat::Word => export_docx(document, options),
        ExportFormat::Epub => export_epub(document, options),
    };
    std::fs::write(path, bytes)?;
    Ok(())
}

fn export_docx(document: &RichDocument, options: ExportOptions) -> Vec<u8> {
    let export = ExportDocument::from(document);
    let mut zip = SimpleZip::new();
    zip.push_stored(
        "word/document.xml",
        build_docx_document_xml(&export, options).into_bytes(),
    );
    zip.push_stored("word/styles.xml", build_docx_styles_xml().into_bytes());
    zip.push_stored(
        "word/_rels/document.xml.rels",
        build_docx_document_rels().into_bytes(),
    );
    zip.push_stored("_rels/.rels", build_root_relationships_xml().into_bytes());
    zip.push_stored("[Content_Types].xml", build_docx_content_types().into_bytes());
    zip.finish()
}

fn export_epub(document: &RichDocument, options: ExportOptions) -> Vec<u8> {
    let export = ExportDocument::from(document);
    let mut zip = SimpleZip::new();
    zip.push_stored("mimetype", b"application/epub+zip".to_vec());
    zip.push_stored(
        "META-INF/container.xml",
        build_epub_container_xml().into_bytes(),
    );
    zip.push_stored(
        "OEBPS/content.opf",
        build_epub_package_xml(&export, &options).into_bytes(),
    );
    zip.push_stored("OEBPS/nav.xhtml", build_epub_nav_xml().into_bytes());
    zip.push_stored(
        "OEBPS/text.xhtml",
        build_epub_text_xml(&export, &options).into_bytes(),
    );
    zip.finish()
}

#[derive(Clone, Debug)]
struct ExportDocument {
    paragraphs: Vec<Paragraph>,
}

impl From<&RichDocument> for ExportDocument {
    fn from(document: &RichDocument) -> Self {
        let mut paragraphs = Vec::new();
        let text = document.plain_text();
        let mut start = 0;

        loop {
            let end = text[start..]
                .find('\n')
                .map(|index| start + index)
                .unwrap_or(text.len());
            let paragraph_text = &text[start..end];
            let block_kind = document.block_kind_for_offset(start.min(text.len()));
            let inline_marks = document
                .spans
                .iter()
                .filter(|mark| mark.start < end && start < mark.end)
                .cloned()
                .collect::<Vec<_>>();
            paragraphs.push(Paragraph {
                block_kind,
                runs: paragraph_runs(start, paragraph_text, &inline_marks),
            });

            if end == text.len() {
                break;
            }
            start = end + 1;
        }

        Self { paragraphs }
    }
}

#[derive(Clone, Debug)]
struct Paragraph {
    block_kind: BlockKind,
    runs: Vec<Run>,
}

#[derive(Clone, Debug, Default)]
struct RunStyle {
    bold: bool,
    strikethrough: bool,
}

#[derive(Clone, Debug)]
struct Run {
    text: String,
    style: RunStyle,
}

fn paragraph_runs(start_offset: usize, text: &str, marks: &[richtext::InlineMark]) -> Vec<Run> {
    if text.is_empty() {
        return vec![Run {
            text: String::new(),
            style: RunStyle::default(),
        }];
    }

    let mut boundaries = BTreeSet::from([start_offset, start_offset + text.len()]);
    for mark in marks {
        boundaries.insert(mark.start.max(start_offset).min(start_offset + text.len()));
        boundaries.insert(mark.end.max(start_offset).min(start_offset + text.len()));
    }

    let boundaries = boundaries.into_iter().collect::<Vec<_>>();
    let mut runs = Vec::new();
    for window in boundaries.windows(2) {
        let local_start = window[0] - start_offset;
        let local_end = window[1] - start_offset;
        let slice = &text[local_start..local_end];
        if slice.is_empty() {
            continue;
        }
        let mut style = RunStyle::default();
        for mark in marks {
            if mark.start < window[1] && window[0] < mark.end {
                match mark.style {
                    InlineStyle::Bold => style.bold = true,
                    InlineStyle::Strikethrough => style.strikethrough = true,
                }
            }
        }
        runs.push(Run {
            text: slice.to_string(),
            style,
        });
    }
    runs
}

fn build_docx_document_xml(document: &ExportDocument, options: ExportOptions) -> String {
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
    for paragraph in &document.paragraphs {
        xml.start_element("w:p");
        match paragraph.block_kind {
            BlockKind::HeadingLarge | BlockKind::HeadingMedium => {
                xml.start_element("w:pPr");
                xml.start_element("w:pStyle");
                xml.write_attribute(
                    "w:val",
                    match paragraph.block_kind {
                        BlockKind::HeadingLarge => "Heading1",
                        BlockKind::HeadingMedium => "Heading2",
                        BlockKind::Body => "Normal",
                    },
                );
                xml.end_element();
                xml.end_element();
            }
            BlockKind::Body => {}
        }
        for run in &paragraph.runs {
            xml.start_element("w:r");
            if run.style.bold || run.style.strikethrough {
                xml.start_element("w:rPr");
                if run.style.bold {
                    xml.start_element("w:b");
                    xml.end_element();
                }
                if run.style.strikethrough {
                    xml.start_element("w:strike");
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
    match options.writing_mode {
        ExportWritingMode::Vertical => {
            xml.write_attribute("w:w", "16838");
            xml.write_attribute("w:h", "11906");
            xml.write_attribute("w:orient", "landscape");
        }
        ExportWritingMode::Horizontal => {
            xml.write_attribute("w:w", "11906");
            xml.write_attribute("w:h", "16838");
        }
    }
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
    if options.writing_mode == ExportWritingMode::Vertical {
        xml.start_element("w:textDirection");
        xml.write_attribute("w:val", "tbRl");
        xml.end_element();
    }
    xml.end_element();
    xml.end_element();
    xml.end_element();
    xml.end_document()
}

fn build_docx_styles_xml() -> String {
    let mut xml = XmlWriter::new(XmlOptions::default());
    xml.start_element("w:styles");
    xml.write_attribute(
        "xmlns:w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    );
    write_docx_style(&mut xml, "Normal", "Normal", None, 22, false);
    write_docx_style(&mut xml, "Heading1", "大見出し", Some("Normal"), 36, true);
    write_docx_style(&mut xml, "Heading2", "小見出し", Some("Normal"), 28, true);
    xml.end_element();
    xml.end_document()
}

fn write_docx_style(
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

fn build_docx_document_rels() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#
        .to_string()
}

fn build_root_relationships_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#
        .to_string()
}

fn build_docx_content_types() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#
        .to_string()
}

fn build_epub_container_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#
        .to_string()
}

fn build_epub_package_xml(document: &ExportDocument, options: &ExportOptions) -> String {
    let metadata = resolved_epub_metadata(document, options.epub_metadata.as_ref());
    let page_progression = if options.writing_mode == ExportWritingMode::Vertical {
        r#" page-progression-direction="rtl""#
    } else {
        ""
    };
    let creators = metadata
        .creators
        .iter()
        .map(|creator| format!("    <dc:creator>{}</dc:creator>\n", xml_escape(creator)))
        .collect::<String>();
    let description = metadata
        .description
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| format!("    <dc:description>{}</dc:description>\n", xml_escape(value)))
        .unwrap_or_default();
    let publisher = metadata
        .publisher
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| format!("    <dc:publisher>{}</dc:publisher>\n", xml_escape(value)))
        .unwrap_or_default();
    let rights = metadata
        .rights
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| format!("    <dc:rights>{}</dc:rights>\n", xml_escape(value)))
        .unwrap_or_default();
    let published_at = metadata
        .published_at
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| format!("    <dc:date>{}</dc:date>\n", xml_escape(value)))
        .unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" unique-identifier="bookid" xmlns="http://www.idpf.org/2007/opf">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">{}</dc:identifier>
    <dc:title>{}</dc:title>
{}    <dc:language>{}</dc:language>
{}{}{}{}
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="text" href="text.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine{}>
    <itemref idref="text"/>
  </spine>
</package>"#,
        xml_escape(metadata.identifier.as_str()),
        xml_escape(metadata.title.as_str()),
        creators,
        xml_escape(metadata.language.as_str()),
        description,
        publisher,
        rights,
        published_at,
        page_progression,
    )
}

fn build_epub_nav_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="ja">
  <head><title>目次</title></head>
  <body>
    <nav epub:type="toc" id="toc">
      <ol><li><a href="text.xhtml">本文</a></li></ol>
    </nav>
  </body>
</html>"#
        .to_string()
}

fn build_epub_text_xml(document: &ExportDocument, options: &ExportOptions) -> String {
    let metadata = resolved_epub_metadata(document, options.epub_metadata.as_ref());
    let body_style = match options.writing_mode {
        ExportWritingMode::Vertical => {
            "font-family: serif; line-height: 1.8; margin: 2em; writing-mode: vertical-rl;"
        }
        ExportWritingMode::Horizontal => "font-family: serif; line-height: 1.8; margin: 2em;",
    };
    let mut html = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" lang="{}">
  <head>
    <title>{}</title>
    <style>
      body {{ {} }}
      h1 {{ font-size: 2em; margin: 1.6em 0 0.8em; }}
      h2 {{ font-size: 1.5em; margin: 1.4em 0 0.7em; }}
      p {{ margin: 0 0 1em; }}
      .strike {{ text-decoration: line-through; }}
      strong {{ font-weight: 700; }}
    </style>
  </head>
  <body>
"#,
        xml_escape(metadata.language.as_str()),
        xml_escape(metadata.title.as_str()),
        body_style
    );

    for paragraph in &document.paragraphs {
        match paragraph.block_kind {
            BlockKind::HeadingLarge => html.push_str("<h1>"),
            BlockKind::HeadingMedium => html.push_str("<h2>"),
            BlockKind::Body => html.push_str("<p>"),
        }
        for run in &paragraph.runs {
            let content = xml_escape(run.text.as_str());
            if run.style.bold {
                html.push_str("<strong>");
            }
            if run.style.strikethrough {
                html.push_str(r#"<span class="strike">"#);
            }
            html.push_str(content.as_str());
            if run.style.strikethrough {
                html.push_str("</span>");
            }
            if run.style.bold {
                html.push_str("</strong>");
            }
        }
        match paragraph.block_kind {
            BlockKind::HeadingLarge => html.push_str("</h1>\n"),
            BlockKind::HeadingMedium => html.push_str("</h2>\n"),
            BlockKind::Body => html.push_str("</p>\n"),
        }
    }

    html.push_str("  </body>\n</html>");
    html
}

fn resolved_epub_metadata(
    document: &ExportDocument,
    metadata: Option<&EpubMetadata>,
) -> EpubMetadata {
    let mut resolved = metadata.cloned().unwrap_or_default();
    if resolved.title.trim().is_empty() {
        resolved.title = first_heading_title(document).unwrap_or_else(|| "草稿".to_string());
    }
    resolved.creators = resolved
        .creators
        .into_iter()
        .map(|creator| creator.trim().to_string())
        .filter(|creator| !creator.is_empty())
        .collect();
    if resolved.language.trim().is_empty() {
        resolved.language = "ja".to_string();
    }
    if resolved.identifier.trim().is_empty() {
        resolved.identifier = "urn:soukou:export".to_string();
    }
    resolved
}

fn first_heading_title(document: &ExportDocument) -> Option<String> {
    document.paragraphs.iter().find_map(|paragraph| {
        matches!(paragraph.block_kind, BlockKind::HeadingLarge | BlockKind::HeadingMedium)
            .then_some(paragraph.runs.iter().map(|run| run.text.as_str()).collect::<String>())
            .filter(|value| !value.is_empty())
    })
}

fn xml_escape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(ch),
        }
    }
    output
}

struct SimpleZip {
    entries: Vec<ZipEntry>,
}

struct ZipEntry {
    name: String,
    data: Vec<u8>,
}

impl SimpleZip {
    fn new() -> Self {
        Self { entries: Vec::new() }
    }

    fn push_stored(&mut self, name: impl Into<String>, data: Vec<u8>) {
        self.entries.push(ZipEntry {
            name: name.into(),
            data,
        });
    }

    fn finish(self) -> Vec<u8> {
        let mut output = Vec::new();
        let mut central_directory = Vec::new();
        let mut offset = 0u32;

        for entry in &self.entries {
            let crc = crc32(entry.data.as_slice());
            let name_bytes = entry.name.as_bytes();
            write_local_file_header(&mut output, name_bytes, entry.data.len() as u32, crc);
            output.extend_from_slice(name_bytes);
            output.extend_from_slice(&entry.data);

            write_central_directory_header(
                &mut central_directory,
                name_bytes,
                entry.data.len() as u32,
                crc,
                offset,
            );
            central_directory.extend_from_slice(name_bytes);

            offset = output.len() as u32;
        }

        let central_offset = output.len() as u32;
        output.extend_from_slice(&central_directory);
        let central_size = central_directory.len() as u32;
        write_end_of_central_directory(
            &mut output,
            self.entries.len() as u16,
            central_size,
            central_offset,
        );
        output
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    hasher.finalize()
}

fn write_local_file_header(buffer: &mut Vec<u8>, name: &[u8], size: u32, crc: u32) {
    buffer.extend_from_slice(&0x04034b50u32.to_le_bytes());
    buffer.extend_from_slice(&20u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&crc.to_le_bytes());
    buffer.extend_from_slice(&size.to_le_bytes());
    buffer.extend_from_slice(&size.to_le_bytes());
    buffer.extend_from_slice(&(name.len() as u16).to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
}

fn write_central_directory_header(
    buffer: &mut Vec<u8>,
    name: &[u8],
    size: u32,
    crc: u32,
    offset: u32,
) {
    buffer.extend_from_slice(&0x02014b50u32.to_le_bytes());
    buffer.extend_from_slice(&20u16.to_le_bytes());
    buffer.extend_from_slice(&20u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&crc.to_le_bytes());
    buffer.extend_from_slice(&size.to_le_bytes());
    buffer.extend_from_slice(&size.to_le_bytes());
    buffer.extend_from_slice(&(name.len() as u16).to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u32.to_le_bytes());
    buffer.extend_from_slice(&offset.to_le_bytes());
}

fn write_end_of_central_directory(
    buffer: &mut Vec<u8>,
    entry_count: u16,
    central_size: u32,
    central_offset: u32,
) {
    buffer.extend_from_slice(&0x06054b50u32.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&entry_count.to_le_bytes());
    buffer.extend_from_slice(&entry_count.to_le_bytes());
    buffer.extend_from_slice(&central_size.to_le_bytes());
    buffer.extend_from_slice(&central_offset.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::{ExportDocument, ExportFormat, ExportOptions, ExportWritingMode, build_epub_package_xml, build_epub_text_xml, export_docx, export_epub};
    use richtext::{BlockKind, EpubMetadata, InlineStyle, RichDocument};

    fn sample_document() -> RichDocument {
        let mut document = RichDocument::new("大見出し\n本文です\n".to_string());
        document.set_block_kind_for_range(0..12, BlockKind::HeadingLarge);
        let body_start = "大見出し\n".len();
        let body_end = body_start + "本文".len();
        document.toggle_inline_style(body_start..body_end, InlineStyle::Bold);
        document
    }

    #[test]
    fn docx_export_has_zip_signature() {
        let bytes = export_docx(&sample_document(), ExportOptions::default());
        assert!(bytes.starts_with(b"PK\x03\x04"));
    }

    #[test]
    fn epub_export_has_zip_signature() {
        let bytes = export_epub(&sample_document(), ExportOptions::default());
        assert!(bytes.starts_with(b"PK\x03\x04"));
    }

    #[test]
    fn vertical_epub_uses_vertical_writing_mode_css() {
        let html = build_epub_text_xml(
            &ExportDocument::from(&sample_document()),
            &ExportOptions {
                writing_mode: ExportWritingMode::Vertical,
                epub_metadata: None,
            },
        );
        assert!(html.contains("writing-mode: vertical-rl;"));
    }

    #[test]
    fn export_format_extensions_are_stable() {
        assert_eq!(ExportFormat::Word.file_extension(), "docx");
        assert_eq!(ExportFormat::Epub.file_extension(), "epub");
    }

    #[test]
    fn epub_package_xml_uses_metadata() {
        let package = build_epub_package_xml(
            &ExportDocument::from(&sample_document()),
            &ExportOptions {
                epub_metadata: Some(EpubMetadata {
                    title: "本の題名".to_string(),
                    creators: vec!["著者名".to_string()],
                    language: "ja".to_string(),
                    identifier: "urn:test:book".to_string(),
                    description: Some("説明".to_string()),
                    publisher: Some("出版社".to_string()),
                    rights: Some("All rights reserved".to_string()),
                    published_at: Some("2026-05-24".to_string()),
                }),
                ..ExportOptions::default()
            },
        );

        assert!(package.contains("<dc:title>本の題名</dc:title>"));
        assert!(package.contains("<dc:creator>著者名</dc:creator>"));
        assert!(package.contains("<dc:identifier id=\"bookid\">urn:test:book</dc:identifier>"));
        assert!(package.contains("<dc:publisher>出版社</dc:publisher>"));
    }

    #[test]
    fn vertical_epub_sets_rtl_page_progression_on_spine() {
        let package = build_epub_package_xml(
            &ExportDocument::from(&sample_document()),
            &ExportOptions {
                writing_mode: ExportWritingMode::Vertical,
                epub_metadata: None,
            },
        );

        assert!(package.contains(r#"<spine page-progression-direction="rtl">"#));
        assert!(!package.contains(r#"<package version="3.0" unique-identifier="bookid" xmlns="http://www.idpf.org/2007/opf" page-progression-direction="rtl">"#));
    }
}
