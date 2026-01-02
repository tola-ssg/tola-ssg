# TTG 任务分解 v3

> 逐文件细化，每任务 ≤2h，包含 LOC 估算和验收标准
>
> ⚠️ **策略：激进重构** — 从一开始就确保正确性、高性能、完美架构

## 总览

| 阶段 | 任务数 | 总估时 | 目标 |
|------|--------|--------|------|
| Phase 0: 基础设施 | 2 | 1h | 依赖 + 模块骨架 |
| Phase 1: VDOM 核心 | 8 | 12h | 类型系统 + 转换 |
| Phase 2: Transform | 8 | 14h | Pipeline 实现 |
| Phase 3: 切换与删除 | 4 | 8h | 替换旧代码 |
| Phase 4: 热更新优化 | 3 | 6h | WebSocket + Diff |
| **总计** | **25** | **41h** | - |

---

## 重构原则

```
1. 无渐进迁移 — 直接替换，不做并行路径
2. 零解析开销 — 从 HtmlDocument 直接转换，跳过 typst_html::html()
3. 类型安全优先 — 编译期保证阶段转换正确性
4. 删除胜于兼容 — 宁可删旧代码，不留兼容层
```

---


## Phase 0: 基础设施 [1h]

### Task 0.1: 添加依赖 [15min]
**文件**: `Cargo.toml`

```toml
compact_str = "0.8"
smallvec = { version = "1.13", features = ["union", "const_generics"] }
```

**验收**: `cargo check` 通过

---

### Task 0.2: 创建模块骨架 [45min]
**文件**: `src/vdom/mod.rs` + 7 个空文件

```rust
// src/vdom/mod.rs
pub mod attr;
pub mod convert;
pub mod family;
pub mod folder;
pub mod node;
pub mod phase;
pub mod visitor;

pub use attr::*;
pub use family::*;
pub use node::*;
pub use phase::*;
```

**验收**: `cargo check` 通过，所有文件 `todo!()` 编译

---

## Phase 1: vdom 核心类型 [12h]

### Task 1.1: family.rs - TagFamily trait [1.5h]
**文件**: `src/vdom/family.rs`
**LOC**: ~150

```rust
// 必须实现
pub trait TagFamily: 'static + Send + Sync {
    const NAME: &'static str;
    type Data: Debug + Clone + Default + Send + Sync;
}

// 族定义
pub struct SvgFamily;
pub struct LinkFamily;
pub struct HeadingFamily;
pub struct MediaFamily;
pub struct OtherFamily;

// 族数据
pub struct SvgFamilyData { is_root: bool, viewbox: Option<String>, dimensions: Option<(f32, f32)> }
pub struct LinkFamilyData { link_type: LinkType, original_href: Option<String>, processed: bool }
pub struct HeadingFamilyData { level: u8, original_id: Option<String>, slugified_id: Option<String> }
pub struct MediaFamilyData { src: Option<String>, is_svg_image: bool }

// 族识别
pub fn identify_family(tag: &str, attrs: &Attrs) -> &'static str;
```

**测试**:
```rust
#[test]
fn test_identify_family() {
    assert_eq!(identify_family("svg", &Attrs::empty()), "svg");
    assert_eq!(identify_family("path", &Attrs::empty()), "svg");
    assert_eq!(identify_family("a", &Attrs::empty()), "link");
    let attrs = Attrs::from([("href", "/")]);
    assert_eq!(identify_family("div", &attrs), "link");
}
```

**验收**: 所有族 trait impl 正确，identify_family 覆盖所有 SVG 标签

---

### Task 1.2: attr.rs - 属性系统 [1.5h]
**文件**: `src/vdom/attr.rs`
**LOC**: ~120

