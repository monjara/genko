use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use crc32fast::Hasher;
use richtext::{BlockKind, InlineStyle, RichDocument};
use ttf_parser::Face;
use xmlwriter::{Options as XmlOptions, XmlWriter};

const APP_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/ZenOldMincho-Regular.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Pdf,
    Word,
    Epub,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportWritingMode {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    pub writing_mode: ExportWritingMode,
}

impl ExportFormat {
    pub fn file_extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Word => "docx",
            Self::Epub => "epub",
        }
    }
}

#[derive(Debug)]
pub enum ExportError {
    InvalidFont,
    MissingGlyph(char),
    Io(std::io::Error),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFont => write!(f, "埋め込みフォントを読み込めませんでした"),
            Self::MissingGlyph(ch) => write!(f, "フォントに文字 `{ch}` を描画するグリフがありません"),
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
        ExportFormat::Pdf => export_pdf(document, options)?,
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
        build_epub_package_xml(&export, options).into_bytes(),
    );
    zip.push_stored("OEBPS/nav.xhtml", build_epub_nav_xml().into_bytes());
    zip.push_stored(
        "OEBPS/text.xhtml",
        build_epub_text_xml(&export, options).into_bytes(),
    );
    zip.finish()
}

fn export_pdf(document: &RichDocument, options: ExportOptions) -> Result<Vec<u8>, ExportError> {
    let export = ExportDocument::from(document);
    let face = Face::parse(APP_FONT_BYTES, 0).map_err(|_| ExportError::InvalidFont)?;
    let units_per_em = face.units_per_em() as f32;
    let mut glyphs = GlyphPlan::new();
    glyphs.collect(&face, &export)?;

    let mut writer = PdfWriter::new();
    let font_stream_id = writer.alloc_id();
    let font_descriptor_id = writer.alloc_id();
    let cid_font_id = writer.alloc_id();
    let to_unicode_id = writer.alloc_id();
    let type0_font_id = writer.alloc_id();
    let content_id = writer.alloc_id();
    let page_id = writer.alloc_id();
    let pages_id = writer.alloc_id();
    let catalog_id = writer.alloc_id();

    writer.object(catalog_id, format!("<< /Type /Catalog /Pages {} 0 R >>", pages_id));
    writer.object(pages_id, format!("<< /Type /Pages /Count 1 /Kids [ {} 0 R ] >>", page_id));

    let content_stream = build_pdf_content_stream(&export, &glyphs, options);
    writer.stream(
        content_id,
        format!("<< /Length {} >>", content_stream.len()),
        content_stream.as_bytes(),
    );

    let media_box = match options.writing_mode {
        ExportWritingMode::Vertical => "[0 0 842 595]",
        ExportWritingMode::Horizontal => "[0 0 595 842]",
    };
    let page = format!(
        "<< /Type /Page /Parent {pages_id} 0 R /MediaBox {media_box} /Resources << /Font << /F1 {type0_font_id} 0 R >> >> /Contents {content_id} 0 R >>"
    );
    writer.object(page_id, page);

    writer.stream(font_stream_id, format!(
        "<< /Length {} /Length1 {} >>",
        APP_FONT_BYTES.len(),
        APP_FONT_BYTES.len()
    ), APP_FONT_BYTES);

    let bbox = face.global_bounding_box();
    let ascent = face.ascender() as i32;
    let descent = face.descender() as i32;
    let cap_height = face.capital_height().unwrap_or(face.ascender()) as i32;
    let descriptor = format!(
        "<< /Type /FontDescriptor /FontName /ZenOldMincho /Flags 4 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 {} 0 R >>",
        bbox.x_min, bbox.y_min, bbox.x_max, bbox.y_max, ascent, descent, cap_height, font_stream_id
    );
    writer.object(font_descriptor_id, descriptor);

    let widths = glyphs.widths_pdf(units_per_em);
    let cid_font = format!(
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /ZenOldMincho /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {font_descriptor_id} 0 R /CIDToGIDMap /Identity /DW 1000 /W [ {widths} ] >>"
    );
    writer.object(cid_font_id, cid_font);

    writer.stream(
        to_unicode_id,
        format!("<< /Length {} >>", glyphs.to_unicode_cmap().len()),
        glyphs.to_unicode_cmap().as_bytes(),
    );

    let type0_font = format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /ZenOldMincho /Encoding /Identity-H /DescendantFonts [ {cid_font_id} 0 R ] /ToUnicode {to_unicode_id} 0 R >>"
    );
    writer.object(type0_font_id, type0_font);

    Ok(writer.finish(catalog_id))
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

