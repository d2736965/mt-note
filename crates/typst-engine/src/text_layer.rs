//! 文本层提取。
//!
//! 从 Typst 编译后的 `PagedDocument` 中提取文本信息，
//! 生成透明文字层 HTML，叠加在 SVG 渲染之上以支持文字选择。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use subsetter::{subset_with_variations, GlyphRemapper};
use typst_library::foundations::Bytes;
use typst_library::layout::{Frame, FrameItem, Point, Size};
use typst_library::text::{FontInstance, Tag, TextItem};

/// 文本层中的一项，对应一段可选择的文字。
#[derive(Debug, Clone)]
pub struct TextLayerItem {
    /// 文字内容。
    pub text: String,
    /// 在页面中的 X 坐标（pt）。
    pub x: f64,
    /// 在页面中的 Y 坐标（pt）。
    pub y: f64,
    /// 文字宽度（pt）。
    pub width: f64,
    /// 字号（pt）。
    pub font_size: f64,
    /// 运行时实际命中字体的 @font-face family 名（tl- 前缀 slug）。
    pub font: String,
}

/// 一个运行时字体的子集资产，内联进 text_layer 的 `<style>` 供 span 引用。
#[derive(Debug, Clone)]
pub struct FontFace {
    /// CSS @font-face 的 font-family 名（tl- 前缀 slug）。
    pub family: String,
    /// 子集化后字体数据的 base64。
    pub data_b64: String,
}

/// 文档级字体收集器：按 slug 聚合运行时字体的 glyph id 集，
/// `finish` 时按 glyph 子集化出 [`FontFace`] 列表。
#[derive(Default)]
pub struct FaceCollector {
    faces: BTreeMap<String, FaceAcc>,
    slug_cache: HashMap<(String, u32), String>,
}

#[derive(Default)]
struct FaceAcc {
    index: u32,
    data: Option<Bytes>,
    variations: Vec<(Tag, f32)>,
    glyphs: BTreeSet<u16>,
    chars: BTreeMap<char, u16>,
}

impl FaceCollector {
    /// 记录一个 TextItem 命中的字体、其 glyph id 与 char→glyph 映射，返回 slug。
    pub fn note(
        &mut self,
        font: &FontInstance,
        glyph_ids: impl IntoIterator<Item = (u16, Vec<char>)>,
    ) -> String {
        let f = font.font();
        let ps = f.post_script_name().unwrap_or_else(|| f.info().family.clone());
        let index = f.index();
        let slug = self
            .slug_cache
            .entry((ps.clone(), index))
            .or_insert_with(|| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                ps.hash(&mut h);
                index.hash(&mut h);
                f.data().len().hash(&mut h);
                font_slug(&ps, h.finish())
            })
            .clone();
        let acc = self.faces.entry(slug.clone()).or_insert_with(|| FaceAcc {
            index,
            data: Some(f.data().clone()),
            variations: font
                .variations()
                .0
                .iter()
                .map(|(tag, v)| (*tag, v.0))
                .collect(),
            glyphs: BTreeSet::new(),
            chars: BTreeMap::new(),
        });
        for (glyph, chars) in glyph_ids {
            acc.glyphs.insert(glyph);
            for c in chars {
                acc.chars.insert(c, glyph);
            }
        }
        slug
    }

    /// 纯聚合入口：按 slug 记一条 char→glyph（字体数据由 `note` 路径补齐）。
    pub fn note_char(&mut self, slug: &str, c: char, glyph: u16) {
        let acc = self.face_acc(slug, 0);
        acc.glyphs.insert(glyph);
        acc.chars.insert(c, glyph);
    }

    /// 某 slug 已收集的 char→glyph 映射，测试观察用。
    pub fn char_map(&self, slug: &str) -> BTreeMap<char, u16> {
        self.faces.get(slug).map(|a| a.chars.clone()).unwrap_or_default()
    }

    fn face_acc(&mut self, slug: &str, index: u32) -> &mut FaceAcc {
        self.faces.entry(slug.to_string()).or_insert_with(|| FaceAcc {
            index,
            data: None,
            variations: Vec::new(),
            glyphs: BTreeSet::new(),
            chars: BTreeMap::new(),
        })
    }

    /// 纯聚合入口：只按 slug 收 glyph（字体数据由 `note` 路径补齐）。
    pub fn note_slug(&mut self, slug: &str, index: u32, glyph: u16) {
        self.face_acc(slug, index).glyphs.insert(glyph);
    }

    /// 合并并行分支的局部收集器（同 slug 并 glyph 集）。
    pub fn merge(&mut self, other: &FaceCollector) {
        for (slug, acc) in &other.faces {
            let mine = self.face_acc(slug, acc.index);
            mine.data = mine.data.take().or_else(|| acc.data.clone());
            mine.variations = acc.variations.clone();
            mine.glyphs.extend(acc.glyphs.iter().copied());
            for (c, g) in &acc.chars {
                mine.chars.insert(*c, *g);
            }
        }
    }

    /// 各 slug 当前聚合的 glyph id（升序），测试观察用。
    pub fn slug_glyphs(&self) -> BTreeMap<String, Vec<u16>> {
        self.faces
            .iter()
            .map(|(k, v)| (k.clone(), v.glyphs.iter().copied().collect()))
            .collect()
    }

    /// 按实际用到的 glyph 子集化每个字体；无数据或子集化失败的字体跳过
    /// （对应 span 由浏览器回退系统字体，行为退回旧状）。
    pub fn finish(self) -> Vec<FontFace> {
        self.faces
            .into_iter()
            .filter_map(|(slug, acc)| {
                let data = acc.data.as_ref()?.as_slice();
                let glyphs: Vec<u16> = acc.glyphs.into_iter().collect();
                let mapper = GlyphRemapper::new_from_glyphs_sorted(&glyphs);
                let variations: Vec<(subsetter::Tag, f32)> = acc
                    .variations
                    .iter()
                    .map(|(tag, v)| (subsetter::Tag::new(&tag.to_bytes()), *v))
                    .collect();
                let subset =
                    subset_with_variations(data, acc.index, &variations, &mapper).ok()?;
                // cmap 的 char→gid 必须换算到子集新 id，否则 gid 越界 OTS 拒载
                let cmap = build_cmap(&remap_char_map(&acc.chars, &mapper));
                let patched =
                    patch_os2_table(&subset, data, acc.index, Some(cmap)).unwrap_or(subset);
                Some(FontFace { family: slug, data_b64: BASE64.encode(patched) })
            })
            .collect()
    }
}