```rust
#[derive(Clone, Debug)]
pub struct Attr {
    pub name: AttrName,
    pub value: AttrValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AttrName {
    Id, Class, Href, Src, Style,
    ViewBox, D, Fill, Stroke,
    DataDarkInvert,
    Other(CompactString),
}

#[derive(Clone, Debug)]
pub enum AttrValue {

    String(CompactString),
    Bool(bool),
}

#[derive(Clone, Debug, Default)]
pub struct Attrs(pub SmallVec<[Attr; 4]>);

impl Attrs {
    pub fn get(&self, name: &str) -> Option<&str>;
    pub fn set(&mut self, name: impl Into<AttrName>, value: impl Into<AttrValue>);
    pub fn has(&self, name: &str) -> bool;
    pub fn has_any(&self, names: &[&str]) -> bool;
    pub fn remove(&mut self, name: &str) -> Option<AttrValue>;
    pub fn iter(&self) -> impl Iterator<Item = &Attr>;
}
```

**测试**:
```rust
#[test]
fn test_attrs_crud() {
    let mut attrs = Attrs::from([("id", "test")]);
    assert_eq!(attrs.get("id"), Some("test"));
    attrs.set("class", "foo");
    assert!(attrs.has("class"));
    attrs.remove("id");
    assert!(!attrs.has("id"));
}
```

**验收**: Attrs 的增删改查 API 完整，From trait 实现

---

### Task 1.3: phase.rs - Phase trait [1h]
**文件**: `src/vdom/phase.rs`
**LOC**: ~80

```rust
pub trait Phase: 'static + Clone + Send + Sync + Debug {
    const NAME: &'static str;
}

pub trait PhaseData: Phase {
    type DocExt: Debug + Clone + Default + Send + Sync;
    type ElemExt<F: TagFamily>: Debug + Clone + Default + Send + Sync;
    type TextExt: Debug + Clone + Default + Send + Sync;
    type FrameExt: Debug + Clone + Send + Sync;
}
```

**验收**: Phase trait 定义正确，PhaseData 使用 GAT

---

### Task 1.4: phase.rs - Raw 阶段 [1h]
**文件**: `src/vdom/phase.rs`（续）
**LOC**: ~50

```rust
#[derive(Debug, Clone)]
pub struct Raw;

impl Phase for Raw { const NAME: &'static str = "raw"; }

impl PhaseData for Raw {
    type DocExt = RawDocExt;
    type ElemExt<F: TagFamily> = ();
    type TextExt = ();
    type FrameExt = RawFrameExt;
}

#[derive(Debug, Clone, Default)]
pub struct RawDocExt {
    pub source_path: PathBuf,
    pub dependencies: Vec<PathBuf>,
    pub content_meta: Option<ContentMeta>,
    pub is_index: bool,
}

#[derive(Clone)]
pub struct RawFrameExt {
    pub frame_ref: Arc<typst::layout::Frame>,
    pub size: (f32, f32),
    pub span: Option<u64>,
}
```

**验收**: Raw 阶段编译通过，RawFrameExt 持有 Arc<Frame>

---

### Task 1.5: phase.rs - Indexed 阶段 [1.5h]
**文件**: `src/vdom/phase.rs`（续）
**LOC**: ~100

```rust
#[derive(Debug, Clone)]
pub struct Indexed;

impl Phase for Indexed { const NAME: &'static str = "indexed"; }

impl PhaseData for Indexed {
    type DocExt = IndexedDocExt;
    type ElemExt<F: TagFamily> = IndexedElemExt<F>;
    type TextExt = NodeId;
    type FrameExt = IndexedFrameExt;
}

#[derive(Debug, Clone, Default)]
pub struct IndexedDocExt {
    pub base: RawDocExt,
    pub node_count: u32,
    pub svg_nodes: Vec<NodeId>,
    pub link_nodes: Vec<NodeId>,
    pub heading_nodes: Vec<NodeId>,
    pub frame_nodes: Vec<NodeId>,
}

pub struct IndexedElemExt<F: TagFamily> {
    pub node_id: NodeId,
    pub family_data: F::Data,
}
// 需要手动实现 Debug, Clone, Default（因为 F 无约束）
```

**验收**: IndexedElemExt 正确使用 GAT 特化

---

### Task 1.6: phase.rs - Processed 阶段 [1h]
**文件**: `src/vdom/phase.rs`（续）
**LOC**: ~60