fn build_epub_package_xml(document: &ExportDocument, options: ExportOptions) -> String {
    let title = first_heading_title(document).unwrap_or_else(|| "草稿".to_string());
    let page_progression = if options.writing_mode == ExportWritingMode::Vertical {
        r#" page-progression-direction="rtl""#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" unique-identifier="bookid" xmlns="http://www.idpf.org/2007/opf"{}>
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">urn:soukou:export</dc:identifier>
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
        page_progression,
        xml_escape(title.as_str())
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

fn build_epub_text_xml(document: &ExportDocument, options: ExportOptions) -> String {
    let body_style = match options.writing_mode {
        ExportWritingMode::Vertical => {
            "font-family: serif; line-height: 1.8; margin: 2em; writing-mode: vertical-rl;"
        }
        ExportWritingMode::Horizontal => "font-family: serif; line-height: 1.8; margin: 2em;",
    };
    let mut html = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" lang="ja">
  <head>
    <title>草稿</title>
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

#[derive(Default)]
struct GlyphPlan {
    glyphs: BTreeMap<u16, GlyphInfo>,
}

#[derive(Clone)]
struct GlyphInfo {
    unicode: char,
    advance_width: u16,
}

impl GlyphPlan {
    fn new() -> Self {
        Self::default()
    }

    fn collect(&mut self, face: &Face<'_>, document: &ExportDocument) -> Result<(), ExportError> {
        for paragraph in &document.paragraphs {
            for run in &paragraph.runs {
                for ch in run.text.chars() {
                    let glyph = face.glyph_index(ch).ok_or(ExportError::MissingGlyph(ch))?;
                    let advance = face.glyph_hor_advance(glyph).unwrap_or(face.units_per_em());
                    self.glyphs.entry(glyph.0).or_insert(GlyphInfo {
                        unicode: ch,
                        advance_width: advance,
                    });
                }
            }
        }
        Ok(())
    }

    fn encode_text(&self, text: &str, face: &Face<'_>) -> Result<String, ExportError> {
        let mut encoded = String::new();
        for ch in text.chars() {
            let glyph = face.glyph_index(ch).ok_or(ExportError::MissingGlyph(ch))?;
            write!(&mut encoded, "{:04X}", glyph.0).ok();
        }
        Ok(encoded)
    }

    fn widths_pdf(&self, units_per_em: f32) -> String {
        let mut parts = Vec::new();
        for (glyph_id, info) in &self.glyphs {
            let width = ((info.advance_width as f32 / units_per_em) * 1000.0).round() as u16;
            parts.push(format!("{glyph_id} [ {width} ]"));
        }
        parts.join(" ")
    }

    fn to_unicode_cmap(&self) -> String {
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def\n/CMapName /ZenOldMinchoUnicode def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
        );
        let count = self.glyphs.len();
        write!(&mut cmap, "{count} beginbfchar\n").ok();
        for (glyph_id, info) in &self.glyphs {
            write!(
                &mut cmap,
                "<{:04X}> <{:04X}>\n",
                glyph_id,
                info.unicode as u32
            )
            .ok();
        }
        cmap.push_str(
            "endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend",
        );
        cmap
    }
}

fn build_pdf_content_stream(
    document: &ExportDocument,
    glyphs: &GlyphPlan,
    options: ExportOptions,
) -> String {
    let face = Face::parse(APP_FONT_BYTES, 0).expect("font bytes must be valid");
    match options.writing_mode {
        ExportWritingMode::Vertical => {
            return build_pdf_vertical_content_stream(document, glyphs, &face);
        }
        ExportWritingMode::Horizontal => {}
    }
    let mut stream = String::new();
    let mut current_y = 790.0;
    let strike_metrics = PdfStrikeMetrics::from_face(&face, 14.0);

    for paragraph in &document.paragraphs {
        let font_size = match paragraph.block_kind {
            BlockKind::HeadingLarge => 24.0,
            BlockKind::HeadingMedium => 18.0,
            BlockKind::Body => 14.0,
        };
        let strike_metrics = strike_metrics.scaled(font_size / 14.0);
        stream.push_str("BT\n");
        write!(&mut stream, "/F1 {} Tf\n1 0 0 1 50 {:.1} Tm\n", font_size, current_y).ok();
        let mut current_x = 50.0;
        let mut strike_segments = Vec::new();
        for run in &paragraph.runs {
            if run.text.is_empty() {
                continue;
            }
            let encoded = glyphs.encode_text(run.text.as_str(), &face).unwrap_or_default();
            if run.style.bold {
                stream.push_str("0.25 w 2 Tr\n");
            } else {
                stream.push_str("0 Tr\n");
            }
            write!(&mut stream, "<{}> Tj\n", encoded).ok();
            let width = text_width_units(run.text.as_str(), glyphs, &face, font_size);
            if run.style.strikethrough {
                let strike_y = current_y + strike_metrics.center_y;
                strike_segments.push((current_x, current_x + width, strike_y));
            }
            current_x += width;
        }
        stream.push_str("ET\n");
        for (start_x, end_x, strike_y) in strike_segments {
            write!(
                &mut stream,
                "{:.1} {:.1} m {:.1} {:.1} l S\n",
                start_x, strike_y, end_x, strike_y
            )
            .ok();
        }
        current_y -= match paragraph.block_kind {
            BlockKind::HeadingLarge => 34.0,
            BlockKind::HeadingMedium => 28.0,
            BlockKind::Body => 22.0,
        };
    }
    stream
}

fn build_pdf_vertical_content_stream(
    document: &ExportDocument,
    glyphs: &GlyphPlan,
    face: &Face<'_>,
) -> String {
    let mut stream = String::new();
    let mut current_x = 792.0;
    let top_y = 540.0;
    let bottom_y = 55.0;
    let column_gap = 28.0;
    let base_strike_metrics = PdfStrikeMetrics::from_face(face, 14.0);

    for paragraph in &document.paragraphs {
        let font_size = match paragraph.block_kind {
            BlockKind::HeadingLarge => 24.0,
            BlockKind::HeadingMedium => 18.0,
            BlockKind::Body => 14.0,
        };
        let line_step = font_size * 1.35;
        let strike_metrics = base_strike_metrics.scaled(font_size / 14.0);
        let mut strike_segments = Vec::new();
        let mut current_y = top_y;

        for run in &paragraph.runs {
            if run.text.is_empty() {
                continue;
            }
            let mut strike_start_y = None;
            let mut strike_end_y = None;

            for ch in run.text.chars() {
                if current_y < bottom_y {
                    if let (Some(start_y), Some(end_y)) = (strike_start_y.take(), strike_end_y.take()) {
                        strike_segments.push((current_x + strike_metrics.center_x, start_y, end_y));
                    }
                    current_x -= font_size + column_gap;
                    current_y = top_y;
                }

                let glyph_layout = vertical_glyph_layout(ch, font_size);
                let mut buffer = [0; 4];
                let encoded = glyphs
                    .encode_text(ch.encode_utf8(&mut buffer), face)
                    .unwrap_or_default();
                stream.push_str("BT\n");
                if run.style.bold {
                    stream.push_str("0.25 w 2 Tr\n");
                } else {
                    stream.push_str("0 Tr\n");
                }
                write!(
                    &mut stream,
                    "/F1 {} Tf\n1 0 0 1 {:.1} {:.1} Tm\n<{}> Tj\nET\n",
                    glyph_layout.font_size,
                    current_x + glyph_layout.x_offset,
                    current_y + glyph_layout.y_offset,
                    encoded
                )
                .ok();
                if run.style.strikethrough {
                    strike_start_y
                        .get_or_insert(current_y + strike_metrics.top_y);
                    strike_end_y = Some(current_y + strike_metrics.bottom_y);
                }
                current_y -= line_step;
            }

            if let (Some(start_y), Some(end_y)) = (strike_start_y, strike_end_y) {
                strike_segments.push((current_x + strike_metrics.center_x, start_y, end_y));
            }
        }

        for (strike_x, start_y, end_y) in strike_segments {
            write!(
                &mut stream,
                "{:.1} w\n{:.1} {:.1} m {:.1} {:.1} l S\n",
                strike_metrics.stroke_width, strike_x, start_y, strike_x, end_y
            )
            .ok();
        }

        current_x -= font_size + column_gap;
    }
    stream
}

#[derive(Clone, Copy)]
struct VerticalGlyphLayout {
    font_size: f32,
    x_offset: f32,
    y_offset: f32,
}

fn vertical_glyph_layout(ch: char, font_size: f32) -> VerticalGlyphLayout {
    if is_vertical_corner_punctuation(ch) {
        return VerticalGlyphLayout {
            font_size: font_size * 0.5,
            x_offset: font_size * 0.22,
            y_offset: font_size * 0.28,
        };
    }

    if is_vertical_small_kana(ch) {
        return VerticalGlyphLayout {
            font_size: font_size * 0.65,
            x_offset: font_size * 0.18,
            y_offset: font_size * 0.2,
        };
    }

    VerticalGlyphLayout {
        font_size,
        x_offset: 0.0,
        y_offset: 0.0,
    }
}

fn is_vertical_corner_punctuation(ch: char) -> bool {
    matches!(ch, '、' | '。' | '，' | '．')
}

fn is_vertical_small_kana(ch: char) -> bool {
    matches!(
        ch,
        'ぁ'
            | 'ぃ'
            | 'ぅ'
            | 'ぇ'
            | 'ぉ'
            | 'っ'
            | 'ゃ'
            | 'ゅ'
            | 'ょ'
            | 'ゎ'
            | 'ゕ'
            | 'ゖ'
            | 'ァ'
            | 'ィ'
            | 'ゥ'
            | 'ェ'
            | 'ォ'
            | 'ッ'
            | 'ャ'
            | 'ュ'
            | 'ョ'
            | 'ヮ'
            | 'ヵ'
            | 'ヶ'
    )
}

#[derive(Clone, Copy)]
struct PdfStrikeMetrics {
    center_x: f32,
    center_y: f32,
    top_y: f32,
    bottom_y: f32,
    stroke_width: f32,
}

impl PdfStrikeMetrics {
    fn from_face(face: &Face<'_>, font_size: f32) -> Self {
        let units_per_em = face.units_per_em() as f32;
        let bbox = face.global_bounding_box();
        let center_x = ((bbox.x_min as f32 + bbox.x_max as f32) * 0.5 / units_per_em) * font_size;
        let center_y = ((bbox.y_min as f32 + bbox.y_max as f32) * 0.5 / units_per_em) * font_size;
        let top_y = (bbox.y_max as f32 / units_per_em) * font_size;
        let bottom_y = (bbox.y_min as f32 / units_per_em) * font_size;
        let stroke_width = (font_size * 0.08).max(0.8);
        Self {
            center_x,
            center_y,
            top_y,
            bottom_y,
            stroke_width,
        }
    }

    fn scaled(self, factor: f32) -> Self {
        Self {
            center_x: self.center_x * factor,
            center_y: self.center_y * factor,
            top_y: self.top_y * factor,
            bottom_y: self.bottom_y * factor,
            stroke_width: self.stroke_width * factor,
        }
    }
}

fn text_width_units(text: &str, glyphs: &GlyphPlan, face: &Face<'_>, font_size: f32) -> f32 {
    let units_per_em = face.units_per_em() as f32;
    let total = text
        .chars()
        .filter_map(|ch| face.glyph_index(ch))
        .filter_map(|glyph| glyphs.glyphs.get(&glyph.0))
        .map(|info| info.advance_width as f32)
        .sum::<f32>();
    total / units_per_em * font_size
}

struct PdfWriter {
    objects: Vec<(u32, Vec<u8>)>,
    next_id: u32,
}

impl PdfWriter {
    fn new() -> Self {
        Self {
            objects: Vec::new(),
            next_id: 1,
        }
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn object(&mut self, id: u32, body: String) {
        self.objects.push((id, body.into_bytes()));
    }

    fn stream(&mut self, id: u32, dict: impl AsRef<str>, data: &[u8]) {
        let mut body = String::new();
        body.push_str(dict.as_ref());
        body.push_str("\nstream\n");
        let mut bytes = body.into_bytes();
        bytes.extend_from_slice(data);
        bytes.extend_from_slice(b"\nendstream");
        self.objects.push((id, bytes));
    }

    fn finish(mut self, root_id: u32) -> Vec<u8> {
        self.objects.sort_by_key(|(id, _)| *id);
        let mut output = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::with_capacity(self.objects.len() + 1);
        offsets.push(0usize);

        for (id, body) in &self.objects {
            offsets.push(output.len());
            write!(&mut output, "{} 0 obj\n", id).ok();
            output.extend_from_slice(body);
            output.extend_from_slice(b"\nendobj\n");
        }

        let xref_start = output.len();
        write!(&mut output, "xref\n0 {}\n", self.objects.len() + 1).ok();
        output.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            write!(&mut output, "{offset:010} 00000 n \n").ok();
        }
        write!(
            &mut output,
            "trailer\n<< /Size {} /Root {} 0 R >>\nstartxref\n{}\n%%EOF",
            self.objects.len() + 1,
            root_id,
            xref_start
        )
        .ok();
        output
    }
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
    use super::{
        ExportDocument, ExportFormat, ExportOptions, ExportWritingMode, build_epub_text_xml,
        export_docx, export_epub, export_pdf,
    };
    use richtext::{BlockKind, InlineStyle, RichDocument};

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
    fn pdf_export_has_pdf_signature() {
        let bytes =
            export_pdf(&sample_document(), ExportOptions::default()).expect("pdf export should succeed");
        assert!(bytes.starts_with(b"%PDF-1.7"));
    }

    #[test]
    fn vertical_epub_uses_vertical_writing_mode_css() {
        let html = build_epub_text_xml(
            &ExportDocument::from(&sample_document()),
            ExportOptions {
                writing_mode: ExportWritingMode::Vertical,
            },
        );
        assert!(html.contains("writing-mode: vertical-rl;"));
    }

    #[test]
    fn export_format_extensions_are_stable() {
        assert_eq!(ExportFormat::Pdf.file_extension(), "pdf");
        assert_eq!(ExportFormat::Word.file_extension(), "docx");
        assert_eq!(ExportFormat::Epub.file_extension(), "epub");
    }
}