/// 读 sfnt（含 ttc 集合的第 `index` 个 face）里的原始表数据。
fn sfnt_table(data: &[u8], index: u32, want: &[u8; 4]) -> Option<Vec<u8>> {
    sfnt_tables_at(data, index)
        .into_iter()
        .flatten()
        .find(|(tag, _)| tag == want)
        .map(|(_, d)| d)
}

/// 解析 sfnt（含 ttc 第 `index` 个 face）的全部表。
fn sfnt_tables(data: &[u8]) -> Option<Vec<([u8; 4], Vec<u8>)>> {
    sfnt_tables_at(data, 0)
}

fn sfnt_tables_at(data: &[u8], index: u32) -> Option<Vec<([u8; 4], Vec<u8>)>> {
    let face: &[u8] = if data.get(0..4)? == b"ttcf" {
        let n = u32::from_be_bytes(data.get(8..12)?.try_into().ok()?) as usize;
        let i = index as usize;
        if i >= n {
            return None;
        }
        let at = 12 + 4 * i;
        let off = u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize;
        data.get(off..)?
    } else {
        data
    };
    let num = u16::from_be_bytes(face.get(4..6)?.try_into().ok()?) as usize;
    let mut tables = Vec::with_capacity(num);
    for k in 0..num {
        let at = 12 + 16 * k;
        let rec = face.get(at..at + 16)?;
        let tag: [u8; 4] = rec[0..4].try_into().ok()?;
        let off = u32::from_be_bytes(rec[8..12].try_into().ok()?) as usize;
        let len = u32::from_be_bytes(rec[12..16].try_into().ok()?) as usize;
        tables.push((tag, face.get(off..off + len)?.to_vec()));
    }
    Some(tables)
}

/// 重写规范 sfnt：表目录按 tag 升序、数据 4 字节对齐；表 checksum 与
/// head.checkSumAdjustment 置 0（OTS 不校验校验和，只校验结构）。
fn rewrite_sfnt(version: u32, mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_by_key(|(tag, _)| *tag);
    let n = tables.len() as u16;
    let entry_selector = if n > 0 { 15 - (n.leading_zeros() as u16) } else { 0 };
    let search_range = 16u16 << entry_selector.min(7);
    let mut out = Vec::with_capacity(12 + 16 * n as usize + tables.iter().map(|(_, d)| d.len() + 3).sum::<usize>());
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&(16 * n - search_range).to_be_bytes());
    let mut offset = 12 + 16 * n as usize;
    for (tag, data) in &tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += (data.len() + 3) & !3;
    }
    for (_, data) in &tables {
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    out
}

/// 给子集字体补 OTS/渲染必需表：OS/2（OTS 强制，subsetter 为 PDF 嵌入
/// 而省略）与 cmap（subsetter 一律移除，浏览器要靠它从 unicode 定位
/// 字形）。`cmap12` 来自收集到的 char→glyph 映射；原字体里已有 OS/2 的
/// 情况下优先拷原表，cmap 始终用重建的（subsetter 的必然缺失）。
/// 已含 OS/2 且传入 cmap 为空时原样返回。
fn patch_os2_table(
    subset: &[u8],
    original: &[u8],
    index: u32,
    cmap12: Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    let mut tables = sfnt_tables(subset)?;
    let has_os2 = tables.iter().any(|(t, _)| t == b"OS/2");
    if has_os2 && cmap12.is_none() {
        return Some(subset.to_vec());
    }
    if !has_os2 {
        let os2 = sfnt_table(original, index, b"OS/2")?;
        tables.push((*b"OS/2", os2));
    }
    if let Some(cmap) = cmap12.filter(|_| !tables.iter().any(|(t, _)| t == b"cmap")) {
        tables.push((*b"cmap", cmap));
    }
    let version = u32::from_be_bytes(subset.get(0..4)?.try_into().ok()?);
    Some(rewrite_sfnt(version, tables))
}