```rust
#[derive(Debug, Clone)]
pub struct Processed;

impl PhaseData for Processed {
    type DocExt = ProcessedDocExt;
    type ElemExt<F: TagFamily> = ProcessedElemExt;
    type TextExt = ();
    type FrameExt = std::convert::Infallible;  // Frame 已全部转为 Element
}

#[derive(Debug, Clone, Default)]
pub struct ProcessedDocExt {
    pub svg_count: usize,
    pub total_svg_bytes: usize,
    pub frames_expanded: usize,
    pub head_injections: Vec<HeadInjection>,
}
```

**验收**: Processed 阶段 FrameExt = Infallible，表示无 Frame

---

### Task 1.7: node.rs - FamilyExt + Node 类型 [2h]
**文件**: `src/vdom/node.rs`
**LOC**: ~200

```rust
// 零开销族扩展枚举
#[derive(Debug, Clone)]
pub enum FamilyExt<P: PhaseData> {
    Svg(P::ElemExt<SvgFamily>),
    Link(P::ElemExt<LinkFamily>),
    Heading(P::ElemExt<HeadingFamily>),
    Media(P::ElemExt<MediaFamily>),
    Other(P::ElemExt<OtherFamily>),
}

impl<P: PhaseData> FamilyExt<P> {
    pub fn family_name(&self) -> &'static str;
    pub fn is_svg(&self) -> bool;
    pub fn as_svg(&self) -> Option<&P::ElemExt<SvgFamily>>;
    pub fn as_svg_mut(&mut self) -> Option<&mut P::ElemExt<SvgFamily>>;
    // ... 其他族
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone)]
pub enum Node<P: PhaseData> {
    Element(Element<P>),
    Text(Text<P>),
    Frame(Frame<P>),
}

#[derive(Debug, Clone)]
pub struct Element<P: PhaseData> {
    pub tag: CompactString,
    pub attrs: Attrs,
    pub children: Vec<Node<P>>,
    pub span: Option<u64>,
    pub ext: FamilyExt<P>,
}

impl<P: PhaseData> Element<P> {
    pub fn svg(...) -> Self;
    pub fn link(...) -> Self;
    pub fn heading(...) -> Self;
    pub fn media(...) -> Self;
    pub fn other(...) -> Self;
    pub fn auto(tag: &str, attrs: Attrs, children: Vec<Node<P>>) -> Self;
    pub fn family(&self) -> &'static str;
}

#[derive(Debug, Clone)]
pub struct Text<P: PhaseData> { pub content: CompactString, pub span: Option<u64>, pub ext: P::TextExt }

#[derive(Debug, Clone)]
pub struct Frame<P: PhaseData> { pub ext: P::FrameExt }

#[derive(Debug, Clone)]
pub struct Document<P: PhaseData> { pub root: Element<P>, pub ext: P::DocExt }
```

**测试**:
```rust
#[test]
fn test_family_ext_size() {
    assert!(std::mem::size_of::<FamilyExt<Indexed>>() < 128);
}

#[test]
fn test_element_auto() {
    let elem = Element::<Raw>::auto("svg", Attrs::empty(), vec![]);
    assert_eq!(elem.family(), "svg");
}
```

**验收**: FamilyExt 是栈分配（无 Box），Element::auto 正确识别族

---

### Task 1.8: visitor.rs + folder.rs - 遍历 trait [2.5h]
**文件**: `src/vdom/visitor.rs`, `src/vdom/folder.rs`
**LOC**: ~180

```rust
// visitor.rs
pub trait Visitor<P: PhaseData> {
    fn visit_element(&mut self, elem: &Element<P>) -> bool { true }
    fn leave_element(&mut self, elem: &Element<P>) {}
    fn visit_text(&mut self, text: &Text<P>) {}
    fn visit_frame(&mut self, frame: &Frame<P>) {}
}

pub fn visit<P: PhaseData, V: Visitor<P>>(doc: &Document<P>, visitor: &mut V);

// folder.rs
pub trait Folder<From: PhaseData, To: PhaseData> {
    fn fold_doc_ext(&mut self, ext: From::DocExt) -> To::DocExt;
    fn fold_element(&mut self, elem: Element<From>) -> Element<To>;
    fn fold_text(&mut self, text: Text<From>) -> Text<To>;
    fn fold_frame(&mut self, frame: Frame<From>) -> Node<To>;
    fn fold_node(&mut self, node: Node<From>) -> Node<To>;
    fn fold_children(&mut self, children: Vec<Node<From>>) -> Vec<Node<To>>;
}

pub fn fold<From, To, F>(doc: Document<From>, folder: &mut F) -> Document<To>
where F: Folder<From, To>;

// MutVisitor
pub trait MutVisitor<P: PhaseData> {
    fn visit_element_mut(&mut self, elem: &mut Element<P>) -> bool { true }
    fn visit_text_mut(&mut self, text: &mut Text<P>) {}
    fn visit_frame_mut(&mut self, frame: &mut Frame<P>) {}
}

pub fn visit_mut<P: PhaseData, V: MutVisitor<P>>(doc: &mut Document<P>, visitor: &mut V);
```

**测试**:
```rust
struct CountVisitor { count: usize }
impl<P: PhaseData> Visitor<P> for CountVisitor {
    fn visit_element(&mut self, _: &Element<P>) -> bool { self.count += 1; true }
}

#[test]
fn test_visitor() {
    let doc = create_test_doc();
    let mut v = CountVisitor { count: 0 };
    visit(&doc, &mut v);
    assert!(v.count > 0);
}
```

**验收**: Visitor/Folder/MutVisitor 三个 trait 完整实现，消除手写递归

---

## Phase 2: transform 系统 [14h]

### Task 2.1: transform/mod.rs - Transform trait [1h]
**文件**: `src/transform/mod.rs`
**LOC**: ~60

```rust
pub mod frame;
pub mod head;
pub mod heading;
pub mod indexer;
pub mod link;
pub mod pipeline;
pub mod render;

pub trait Transform {
    type From: PhaseData;
    type To: PhaseData;
    fn transform(&self, doc: Document<Self::From>) -> Document<Self::To>;
    fn name(&self) -> &'static str;
}

pub trait InPlaceTransform<P: PhaseData> {
    fn transform(&self, doc: &mut Document<P>);
    fn name(&self) -> &'static str;
}
```

**验收**: Transform trait 定义正确

---

### Task 2.2: transform/indexer.rs - Indexer [2h]
**文件**: `src/transform/indexer.rs`
**LOC**: ~180

```rust
pub struct Indexer;

impl Transform for Indexer {
    type From = Raw;
    type To = Indexed;

    fn transform(&self, doc: Document<Raw>) -> Document<Indexed> {
        let mut folder = IndexerFolder::new(doc.ext.clone());
        fold(doc, &mut folder)
    }
}

struct IndexerFolder {
    next_id: u32,
    doc_ext: IndexedDocExt,
}

impl Folder<Raw, Indexed> for IndexerFolder {
    fn fold_element(&mut self, elem: Element<Raw>) -> Element<Indexed> {
        // 根据 identify_family 分配族
        // 填充族特定数据（viewbox, href, level 等）
    }
    // ...
}
```

**测试**:
```rust
#[test]
fn test_indexer_counts() {
    let doc = create_raw_doc_with_svg_and_links();
    let indexed = Indexer.transform(doc);
    assert_eq!(indexed.ext.svg_nodes.len(), 2);
    assert_eq!(indexed.ext.link_nodes.len(), 1);
}
```

**验收**: Indexer 正确分配 NodeId，填充族数据

---

### Task 2.3: transform/link.rs - LinkProcessor [1.5h]
**文件**: `src/transform/link.rs`
**LOC**: ~100

```rust
pub struct LinkProcessor {
    pub path_prefix: Option<String>,
    pub base_url: Option<String>,
}

impl InPlaceTransform<Indexed> for LinkProcessor {
    fn transform(&self, doc: &mut Document<Indexed>) {
        let mut visitor = LinkProcessorVisitor { ... };
        visit_mut(doc, &mut visitor);
    }
}

struct LinkProcessorVisitor<'a> { ... }

impl MutVisitor<Indexed> for LinkProcessorVisitor<'_> {
    fn visit_element_mut(&mut self, elem: &mut Element<Indexed>) -> bool {
        if let Some(link_ext) = elem.ext.as_link_mut() {
            // 处理 href 属性
        }
        true
    }
}
```