/// 相邻字符且 glyph 同样连号的合并段：(start_char, end_char, start_glyph)。
fn merged_runs(chars: &BTreeMap<char, u16>) -> Vec<(u32, u32, u32)> {
    let mut runs: Vec<(u32, u32, u32)> = Vec::new();
    for (c, g) in chars {
        let (c, g) = (*c as u32, *g as u32);
        match runs.last_mut() {
            Some((sc, ec, sg)) if *ec + 1 == c && *sg + (*ec - *sc) + 1 == g => *ec = c,
            _ => runs.push((c, c, g)),
        }
    }
    runs
}

/// format 12 子表：全 Unicode 域的 (charCode, endCharCode, startGlyphID) 组。
fn build_format12(runs: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + 12 * runs.len());
    out.extend_from_slice(&12u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // reserved
    out.extend_from_slice(&(16 + 12 * runs.len() as u32).to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes()); // language
    out.extend_from_slice(&(runs.len() as u32).to_be_bytes());
    for (sc, ec, sg) in runs {
        out.extend_from_slice(&sc.to_be_bytes());
        out.extend_from_slice(&ec.to_be_bytes());
        out.extend_from_slice(&sg.to_be_bytes());
    }
    out
}

/// format 4 子表：u16 段表，查字走 idDelta（glyph = char + delta）；
/// 段尾 0xFFFF 哨兵（idDelta=1）把 0xFFFF 非字符兜到 .notdef。
fn build_format4(runs: &[(u32, u32, u32)]) -> Vec<u8> {
    let seg_count = runs.len() + 1;
    let es = (seg_count as u32).ilog2() as u16;
    let search_range = 2u16 << es;
    let seg_x2 = seg_count as u16 * 2;
    let len4 = 16u16 + 8 * seg_count as u16;
    let mut out = Vec::with_capacity(len4 as usize);
    out.extend_from_slice(&4u16.to_be_bytes());
    out.extend_from_slice(&len4.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // language
    out.extend_from_slice(&seg_x2.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&es.to_be_bytes());
    out.extend_from_slice(&(seg_x2 - search_range).to_be_bytes());
    for (_, ec, _) in runs {
        out.extend_from_slice(&(*ec as u16).to_be_bytes());
    }
    out.extend_from_slice(&0xFFFFu16.to_be_bytes()); // 哨兵 endCode
    out.extend_from_slice(&0u16.to_be_bytes()); // reservedPad
    for (sc, _, _) in runs {
        out.extend_from_slice(&(*sc as u16).to_be_bytes());
    }
    out.extend_from_slice(&0xFFFFu16.to_be_bytes()); // 哨兵 startCode
    for (sc, _, sg) in runs {
        let delta = (*sg as i64 - *sc as i64).rem_euclid(0x1_0000) as u16;
        out.extend_from_slice(&delta.to_be_bytes());
    }
    out.extend_from_slice(&1u16.to_be_bytes()); // 哨兵 idDelta
    for _ in runs {
        out.extend_from_slice(&0u16.to_be_bytes()); // idRangeOffset=0
    }
    out.extend_from_slice(&0u16.to_be_bytes());
    out
}

/// 构造完整 cmap：两条平台记录 —— (3,1) format 4 覆盖 BMP、(3,10)
/// format 12 覆盖全部字符（含增补平面）。OTS 强制要求存在 BMP 面向
/// 子表，孤 (3,10) 会被拒载；老渲染器也只认 format 4。
fn build_cmap(chars: &BTreeMap<char, u16>) -> Vec<u8> {
    let f12 = build_format12(&merged_runs(chars));
    let bmp: BTreeMap<char, u16> = chars
        .iter()
        .filter(|(c, _)| (**c as u32) <= 0xFFFE)
        .map(|(c, g)| (*c, *g))
        .collect();
    let f4 = build_format4(&merged_runs(&bmp));
    let mut out = Vec::with_capacity(20 + f4.len() + f12.len());
    out.extend_from_slice(&0u16.to_be_bytes()); // version
    out.extend_from_slice(&2u16.to_be_bytes()); // numTables
    for (enc, off) in [(1u16, 20u32), (10u16, 20 + f4.len() as u32)] {
        out.extend_from_slice(&3u16.to_be_bytes());
        out.extend_from_slice(&enc.to_be_bytes());
        out.extend_from_slice(&off.to_be_bytes());
    }
    out.extend_from_slice(&f4);
    out.extend_from_slice(&f12);
    out
}

/// 把 char→原始 glyph 映射换算到子集字体的新 glyph id（与 subsetter
/// GlyphRemapper 同一语义：排序后取秩）；未进入子集的字符丢弃。
fn remap_char_map(chars: &BTreeMap<char, u16>, mapper: &GlyphRemapper) -> BTreeMap<char, u16> {
    chars
        .iter()
        .filter_map(|(c, g)| mapper.get(*g).map(|n| (*c, n)))
        .collect()
}

/// CSS @font-face family 名：`tl-` 前缀 + 非 ASCII 字母数字字符清洗为 `-`
/// + 数据指纹后缀（防同名不同数据的字体互相覆盖）。
fn font_slug(ps_name: &str, data_fingerprint: u64) -> String {
    let clean: String = ps_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("tl-{clean}-{data_fingerprint:04x}")
}

/// 提取一帧的文本层项，同时收集命中的运行时字体 glyph。
pub fn extract_frame_text(frame: &Frame) -> (Vec<TextLayerItem>, FaceCollector) {
    let mut items = Vec::new();
    let mut faces = FaceCollector::default();
    collect_text_items(frame, Point::zero(), &mut faces, &mut items);
    (items, faces)
}

fn collect_text_items(
    frame: &Frame,
    offset: Point,
    faces: &mut FaceCollector,
    items: &mut Vec<TextLayerItem>,
) {
    for (pos, item) in frame.items() {
        let abs_pos = offset + *pos;
        if let FrameItem::Group(group) = item {
            collect_text_items(&group.frame, abs_pos, faces, items);
        } else if let FrameItem::Text(text) = item {
            try_push_text_item(text, abs_pos, faces, items);
        }
    }
}

fn try_push_text_item(
    text: &TextItem,
    pos: Point,
    faces: &mut FaceCollector,
    items: &mut Vec<TextLayerItem>,
) {
    let size_pt = text.size.to_pt();
    if !text.text.is_empty() && !text.glyphs.is_empty() {
        let slug = faces.note(
            &text.font,
            text.glyphs
                .iter()
                .map(|g| (g.id, text.text[g.range()].chars().collect::<Vec<char>>())),
        );
        let clusters: Vec<(String, f64, f64, f64)> = text
            .glyphs
            .iter()
            .map(|g| {
                (
                    text.text[g.range()].to_string(),
                    g.x_offset.at(text.size).to_pt(),
                    g.y_offset.at(text.size).to_pt(),
                    g.x_advance.at(text.size).to_pt(),
                )
            })
            .collect();
        items.extend(split_item_clusters(
            &clusters,
            pos.x.to_pt(),
            pos.y.to_pt(),
            size_pt,
            &slug,
        ));
    }
}

/// cluster 序列（文本，x_offset pt，y_offset pt，x_advance pt）→ 逐 cluster
/// 独立的文字层项。
///
/// 每个 span 的起点由 data-x 钉死在 typst 精确位置，字符宽度差不跨字符累积；
/// glyph.y_offset 并入基线（svg 渲染同式 `y + y_offset`，数学上下标依赖它）；
/// 连字 cluster（多字符）在 typst 中本就是单 glyph 单位置，整体一个 span。
/// 零宽/零进齐（advance ≤ 0 或空文本）的 cluster 丢弃但保持游标推进。
fn split_item_clusters(
    clusters: &[(String, f64, f64, f64)],
    origin_x_pt: f64,
    y_pt: f64,
    size_pt: f64,
    font: &str,
) -> Vec<TextLayerItem> {
    let mut items = Vec::new();
    let mut cursor = 0.0f64;
    for (text, x_offset, y_offset, advance) in clusters {
        if *advance > 0.0 && !text.is_empty() {
            items.push(TextLayerItem {
                text: text.clone(),
                x: origin_x_pt + cursor + x_offset,
                y: y_pt + y_offset,
                width: *advance,
                font_size: size_pt,
                font: font.to_string(),
            });
        }
        cursor += advance;
    }
    items
}

/// 将文本层渲染为 HTML 字符串，用于叠加在 SVG 之上。
///
/// 每个文本项渲染为绝对定位的透明 `<span>`，
/// 通过 CSS `user-select: text` 支持文字选择。
///
/// 坐标契约：`data-x/y/w/fs` 为页面 pt 原值，浏览器端 `alignTextLayer` 按
/// 「text_layer 容器宽 ÷ SVG viewBox 宽」的系数换算后精修摆位（pdfjs TextLayer
/// 同款：真实 ascent 定 top）。内联 left/top/font-size 仅为精修前的初值
/// （CSS 标准 1pt = 4/3px），避免首帧闪烁；字号/颜色等外观统一在
/// typst_preview.module.scss。`faces` 非空时在最前面输出 @font-face 样式
/// （CSS 文档级生效，只需随第一个 text-page 注入一次）。
pub fn render_text_layer_html(
    items: &[TextLayerItem],
    page_size: Size,
    faces: &[FontFace],
) -> String {
    let w = page_size.x.to_pt();
    let h = page_size.y.to_pt();
    let mut html = String::new();
    if !faces.is_empty() {
        html.push_str("<style>");
        for f in faces {
            html.push_str(&format!(
                "@font-face{{font-family:{};src:url(data:font/ttf;base64,{}) format(\"truetype\")}}",
                f.family, f.data_b64
            ));
        }
        html.push_str("</style>");
    }
    html.push_str(&format!(
        r#"<div class="text-layer" style="position:relative;width:{:.2}px;height:{:.2}px;pointer-events:none;">"#,
        w * PT_TO_PX,
        h * PT_TO_PX,
    ));

    for item in items {
        html.push_str(&render_span_html(item));
    }

    html.push_str("</div>");
    html
}

/// CSS 标准换算：1pt = 4/3px。text_layer 容器宽即页面 pt × 此系数的 px 空间。
const PT_TO_PX: f64 = 4.0 / 3.0;

fn render_span_html(item: &TextLayerItem) -> String {
    let escaped = html_escape(&item.text);
    format!(
        r#"<span style="position:absolute;left:{:.2}px;top:{:.2}px;font-size:{:.2}px;font-family:{},sans-serif;" data-x="{:.2}" data-y="{:.2}" data-w="{:.2}" data-fs="{:.2}">{escaped}</span>"#,
        item.x * PT_TO_PX,
        (item.y - item.font_size) * PT_TO_PX,
        item.font_size * PT_TO_PX,
        item.font,
        item.x,
        item.y,
        item.width,
        item.font_size,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst_library::layout::Abs;

    fn sample() -> TextLayerItem {
        TextLayerItem {
            text: "中文 English".to_string(),
            x: 56.7,
            y: 52.32,
            width: 100.0,
            font_size: 12.0,
            font: "tl-LibertinusSerif-Regular".to_string(),
        }
    }

    #[test]
    fn span_uses_px_with_pt_data() {
        let html = render_span_html(&sample());
        // 内联初值 = pt × 4/3（CSS 标准），精修由浏览器端按容器/viewBox 系数覆盖
        assert!(html.contains("left:75.60px"), "{html}");
        assert!(html.contains("top:53.76px"), "{html}");
        assert!(html.contains("font-size:16.00px"), "{html}");
        assert!(html.contains("font-family:tl-LibertinusSerif-Regular,sans-serif;"), "{html}");
        // data 属性保持页面 pt 原值，供浏览器端 measureText 摆位（pdfjs TextLayer 同款）
        assert!(html.contains(r#"data-x="56.70""#), "{html}");
        assert!(html.contains(r#"data-y="52.32""#), "{html}");
        assert!(html.contains(r#"data-w="100.00""#), "{html}");
        assert!(html.contains(r#"data-fs="12.00""#), "{html}");
        assert!(!html.contains("pt;"), "{html}");
    }

    #[test]
    fn text_layer_container_uses_px() {
        let page = Size::splat(Abs::pt(595.28));
        let html = render_text_layer_html(&[sample()], page, &[]);
        assert!(html.contains(r#"width:793.71px;height:793.71px"#), "{html}");
        assert!(!html.contains("pt;"), "{html}");
    }

    #[test]
    fn span_escapes_html() {
        let mut item = sample();
        item.text = r#"a<b>&"c"#.to_string();
        let html = render_span_html(&item);
        assert!(html.contains("a&lt;b&gt;&amp;&quot;c"), "{html}");
    }

    // ── cluster 拆分契约 ──
    //
    // Typst 的 TextItem 粒度是整句，Web 端 sans-serif 与 typst 字体的标点/
    // 字偶/数字 shaping 差（约 2.5%，数字串可达 17%）在 scaleX 均匀压缩下
    // 变成行中/词中段的鼓形错位。契约：全部 cluster 逐字独立钉位——每个
    // span 的起点被自身 data-x 钉死，误差不再累积；连字 cluster（多字符）
    // 作为一个整体（typst 中本就是单 glyph 单位置）。

    fn clusters(texts: &[&str], advance: f64) -> Vec<(String, f64, f64, f64)> {
        texts.iter().map(|t| (t.to_string(), 0.0, 0.0, advance)).collect()
    }

    #[test]
    fn cjk_and_punct_split_per_char() {
        let c = clusters(&["末", "页", "：", "逐"], 12.0);
        let items = split_item_clusters(&c, 0.0, 87.0, 12.0, "tl-F");
        assert_eq!(
            items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
            vec!["末", "页", "：", "逐"]
        );
    }

    #[test]
    fn latin_chars_pinned_per_char() {
        let c = clusters(&["E", "n", "g", "l", "i", "s", "h"], 7.0);
        let items = split_item_clusters(&c, 56.7, 87.0, 12.0, "tl-F");
        assert_eq!(items.len(), 7);
        for (idx, item) in items.iter().enumerate() {
            assert_eq!(item.x, 56.7 + idx as f64 * 7.0);
            assert_eq!(item.width, 7.0);
        }
    }

    #[test]
    fn ligature_cluster_is_one_span() {
        // 连字 cluster（多字符如 "fi"）在 typst 中是单 glyph 单位置，整体一个 span
        let c = clusters(&["fi", "x"], 7.0);
        let items = split_item_clusters(&c, 0.0, 0.0, 12.0, "tl-F");
        assert_eq!(
            items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
            vec!["fi", "x"]
        );
    }

    #[test]
    fn split_positions_are_cumulative_advances() {
        let mut c = clusters(&["末", "页", "：", "A", "B"], 12.0);
        c[2] = ("：".to_string(), 0.5, 0.0, 6.0); // 带x_offset的窄冒号
        let items = split_item_clusters(&c, 56.7, 87.0, 12.0, "tl-F");
        let by_text = |t: &str| items.iter().find(|i| i.text == t).unwrap();
        let mo = by_text("末");
        assert_eq!(mo.x, 56.7);
        assert_eq!(mo.width, 12.0);
        assert_eq!(mo.y, 87.0);
        assert_eq!(mo.font_size, 12.0);
        let ye = by_text("页");
        assert_eq!(ye.x, 68.7);
        let colon = by_text("：");
        assert_eq!(colon.x, 56.7 + 24.0 + 0.5);
        assert_eq!(colon.width, 6.0);
        let a = by_text("A");
        assert_eq!(a.x, 56.7 + 30.0);
        let b = by_text("B");
        assert_eq!(b.x, 56.7 + 42.0);
        assert_eq!(items.len(), 5);
    }

    #[test]
    fn split_drops_zero_advance_clusters() {
        let c = vec![
            ("a".to_string(), 0.0, 0.0, 7.0),
            ("\u{200b}".to_string(), 0.0, 0.0, 0.0), // 零宽不可见
            ("b".to_string(), 0.0, 0.0, 7.0),
        ];
        let items = split_item_clusters(&c, 0.0, 0.0, 12.0, "tl-F");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].x, 7.0);
    }

    // ── 运行时字体契约 ──
    //
    // svg 层是 typst 实际命中的字体（TextItem.font）转的 path，文字层若用
    // 系统字体渲染，字形 ink 永远差零点几像素（数学斜体的起笔/斜度、全角
    // 字形的 cap 高度），选区高亮还跟着 fallback 字体框走。契约：文字层
    // span 内联 font-family 指向运行时字体按 glyph 子集化的 @font-face，
    // 两层同字体后形状与选区一并归齐；glyph.y_offset 同步进基线。

    fn face(family: &str, b64: &str) -> FontFace {
        FontFace { family: family.to_string(), data_b64: b64.to_string() }
    }

    #[test]
    fn span_inlines_runtime_font_family() {
        let mut item = sample();
        item.font = "tl-NewCMMath-Book".to_string();
        let html = render_span_html(&item);
        assert!(html.contains("font-family:tl-NewCMMath-Book,sans-serif;"), "{html}");
    }

    #[test]
    fn text_layer_emits_font_face_style() {
        let page = Size::splat(Abs::pt(595.28));
        let faces = [face("tl-A", "QUJD")];
        let html = render_text_layer_html(&[sample()], page, &faces);
        assert!(
            html.contains(
                "@font-face{font-family:tl-A;src:url(data:font/ttf;base64,QUJD) format(\"truetype\")}"
            ),
            "{html}"
        );
        assert_eq!(html.matches("@font-face").count(), 1);
    }

    #[test]
    fn text_layer_without_faces_has_no_font_style() {
        let page = Size::splat(Abs::pt(595.28));
        let html = render_text_layer_html(&[sample()], page, &[]);
        assert!(!html.contains("@font-face"), "{html}");
    }

    #[test]
    fn font_slug_sanitizes_and_hashes() {
        let s = font_slug("New CM Math Book+Bold", 0xdeadbeefu64);
        assert!(s.starts_with("tl-New-CM-Math-Book-Bold-"), "{s}");
        assert!(
            s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "{s}"
        );
        assert_eq!(font_slug("New CM Math Book+Bold", 0xdeadbeefu64), s);
    }

    #[test]
    fn cluster_y_offset_shifts_baseline() {
        let c = vec![
            ("a".to_string(), 0.0, 0.0, 7.0),
            ("b".to_string(), 0.0, 3.0, 7.0), // y_offset 3pt
        ];
        let items = split_item_clusters(&c, 10.0, 87.0, 12.0, "tl-F");
        assert_eq!(items[0].y, 87.0);
        assert_eq!(items[1].y, 90.0);
    }

    #[test]
    fn face_collector_merges_glyphs_by_slug() {
        let mut fc = FaceCollector::default();
        fc.note_slug("tl-A", 0, 5);
        fc.note_slug("tl-A", 0, 7);
        fc.note_slug("tl-A", 0, 5); // 重复
        fc.note_slug("tl-B", 1, 9);
        assert_eq!(fc.slug_glyphs()["tl-A"], vec![5, 7]);
        assert_eq!(fc.slug_glyphs()["tl-B"], vec![9]);
    }

    // ── OS/2 表补丁契约 ──
    //
    // subsetter 为 PDF 嵌入而生，输出的 sfnt 不含 OS/2 表（PDF 不需要），
    // 但 WebView 的 OTS 强制要求（console: `OTS parsing error: OS/2:
    // missing required table`，@font-face 全部 error、文字层退回系统字体）。
    // 契约：从原字体拷贝 OS/2 原始字节，与子集字体的全部表重写为规范
    // sfnt（表目录 tag 升序、数据 4 字节对齐）；已含 OS/2 的子集原样返回。

    #[test]
    fn os2_patch_adds_missing_table() {
        let orig = rewrite_sfnt(
            0x0001_0000,
            vec![
                (*b"OS/2", vec![1, 2, 3, 4]),
                (*b"head", vec![0xAA; 8]),
                (*b"glyf", vec![0xBB; 6]),
            ],
        );
        let subset = rewrite_sfnt(
            0x0001_0000,
            vec![(*b"head", vec![0xAA; 8]), (*b"glyf", vec![0xBB; 6])],
        );
        let patched = patch_os2_table(&subset, &orig, 0, None).unwrap();
        let tables = sfnt_tables(&patched).unwrap();
        assert!(
            tables.iter().any(|(t, d)| t == b"OS/2" && d == &vec![1, 2, 3, 4]),
            "{tables:?}"
        );
        assert!(tables.iter().any(|(t, d)| t == b"glyf" && d == &vec![0xBB; 6]));
        let mut tags: Vec<_> = tables.iter().map(|(t, _)| *t).collect();
        tags.sort();
        assert_eq!(tags, tables.iter().map(|(t, _)| *t).collect::<Vec<_>>());
    }

    #[test]
    fn os2_patch_keeps_existing_table() {
        let subset = rewrite_sfnt(0x0001_0000, vec![(*b"OS/2", vec![9]), (*b"head", vec![1; 4])]);
        let orig = rewrite_sfnt(0x0001_0000, vec![(*b"OS/2", vec![1, 2, 3, 4])]);
        assert_eq!(patch_os2_table(&subset, &orig, 0, None).unwrap(), subset);
    }

    #[test]
    fn sfnt_roundtrip_preserves_version_and_data() {
        // OTTO（CFF）version 也要原样保留
        let tables = vec![(*b"CFF ", vec![7; 5]), (*b"OS/2", vec![1; 4])];
        let sfnt = rewrite_sfnt(u32::from_be_bytes(*b"OTTO"), tables.clone());
        let parsed = sfnt_tables(&sfnt).unwrap();
        assert_eq!(parsed, tables);
    }

    // ── cmap 重建契约 ──
    //
    // subsetter 同时移除 cmap（PDF 按 glyph id 直映，不需要）；浏览器没有
    // cmap 就无法从 unicode 定位字形。契约：收集 char→glyph id 映射，
    // finish 时构造 cmap format 12（平台 3/10）一并写入。

    #[test]
    fn face_collector_maps_chars_to_glyphs() {
        let mut fc = FaceCollector::default();
        fc.note_char("tl-A", 'α', 42);
        fc.note_char("tl-A", 'β', 43);
        fc.note_char("tl-A", 'α', 42); // 重复
        fc.note_char("tl-B", 'γ', 7);
        assert_eq!(fc.char_map("tl-A"), BTreeMap::from([('α', 42), ('β', 43)]));
        assert_eq!(fc.char_map("tl-B"), BTreeMap::from([('γ', 7)]));
    }

    #[test]
    fn cmap_has_bmp_and_ucs4_subtables() {
        let cmap = build_cmap(&BTreeMap::from([('a', 5u16)]));
        let u16at = |p: usize| u16::from_be_bytes(cmap[p..p + 2].try_into().unwrap());
        let u32at = |p: usize| u32::from_be_bytes(cmap[p..p + 4].try_into().unwrap());
        assert_eq!(u16at(0), 0); // version
        assert_eq!(u16at(2), 2); // numTables
        // Windows BMP 记录在前（OTS 强制要求存在 BMP 面向子表）
        assert_eq!((u16at(4), u16at(6)), (3, 1));
        assert_eq!(u32at(8), 20); // 4B 头 + 2×8B 记录
        assert_eq!((u16at(12), u16at(14)), (3, 10));
        let f4 = u32at(8) as usize;
        assert_eq!(f4, 20);
        assert_eq!(u16at(f4), 4);
        let f4_len = u16at(f4 + 2) as usize;
        assert_eq!(u32at(16), (f4 + f4_len) as u32); // 记录 2 指向 f12
        let f12 = f4 + f4_len;
        assert_eq!(u16at(f12), 12);
        assert_eq!(u16at(f12 + 2), 0); // reserved
        assert_eq!(u32at(f12 + 4), 16 + 12); // 单组长度
        assert_eq!(u32at(f12 + 12), 1); // numGroups
    }

    #[test]
    fn cmap_format4_resolves_chars_via_delta() {
        let cmap = build_cmap(&BTreeMap::from([('a', 5u16), ('b', 6), ('x', 9)]));
        let u16at = |p: usize| u16::from_be_bytes(cmap[p..p + 2].try_into().unwrap());
        let f4 = 20;
        assert_eq!(u16at(f4 + 6), 6); // segCountX2 = 2 个实段 + 哨兵
        let seg = (u16at(f4 + 6) / 2) as usize;
        let ends = f4 + 14;
        assert_eq!(u16at(ends), 'b' as u16); // endCode[0]
        assert_eq!(u16at(ends + 2), 'x' as u16);
        assert_eq!(u16at(ends + 2 * (seg - 1)), 0xFFFF); // 哨兵 endCode
        let starts = ends + 2 * seg + 2; // 越过 reservedPad
        assert_eq!(u16at(starts), 'a' as u16);
        assert_eq!(u16at(starts + 2 * (seg - 1)), 0xFFFF); // 哨兵 startCode
        let deltas = starts + 2 * seg;
        // 查字算法：glyph = (char + idDelta) & 0xFFFF
        assert_eq!(u16at(deltas).wrapping_add('a' as u16), 5);
        assert_eq!(u16at(deltas).wrapping_add('b' as u16), 6);
        assert_eq!(u16at(deltas + 2).wrapping_add('x' as u16), 9);
        assert_eq!(u16at(deltas + 2 * (seg - 1)), 1); // 哨兵 idDelta=1 → .notdef
        let ranges = deltas + 2 * seg;
        assert_eq!(u16at(ranges), 0); // idRangeOffset 全 0（走 delta 查找）
        assert_eq!(u16at(ranges + 2 * (seg - 1)), 0);
    }

    #[test]
    fn cmap_format4_skips_non_bmp_chars() {
        let cmap = build_cmap(&BTreeMap::from([('a', 5u16), ('\u{1D45B}', 9)]));
        let u16at = |p: usize| u16::from_be_bytes(cmap[p..p + 2].try_into().unwrap());
        // f4 只含 BMP 段：'a' 一段 + 哨兵 = 2 段 → 16+16=32 字节
        assert_eq!(u16at(20 + 2), 32);
        let f12 = 20 + 32;
        // f12 收全部字符（含增补平面）
        let groups = u32::from_be_bytes(cmap[f12 + 12..f12 + 16].try_into().unwrap());
        assert_eq!(groups, 2);
    }

    #[test]
    fn cmap_format12_merges_consecutive_runs() {
        let map = BTreeMap::from([('a', 5), ('b', 6), ('c', 7), ('x', 20)]);
        let cmap = build_cmap(&map);
        let f4_len = u16::from_be_bytes(cmap[20 + 2..20 + 4].try_into().unwrap()) as usize;
        let f12 = 20 + f4_len;
        let groups = u32::from_be_bytes(cmap[f12 + 12..f12 + 16].try_into().unwrap());
        assert_eq!(groups, 2); // a-c 连号合并、x 独立
        let g0 = &cmap[f12 + 16..f12 + 28];
        assert_eq!(u32::from_be_bytes(g0[0..4].try_into().unwrap()), 'a' as u32);
        assert_eq!(u32::from_be_bytes(g0[4..8].try_into().unwrap()), 'c' as u32);
        assert_eq!(u32::from_be_bytes(g0[8..12].try_into().unwrap()), 5);
        let g1 = &cmap[f12 + 28..f12 + 40];
        assert_eq!(u32::from_be_bytes(g1[0..4].try_into().unwrap()), 'x' as u32);
        assert_eq!(u32::from_be_bytes(g1[8..12].try_into().unwrap()), 20);
    }

    #[test]
    fn remap_char_map_translates_to_subset_ids() {
        // sorted [3,7,42]：.notdef 占 0 → 3→1、7→2、42→3
        let mapper = GlyphRemapper::new_from_glyphs_sorted(&[7, 3, 42]);
        let chars = BTreeMap::from([('α', 7u16), ('β', 3)]);
        assert_eq!(remap_char_map(&chars, &mapper), BTreeMap::from([('α', 2), ('β', 1)]));
    }

    #[test]
    fn os2_patch_also_writes_cmap() {
        // 子集缺 cmap 时：从收集到的 char→gid 构造并写入
        let subset = rewrite_sfnt(0x0001_0000, vec![(*b"head", vec![1; 4])]);
        let orig = rewrite_sfnt(0x0001_0000, vec![(*b"OS/2", vec![1; 4]), (*b"cmap", vec![9; 4])]);
        let cmap12 = build_cmap(&BTreeMap::from([('a', 3)]));
        let patched = patch_os2_table(&subset, &orig, 0, Some(cmap12)).unwrap();
        let tables = sfnt_tables(&patched).unwrap();
        assert!(tables.iter().any(|(t, _)| t == b"OS/2"));
        assert!(tables.iter().any(|(t, _)| t == b"cmap"));
    }
}