**验收**: 绝对路径被正确添加前缀

---

### Task 2.4: transform/heading.rs - HeadingProcessor [1.5h]
**文件**: `src/transform/heading.rs`
**LOC**: ~100

```rust
pub struct HeadingProcessor;

impl InPlaceTransform<Indexed> for HeadingProcessor {
    fn transform(&self, doc: &mut Document<Indexed>) {
        // 1. 为无 ID 的标题生成 slug
        // 2. 记录到 doc.ext 供 TOC 使用
    }
}
```

**验收**: 标题自动生成唯一 ID

---

### Task 2.5: transform/frame.rs - FrameExpander [2.5h]
**文件**: `src/transform/frame.rs`
**LOC**: ~200

```rust
pub struct FrameExpander;

impl Transform for FrameExpander {
    type From = Indexed;
    type To = Processed;

    fn transform(&self, doc: Document<Indexed>) -> Document<Processed> {
        let mut folder = FrameExpanderFolder::new();
        fold(doc, &mut folder)
    }
}

impl Folder<Indexed, Processed> for FrameExpanderFolder {
    fn fold_frame(&mut self, frame: Frame<Indexed>) -> Node<Processed> {
        // 🔑 关键：Frame → SVG
        let typst_frame = &frame.ext.frame_ref;
        let svg = typst_render::render_svg(typst_frame);
        let optimized = optimize_svg(&svg);
        // 解析为 Element
        Node::Element(parse_svg_element(&optimized))
    }
}
```

**测试**:
```rust
#[test]
fn test_frame_expansion() {
    let doc = create_indexed_doc_with_frames();
    let processed = FrameExpander.transform(doc);
    // 验证无 Frame 节点残留
    assert!(!has_frame_nodes(&processed));
}
```

**验收**: 所有 Frame 转为 SVG Element，typst_render 正确调用

---

### Task 2.6: transform/head.rs - HeadInjector [2h]
**文件**: `src/transform/head.rs`
**LOC**: ~150

```rust
/// ⚠️ Phase 2 only（依赖 GLOBAL_SITE_DATA）
pub struct HeadInjector<'a> {
    pub site_data: &'a SiteData,
    pub page_meta: &'a PageMeta,
}

impl InPlaceTransform<Processed> for HeadInjector<'_> {
    fn transform(&self, doc: &mut Document<Processed>) {
        // 1. 注入 <title>
        // 2. 注入 <meta> (description, og:*, twitter:*)
        // 3. 注入 CSS
        // 4. 注入 RSS/sitemap links
    }
}
```

**验收**: head 正确注入所有 meta 和 CSS

---

### Task 2.7: transform/render.rs - HtmlRenderer [2h]
**文件**: `src/transform/render.rs`
**LOC**: ~150

```rust
pub struct HtmlRenderer {
    pub minify: bool,
}

impl HtmlRenderer {
    pub fn render(&self, doc: &Document<Processed>) -> Vec<u8> {
        let mut output = Vec::with_capacity(64 * 1024);
        render_node(&Node::Element(doc.root.clone()), &mut output);
        if self.minify {
            minify_html(&output)
        } else {
            output
        }
    }
}

fn render_node(node: &Node<Processed>, output: &mut Vec<u8>) {
    match node {
        Node::Element(elem) => { /* 序列化标签 */ }
        Node::Text(text) => { /* 转义并输出 */ }
        Node::Frame(_) => unreachable!(),
    }
}
```

**验收**: 生成有效 HTML，可选 minify

---

### Task 2.8: transform/pipeline.rs - Pipeline 辅助 [1.5h]
**文件**: `src/transform/pipeline.rs`
**LOC**: ~80

```rust
/// Pipeline 构建辅助（文档 + 示例）
///
/// 注意：不使用 struct Pipeline，而是提供函数示例
/// 原因：直接链式调用可完全内联，无运行时开销

/// Phase 1: 快速元数据提取
pub fn extract_metadata(doc: Document<Raw>) -> ContentMeta {
    let indexed = Indexer.transform(doc);
    // 提取 tola-meta，不做完整转换
    indexed.ext.base.content_meta.unwrap_or_default()
}

/// Phase 2: 完整 HTML 生成
pub fn render_html(doc: Document<Raw>, site_data: &SiteData) -> Vec<u8> {
    let indexed = Indexer.transform(doc);
    let mut indexed = indexed;
    LinkProcessor { path_prefix: None, base_url: None }.transform(&mut indexed);
    HeadingProcessor.transform(&mut indexed);
    let processed = FrameExpander.transform(indexed);
    let mut processed = processed;
    HeadInjector { site_data, page_meta: &... }.transform(&mut processed);
    HtmlRenderer { minify: true }.render(&processed)
}
```

**验收**: 两个入口函数正确编译，文档完整

---

## Phase 3: 集成 [8h]

### Task 3.1: vdom/convert.rs - from_typst_html [2h]
**文件**: `src/vdom/convert.rs`
**LOC**: ~150

```rust
pub fn from_typst_html(
    doc: &typst_library::html::HtmlDocument,
    source: PathBuf,
    deps: Vec<PathBuf>,
    meta: Option<ContentMeta>,
) -> Document<Raw> {
    fn convert_node(node: &HtmlNode) -> Option<Node<Raw>> {
        match node {
            HtmlNode::Tag(_) => None,  // 过滤
            HtmlNode::Text(t, span) => Some(Node::Text(...)),
            HtmlNode::Element(e) => Some(Node::Element(...)),
            HtmlNode::Frame(f) => Some(Node::Frame(Frame {
                ext: RawFrameExt { frame_ref: Arc::new(f.clone()), ... }
            })),
        }
    }
    // ...
}
```

**验收**: 正确转换所有 4 种 HtmlNode 类型

---

### Task 3.2: compiler/pages.rs - Phase 1 集成 [2h]
**文件**: `src/compiler/pages.rs`
**修改**: 现有代码

```rust
// collect_metadata() 中：
pub fn collect_metadata(...) -> Result<...> {
    for page in typ_files {
        let html_doc = compile_meta(page, config)?;
        let doc = from_typst_html(&html_doc, ...);
        let metadata = extract_metadata(doc);  // TTG Phase 1
        GLOBAL_SITE_DATA.insert_page(metadata);
    }
}
```

**验收**: Phase 1 正确提取元数据，virtual_fs 数据完整

---

### Task 3.3: compiler/pages.rs - Phase 2 集成 [2h]
**文件**: `src/compiler/pages.rs`
**修改**: 现有代码

```rust
// compile_pages_with_data() 中：
pub fn compile_pages_with_data(...) -> Result<...> {
    for page in typ_files {
        let html_doc = compile_meta(page, config)?;
        let doc = from_typst_html(&html_doc, ...);
        let html = render_html(doc, &GLOBAL_SITE_DATA);  // TTG Phase 2
        fs::write(&output_path, html)?;
    }
}
```

**验收**: 生成的 HTML 与现有输出一致（diff 测试）

---

### Task 3.4: 删除旧代码 + 最终验证 [2h]
**文件**: 多个

```
// 删除/弃用的代码：
- src/utils/xml/processor.rs (旧 process_html)
- src/utils/xml/head.rs (旧 head 注入)
- src/utils/xml/link.rs (旧 link 处理)
```

**验收**:
1. `cargo test` 全部通过
2. `cargo build --release` 成功
3. `__test_site/` 构建结果与旧版本一致
4. 基准测试：序列化次数从 3→1

---

## Phase 4: 优化 [6h] 🆕

> ⚠️ 经过独立思考，删除不适用的 Dioxus 概念，专注于真正有用的优化

### Task 4.1: Server-Side Diff 热更新 [2h] ⭐ P0
**文件**: `src/serve.rs` + `embed/reload.js`
**LOC**: ~150

> ⚠️ 直接做 server-side diff，不用 morphdom（太大）

**Server 端**：
```rust
pub enum Patch {
    Replace { selector: String, html: String },
    Delete { selector: String },
}

pub fn diff_documents(old: &Document<Processed>, new: &Document<Processed>) -> Vec<Patch> {
    // 简化版：比较顶级 children，生成 CSS selector + 替换 HTML
}
```

**极简 Applier (< 300B gzip)**：
```javascript
// embed/reload.js
const ws = new WebSocket(`ws://${location.host}/__tola`);
ws.onmessage = e => {
    JSON.parse(e.data).forEach(([op, sel, html]) => {
        const el = document.querySelector(sel);
        if (!el) return;
        if (op === 'r') el.outerHTML = html;
        if (op === 'd') el.remove();
    });
};
```

**验收**:
- 文件保存后浏览器 DOM 增量更新
- 不刷新页面，保持滚动位置
- Applier < 500B gzip

---


### Task 4.2: Frame 渲染缓存 [2h] ⭐ P1
**文件**: `src/transform/frame_cache.rs`
**LOC**: ~80

```rust
/// Frame 内容 hash → 缓存 SVG 渲染结果
pub struct FrameCache {
    cache: HashMap<u64, Vec<u8>>,
}

impl FrameCache {
    pub fn get_or_render(&mut self, frame: &Frame<Indexed>) -> &[u8] {
        let hash = hash_frame(&frame.ext);
        self.cache.entry(hash).or_insert_with(|| {
            typst_render::render_svg(&frame.ext.frame_ref)
        })
    }
}
```

**验收**: Watch 模式重复构建时，未变化的 Frame 不重新渲染

---

### Task 4.3: 字符串 Intern [2h] 🟡 P2 (可选)
**文件**: `src/vdom/attr.rs`
**LOC**: ~20

```rust
// 使用 compact_str 减少小字符串堆分配
use compact_str::CompactString;

pub struct Element<P> {
    pub tag: CompactString,  // 24 字节内联
}
```

**验收**: 内存占用减少（可用 DHAT profiler 验证）

---

### ~~Task 4.4: 静态模板分离~~ ❌ 已删除
> 不适用于 SSG：Typst 渲染的 HTML 无固定骨架

### ~~Task 4.5: 节点池化~~ ❌ 已删除
> 不适用于 SSG：无状态渲染无需保持组件实例

---


## 依赖关系图


```
Phase 0
  ├── 0.1 依赖
  └── 0.2 骨架
         │
Phase 1  ▼
  ├── 1.1 family.rs
  ├── 1.2 attr.rs
  ├── 1.3 phase.rs (trait)
  │    ├── 1.4 Raw
  │    ├── 1.5 Indexed
  │    └── 1.6 Processed
  ├── 1.7 node.rs (依赖 1.1-1.6)
  └── 1.8 visitor.rs + folder.rs (依赖 1.7)
               │
Phase 2        ▼
  ├── 2.1 Transform trait
  ├── 2.2 Indexer (依赖 1.8)
  ├── 2.3 LinkProcessor
  ├── 2.4 HeadingProcessor
  ├── 2.5 FrameExpander
  ├── 2.6 HeadInjector
  ├── 2.7 HtmlRenderer
  └── 2.8 pipeline.rs
               │
Phase 3        ▼
  ├── 3.1 convert.rs
  ├── 3.2 Phase 1 集成
  ├── 3.3 Phase 2 集成
  └── 3.4 清理 + 验证
```

---

## 风险与备选方案

| 风险 | 影响 | 备选方案 |
|------|------|----------|
| typst Frame 无法 Arc 共享 | 高 | 使用 frame_id 索引，运行时查找 |
| GAT 编译时间过长 | 中 | 减少泛型深度，使用 enum dispatch |
| 性能未达预期 | 中 | 保留旧代码路径，A/B 切换 |
| minify_html 不兼容 | 低 | 跳过 minify，或使用 html5ever |

---

## 验收清单

- [ ] `cargo test` 全部通过
- [ ] `cargo clippy` 无警告
- [ ] `cargo doc` 无警告
- [ ] `__test_site/` 构建成功
- [ ] 基准测试：编译时间不增加 >10%
- [ ] 基准测试：运行时减少序列化开销
