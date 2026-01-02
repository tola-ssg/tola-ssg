# TTG (Trees That Grow) 统一 VDOM 架构 v8

> 基于 GATs 的多阶段类型安全 SSG 架构 - Pipeline + Transform API


## 0. 设计反思与改进

### 0.1 v1 版本的问题

| 问题 | 描述 | 解决方案 |
|------|------|----------|
| **GAT 未真正发挥作用** | `ElemData<T: TagMarker>` 对所有标签返回相同类型 | 引入 `TagFamily` trait 实现按标签族特化 |
| **遗漏 typst-html 节点类型** | 未处理 `HtmlNode::Tag` 和 `HtmlNode::Frame` | 完整映射所有 4 种节点类型 |
| **Pipeline 类型擦除** | `Box<dyn Any>` 破坏零开销 | 使用泛型链式调用，编译期单态化 |
| **遍历器缺失** | 每个 Transform 重复实现递归遍历 | 引入 `Visitor` / `Folder` trait |
| **阶段边界模糊** | 同阶段转换过多 | 重新划分阶段职责 |

### 0.2 v2 设计审查发现的问题 ⚠️

| 问题 | 严重性 | 修复方案 |
|------|--------|----------|
| **visit() 克隆整个 DOM** | 🔴 高 | 传递引用，改用 `visit_element_ref()` |
| **Processed 阶段族信息丢失** | 🔴 高 | 保留 FamilyExt 变体，使用 `map_family_ext!` 宏 |
| **leave_element 无条件调用** | 🟡 中 | 仅当 visit 返回 true 时调用 leave |
| **SVG 标签识别不完整** | 🟡 中 | 补充 filter/animation/marker 等 40+ 标签 |
| **HtmlRenderer 不处理 void 元素** | 🟡 中 | 添加 VOID_ELEMENTS 列表，不生成关闭标签 |
| **缺少统一扩展访问器** | 🟡 中 | 引入 `HasNodeId` / `HasFamilyData<F>` trait |
| **FamilyExt 转换代码冗长** | 🟡 中 | 引入 `map_family_ext!` 宏简化 |

### 0.2.1 v6 独立批判分析 🔍

经过严格审查，发现以下问题和决策：

| 问题 | 分析 | 决策 |
|------|------|------|
| **LinkType 是否需要 GAT?** | LinkType(Absolute/Relative/External) 是**数据值**而非类型，用 GAT 抽象会导致类型爆炸 | ❌ 保持 enum，不用 GAT |
| **HasFamilyData 8 个重复 impl** | 每个 impl 只是访问器方法名不同 | ✅ 用 `impl_has_family_data!` 宏消除 |
| **identify_family 返回字符串** | 丢失类型信息，需要二次匹配 | ✅ 添加 `FamilyKind` 枚举 |
| **PhaseFamilyData 是否必要?** | 目前只是间接层，未简化代码 | 🟡 保留，为未来泛型 impl 留空间 |
| **FamilyExt 访问器重复** | 每个方法单行，宏反而增加复杂度 | ❌ 保持现状 |

### 0.3 v3 关键改进：辅助访问器与宏

#### 问题：FamilyExt 的模式匹配干扰

```rust
// 问题：每次访问 node_id 都需要 5 分支匹配
let node_id = match &elem.ext {
    FamilyExt::Svg(e) => e.node_id,
    FamilyExt::Link(e) => e.node_id,
    FamilyExt::Heading(e) => e.node_id,
    FamilyExt::Media(e) => e.node_id,
    FamilyExt::Other(e) => e.node_id,
};
```

#### 解决方案 1：统一访问器 Trait

```rust
/// 统一访问 NodeId（适用于 Indexed 阶段）
pub trait HasNodeId {
    fn node_id(&self) -> NodeId;
}

impl HasNodeId for Element<Indexed> {
    fn node_id(&self) -> NodeId {
        match &self.ext {
            FamilyExt::Svg(e) => e.node_id,
            FamilyExt::Link(e) => e.node_id,
            // ... 只写一次，封装细节
        }
    }
}

// 使用：简洁、类型安全
let id = elem.node_id();  // 无需关心族类型
```

#### 解决方案 2：统一阶段感知族数据访问器 (v5 改进)

```rust
/// 🔑 v7 改进：删除 PhaseFamilyData 中间层，使用关联类型
///
/// 之前（过度抽象）：
/// - HasFamilyData<P, F> 需要两个类型参数
/// - PhaseFamilyData<P> 只是间接层
/// - 调用冗长：HasFamilyData<Indexed, LinkFamily>
///
/// 现在（更优雅）：
/// - HasFamilyData<F> 只需一个类型参数
/// - Data 是关联类型，由 impl 决定
/// - 调用简洁：HasFamilyData<LinkFamily>

pub trait HasFamilyData<F: TagFamily> {
    type Data;  // 关联类型，由 impl 决定具体类型
    fn family_data(&self) -> Option<&Self::Data>;
    fn family_data_mut(&mut self) -> Option<&mut Self::Data>;
}

// impl 时指定具体类型
impl HasFamilyData<LinkFamily> for Element<Indexed> {
    type Data = LinkIndexedData;  // Indexed 阶段是 IndexedData
    fn family_data(&self) -> Option<&Self::Data> { ... }
}

impl HasFamilyData<LinkFamily> for Element<Processed> {
    type Data = LinkProcessedData;  // Processed 阶段是 ProcessedData
    fn family_data(&self) -> Option<&Self::Data> { ... }
}

// 使用：P 由 Self 决定，无需显式指定
let data = <Element<Indexed> as HasFamilyData<LinkFamily>>::family_data(&elem);
// data 的类型自动是 &LinkIndexedData
```

#### 解决方案 3：FamilyExt 转换宏

```rust
/// 映射 FamilyExt 到新阶段，保留族信息
macro_rules! map_family_ext {
    ($ext:expr, |$family:pat_param| $new_ext:expr) => {
        match $ext {
            FamilyExt::Svg($family) => FamilyExt::Svg($new_ext),
            FamilyExt::Link($family) => FamilyExt::Link($new_ext),
            FamilyExt::Heading($family) => FamilyExt::Heading($new_ext),
            FamilyExt::Media($family) => FamilyExt::Media($new_ext),
            FamilyExt::Other($family) => FamilyExt::Other($new_ext),
        }
    };
}

// 使用：一行代替五行
let new_ext = map_family_ext!(old_ext, |_| ProcessedElemExt { modified });
```

### 0.4 GAT 的真实价值评估 🔑 (v7 修订)

**独立思考关键洞察**：GAT 的真正价值不仅在于 Indexed 阶段，而是**每个阶段可以有不同的族特定数据**！

#### 旧认知（错误）
```rust
// ❌ Processed 阶段忽略 F 类型参数
impl PhaseData for Processed {
    type ElemExt<F: TagFamily> = ProcessedElemExt;  // F 被忽略！
}
```

#### 新认知（正确）
```rust
// ✅ Processed 阶段真正利用 F
impl PhaseData for Processed {
    type ElemExt<F: TagFamily> = ProcessedElemExt<F>;  // F::ProcessedData 被使用！
}

// 每个族有两种不同的数据：
pub trait TagFamily {
    type IndexedData;    // Indexed 阶段收集的原始数据
    type ProcessedData;  // Processed 阶段的处理结果
}
```

#### GAT 的双阶段数据价值

| 族 | IndexedData (收集阶段) | ProcessedData (处理阶段) |
|----|----------------------|-------------------------|
| **Link** | `original_href, link_type` | `resolved_url, is_broken, nofollow` |
| **SVG** | `is_root, viewbox, dimensions` | `optimized, original_bytes, optimized_bytes` |
| **Heading** | `level, original_id` | `anchor_id, toc_text, in_toc` |
| **Media** | `src, is_svg_image` | `resolved_src, width, height, lazy_load` |

**关键价值**：
1. **类型安全的阶段演进** - LinkIndexedData → LinkProcessedData 是编译期保证的
2. **零开销抽象** - 不同族的数据在编译期单态化
3. **架构清晰** - 收集与处理职责分离，数据结构各自优化
4. **统一访问接口** - `PhaseFamilyData<P>` 关联使同一 trait 访问不同阶段数据

### 0.5 核心改进总结

```
v1: Phase → PhaseData { ElemData<T> = SameType }  // GAT 是摆设
v2: Phase → PhaseData { ElemExt<F: TagFamily> }   // 按标签族特化
v3: + HasNodeId / HasFamilyData<F> 访问器 Trait
    + map_family_ext! 宏简化转换
    + 性能优化（避免克隆、void 元素处理）
v4: TagFamily { IndexedData, ProcessedData }      // 🔑 双阶段族数据！
    + HasIndexedData<F> / HasProcessedData<F> 分离
    + Indexed → Processed 真正的数据转换
v5: PhaseFamilyData<P: PhaseData>                 // 🔑 GAT 统一访问器！
    + HasFamilyData<P, F> 统一两阶段访问
    + 同一 trait，不同 P，自动选择正确数据类型
```

### 0.6 代码库集成分析 🔗

> ⚠️ **关键洞察**：TTG 架构需要与现有 tola-ssg 代码无缝集成

#### 0.6.1 模块级别集成难度分析

| 模块 | 文件数 | 改动程度 | 难度 | 说明 |
|------|--------|----------|------|------|
| `typst_lib/` | 7 | 🟢 低 | ⭐ | VDOM 入口点，只需改 `compile_meta()` 返回类型 |
| `compiler/pages.rs` | 1 | 🟡 中 | ⭐⭐ | 调用链变更，逻辑保留 |
| `utils/xml/processor.rs` | 1 | 🔴 高 | ⭐⭐⭐ | 完全重写为 VDOM pipeline |
| `utils/xml/link.rs` | 1 | 🟢 低 | ⭐ | 逻辑复用，只改调用方式 |
| `utils/svg/` | 5 | 🟡 中 | ⭐⭐ | 迁移到 FrameExpander |
| `serve.rs` | 1 | 🟡 中 | ⭐⭐ | 新增 WebSocket 热更新 |
| `watch.rs` | 1 | 🟢 低 | ⭐ | 无需改动，只是调用新 API |

#### 0.6.2 各模块详细分析

**1. `typst_lib/mod.rs` — VDOM 入口点 ⭐**

```rust
// 🔑 关键发现：compile_base() 已返回 HtmlDocument！
fn compile_base(path: &Path, root: &Path)
    -> Result<(SystemWorld, typst_html::HtmlDocument)>

// 当前：HtmlDocument → String (序列化)
let html = typst_html::html(&document).into_bytes();

// VDOM：HtmlDocument → Document<Raw> (零拷贝转换)
let vdom = from_typst_html(&document);  // 直接使用 HtmlDocument
```

**难点**：无，`HtmlDocument` 就是结构化 DOM，不需要解析

---

**2. `compiler/pages.rs` — 调用链变更 ⭐⭐**

```rust
// 当前流程
compile_meta() → html bytes → process_html() → minify → write

// VDOM 流程
compile_meta_vdom() → Document<Raw> → Pipeline → render → write
    └── from_typst_html()  └── Transform chain  └── HtmlRenderer
```

**难点**：需要重构 `write_page()` 接受 `Document<Processed>` 而非 `Vec<u8>`

---

**3. `utils/xml/processor.rs` — 完全重写 ⭐⭐⭐**

```rust
// 当前：170 行流式处理
pub fn process_html(content: &[u8], ...) -> Result<Vec<u8>> {
    let mut reader = create_xml_reader(content);
    loop {
        match reader.read_event() {
            Event::Start(elem) => handle_start_element(...)
            // ...
        }
    }
}

// VDOM：删除此文件，改为 pipeline
pub fn process_vdom(doc: Document<Raw>, ...) -> Document<Processed> {
    let indexed = Indexer::new(config).transform(doc);
    LinkProcessor::new(config).transform(&mut indexed);
    FrameExpander::new(config).transform(indexed)
}
```

**难点**：
- 需要实现所有 Transform
- 边缘 case 迁移（void elements, CDATA, etc.）

---

**4. `utils/xml/link.rs` — 逻辑复用 ⭐**

```rust
// 保留核心函数，仍可在 LinkProcessor 中调用
pub fn process_link_value(value: &[u8], ...) -> Result<Cow<'static, [u8]>>
pub fn process_absolute_link(value: &str, ...) -> Result<String>
pub fn is_external_link(link: &str) -> bool

// 只需在 VDOM 中调用：
impl MutVisitor<Indexed> for LinkProcessor<'_> {
    fn visit_element_mut(&mut self, elem: &mut Element<Indexed>) -> bool {
        if let Some(href) = elem.get_attr("href") {
            let processed = process_link_value(href.as_bytes(), ...)?;
            elem.set_attr("href", processed);
        }
        true
    }
}
```

**难点**：无，直接复用

---

**5. `utils/svg/` — SVG 处理策略 ⭐⭐**

**关键发现**：`typst_html::HtmlDocument` 包含 `HtmlNode::Frame`！

```rust
// typst_html 内部结构
pub enum HtmlNode {
    Frame(Frame),  // 🔑 这是 typst::layout::Frame，不是已渲染的 SVG
    Element(HtmlElement),
    Text(EcoString, Span),
    Tag(Tag),
}
```

**两种方案对比**：

| 方案 | 流程 | 性能 | 复杂度 |
|------|------|------|--------|
| **A. 解析 SVG 字符串** | HtmlDocument → html() → String → 解析 → 再序列化 | ❌ 差 | 🟢 低 |
| **B. 直接访问 Frame** | HtmlDocument → from_typst_html() 保留 Frame → typst_render | ✅ 优 | 🟡 中 |

**✅ 建议：选 B（直接访问 Frame）**

```rust
// from_typst_html 转换时保留 Frame
fn convert_node(node: &typst_html::HtmlNode) -> Node<Raw> {
    match node {
        HtmlNode::Frame(frame) => Node::Frame(Frame {
            ext: RawFrameExt {
                frame: Arc::clone(frame),  // 🔑 保留引用，不序列化
            }
        }),
        HtmlNode::Element(elem) => /* ... */,
        // ...
    }
}

// FrameExpander 中直接渲染
impl Folder<Indexed, Processed> for FrameExpander<'_> {
    fn fold_frame(&mut self, frame: Frame<Indexed>) -> Node<Processed> {
        // 直接调用 typst_render，跳过 typst_html::html() 的序列化
        let svg_bytes = typst_render::render_svg(&frame.ext.frame);
        let optimized = optimize_svg(&svg_bytes, config)?;  // 复用现有
        // ...
    }
}
```

**性能收益**：
- 跳过 `typst_html::html()` 中的 SVG 序列化
- 跳过 `extract_svg_element()` 中的 quick_xml 解析
- Frame 可以用 hash 缓存，避免重复渲染

---

**6. `serve.rs` — Server-Side Diff 热更新 ⭐⭐**

> ⚠️ 决策：直接实现 server-side diff + 极简 applier，不用 morphdom

**理由**：
- morphdom 2.5KB，用户希望更小
- Client-side diff 后续改 server-side 需重写协议
- 一步到位更优

**极简 Applier (< 300B gzip)**：
```javascript
// embed/reload.js
const ws = new WebSocket(`ws://${location.host}/__tola`);
ws.onmessage = (e) => {
    const patches = JSON.parse(e.data);
    patches.forEach(([op, sel, html]) => {
        const el = document.querySelector(sel);
        if (!el) return;
        if (op === 'r') el.outerHTML = html;  // replace
        if (op === 'd') el.remove();          // delete
        if (op === 'i') el.insertAdjacentHTML('afterend', html);
    });
};
```

**Server-Side Diff (Rust)**：
```rust
pub enum Patch {
    Replace { selector: String, html: String },
    Delete { selector: String },
    Insert { selector: String, html: String },
}

pub fn diff_documents(old: &Document<Processed>, new: &Document<Processed>) -> Vec<Patch> {
    // 使用节点路径作为 selector
    // 比较 old 和 new 的 children
    // 输出最小 patch 集
}
```

**MVP 阶段可简化为整页替换**：
```javascript
// 极简 MVP (< 100B)
const ws = new WebSocket(`ws://${location.host}/__tola`);
ws.onmessage = e => location.reload();  // 最简单，后续改进
```

---


#### 0.6.3 重构策略建议 🚀

**问题：渐进式 vs 激进式？**

| 策略 | 优点 | 缺点 |
|------|------|------|
| **渐进式** | 风险低，可回滚 | 兼容代码膨胀，维护负担 |
| **激进式** | 架构清晰，一步到位 | 临时不可用，需完整测试 |

**✅ 建议：激进重构**

理由：
1. **VDOM 入口已存在** — `compile_base()` 返回 `HtmlDocument`，不需要解析
2. **现有代码规模可控** — `xml/processor.rs` 只有 196 行
3. **逻辑可复用** — `link.rs`, `svg/optimize.rs` 不需要重写
4. **测试覆盖良好** — `ttg_demo.rs` 已有 17 个测试

**重构顺序：**
```
1. src/vdom/          [新增] VDOM 核心类型 (2-3 天)
2. src/vdom/convert.rs [新增] HtmlDocument → Document<Raw>
3. src/transform/      [新增] Transform pipeline
4. src/typst_lib/      [修改] compile_meta() → compile_vdom()
5. src/compiler/pages.rs [修改] 使用新 pipeline
6. src/utils/xml/processor.rs [删除] 旧流式处理
7. src/serve.rs        [修改] 添加 WebSocket
```

---

#### 0.6.4 关键代码路径变更

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 当前流程                                                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  typst::compile() → typst_html::realize() → typst_html::html()             │
│       ↓                     ↓                     ↓                        │
│   Document            HtmlDocument            String                       │
│                                                   ↓                        │
│                                          quick_xml::Reader                 │
│                                                   ↓                        │
│                                          process_html() 流式处理            │
│                                                   ↓                        │
│                                          minify + write                    │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ VDOM 流程                                                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  typst::compile() → typst_html::realize() → from_typst_html()              │
│       ↓                     ↓                     ↓                        │
│   Document            HtmlDocument          Document<Raw>                  │
│                                                   ↓                        │
│                                             Indexer                        │
│                                                   ↓                        │
│                                          Document<Indexed>                 │
│                                                   ↓                        │
│                                    LinkProcessor + HeadingProcessor        │
│                                                   ↓                        │
│                                          FrameExpander                     │
│                                                   ↓                        │
│                                         Document<Processed>                │
│                                                   ↓                        │
│                                          HtmlRenderer                      │
│                                                   ↓                        │
│                                          minify + write                    │
│                                                                             │
│  🔑 关键变化：                                                               │
│  - 跳过 typst_html::html() 序列化                                           │
│  - 跳过 quick_xml 解析（零解析）                                             │
│  - Frame 延迟渲染（可缓存）                                                  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```



### 0.7 Dioxus 分析与适用性评估 🔥

> ⚠️ **独立思考**：经过严格分析，Dioxus 的部分概念**不适用于 SSG 场景**

#### 0.7.1 Dioxus 机制适用性评估

| Dioxus 机制 | 原理 | **是否适用于 tola** | 原因 |
|------------|------|---------------------|------|
| **静态 Template** | 编译期 RSX 宏展开为 `&'static` DOM 骨架 | ❌ **不适用** | Typst 渲染的 HTML 结构由内容决定，无固定骨架 |
| **DynamicValuePool** | 热重载时复用已渲染组件 | ❌ **不适用** | SSG 无状态渲染，无需保持组件实例 |
| **AST Diff** | 比较 RSX 宏的 AST 变化 | ❌ **不直接适用** | 但 DOM diff 思路可借鉴 |
| **字符串 Intern** | 标签名等复用同一内存 | ✅ **适用** | 减少重复字符串分配 |

#### 0.7.2 Typst `templates/utils` vs Dioxus Template

| 概念 | Typst 的 `templates/utils/` | Dioxus 的 Template |
|------|---------------------------|-------------------|
| **本质** | 源码级别依赖文件 | 编译期静态 DOM 骨架 |
| **用途** | 增量重建追踪 | 热重载时复用结构 |
| **你已有的方案** | `compiler/deps.rs` 依赖图 | - |
| **结论** | ✅ **当前方案正确** | ❌ 概念不相关 |

#### 0.7.3 真正有用的优化

**1. Server-Side Diff 热更新（P0 优先级）**

> ⚠️ 决策变更：直接做 server-side diff，不用 morphdom

```
┌──────────────────────────────────────────────────────────────────┐
│ Watch 模式无感刷新 (Server-Side Diff)                            │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. .typ 文件变化                                                │
│  2. Server: 重编译 → VDOM diff → patches                        │
│  3. WebSocket 发送: [["r", "#main", "<div>...</div>"], ...]     │
│  4. 极简 Applier (< 300B): 按 selector 更新 DOM                 │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

**为什么不用 morphdom**：
- morphdom 2.5KB gzip，用户期望 < 500B
- Client-side diff 后续改 server-side 需重写协议
- 一步到位更优

**极简 Applier**：
```javascript
// embed/reload.js (< 300B gzip)
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

**2. Frame 渲染缓存（P1 优先级）**

```rust
/// Frame 内容 hash → 缓存 SVG 渲染结果
pub struct FrameCache {
    cache: HashMap<u64, Vec<u8>>,
}

impl FrameCache {
    pub fn get_or_render(&mut self, frame: &Frame<Indexed>) -> &[u8] {
        let hash = hash_frame(&frame.ext);
        self.cache.entry(hash).or_insert_with(|| {
            typst_render::render_svg(&frame.ext.frame)
        })
    }
}
```

**3. 字符串 Intern（P2 可选）**

```rust
// 使用 compact_str 减少小字符串分配
pub struct Element<P> {
    pub tag: CompactString,  // 24 字节内联，无堆分配
}
```

#### 0.7.4 修订后的优先级

| # | 优化 | 收益 | 复杂度 | 状态 |
|---|------|------|--------|------|
| **P0** | Server-side diff + 极简 applier | 🔴 高 (无感刷新) | 🟡 中 | 新策略 |
| **P1** | Frame 渲染缓存 | 🔴 高 (SVG 慢) | 🟢 低 | 保留 |
| **P2** | 字符串 Intern | 🟡 中 (内存) | 🟢 低 | 可选 |
| ~~P3~~ | ~~morphdom~~ | - | - | ❌ 放弃 |
| ~~P4~~ | ~~静态模板分离~~ | - | - | ❌ 不适用 |
| ~~P5~~ | ~~节点池化~~ | - | - | ❌ 不适用 |

---

### 0.8 工程实现细节 🔧

> 激进重构策略：从一开始就确保正确性、高性能、清晰可维护的架构

#### 0.8.1 模块结构设计

```
src/
├── vdom/                       # VDOM 核心 (新增)
│   ├── mod.rs                  # 公共 API
│   ├── phase.rs                # Phase trait + Raw/Indexed/Processed/Rendered
│   ├── family.rs               # TagFamily trait + FamilyKind enum
│   ├── node.rs                 # Document/Element/Text/Frame 结构
│   ├── attr.rs                 # Attr/AttrName/AttrValue
│   ├── visitor.rs              # Visitor/MutVisitor traits
│   ├── folder.rs               # Folder trait (阶段转换)
│   └── convert.rs              # from_typst_html() 转换
│
├── transform/                  # Transform pipeline (新增)
│   ├── mod.rs
│   ├── indexer.rs              # Raw → Indexed (分配 NodeId, 识别 Family)
│   ├── link_processor.rs       # 链接处理 (MutVisitor)
│   ├── heading_processor.rs    # 标题 ID slugify (MutVisitor)
│   ├── frame_expander.rs       # Frame → SVG Element (Folder: Indexed → Processed)
│   ├── head_injector.rs        # <head> 注入 (MutVisitor)
│   ├── renderer.rs             # HtmlRenderer (Processed → Rendered)
│   └── frame_cache.rs          # Frame 渲染缓存
│
├── typst_lib/                  # 修改
│   └── mod.rs                  # compile_vdom() 新增
│
├── compiler/                   # 修改
│   └── pages.rs                # 使用新 pipeline
│
├── serve.rs                    # 修改: WebSocket + diff
│
└── utils/
    ├── xml/                    # 删除大部分，保留
    │   ├── link.rs             # 复用 process_link_value()
    │   └── escape.rs           # HTML 转义工具
    └── svg/
        ├── optimize.rs         # 复用 optimize_svg()
        └── compress.rs         # 复用 compress_svgs_parallel()
```

#### 0.8.2 类型设计原则

**1. 零运行时开销**
```rust
// ✅ 使用 newtype 而非 enum 标记阶段
pub struct Raw;
pub struct Indexed;
pub struct Processed;

// ✅ 编译期单态化，无虚表
pub struct Element<P: PhaseData> { ... }

// ❌ 避免 Box<dyn Any>
```

**2. 内存布局优化**
```rust
// 目标：Element<P> 单 cache line (64 bytes)
pub struct Element<P: PhaseData> {
    pub tag: CompactString,        // 24 bytes
    pub attrs: SmallVec<[Attr; 4]>, // 32 bytes (4 attrs inline)
    pub children: Vec<Node<P>>,    // 24 bytes  (指针)
    pub ext: FamilyExt<P>,         // 依阶段变化
}

// 使用 #[repr(C)] 确保布局可预测
#[repr(C)]
pub struct NodeId(pub u32);  // 4 bytes, 非 usize
```

**3. 简化遍历 API：Pipeline + Transform**

> ⚠️ **设计变更**：删除 Visitor/Folder/MutVisitor 三个 trait，替换为单一 Transform trait + Pipeline builder

**旧设计（❌ 冗杂）**：
```rust
// 5 个互相关联的 trait，用户需要选择正确的组合
trait Visitor<P>      // 只读遍历
trait MutVisitor<P>   // 同阶段修改
trait Folder<From, To> // 阶段转换
trait Transform       // 阶段转换包装
trait InPlaceTransform<P> // 同阶段包装
```

**新设计（✅ 简洁）**：
```rust
// ============================================================================
// 单一 Transform trait
// ============================================================================
pub trait Transform<From: Phase> {
    type To: Phase;
    fn transform(self, doc: Document<From>) -> Document<Self::To>;
}

// ============================================================================
// Pipeline：类型安全的链式调用
// ============================================================================
pub struct Pipeline<P: Phase> {
    doc: Document<P>,
}

impl<P: Phase> Pipeline<P> {
    pub fn new(doc: Document<P>) -> Self { Self { doc } }

    /// 阶段转换：P → T::To
    pub fn then<T: Transform<P>>(self, t: T) -> Pipeline<T::To> {
        Pipeline { doc: t.transform(self.doc) }
    }

    /// 同阶段就地修改
    pub fn apply<F: FnOnce(&mut Document<P>)>(mut self, f: F) -> Self {
        f(&mut self.doc);
        self
    }

    pub fn finish(self) -> Document<P> { self.doc }
}
```

**使用示例**：
```rust
// 完整 pipeline，一行链式调用
let html = Pipeline::new(raw_doc)
    .then(Indexer::new())                     // Raw → Indexed
    .apply(|doc| process_links(doc, config))  // 修改
    .apply(|doc| process_headings(doc))       // 修改
    .then(FrameExpander::new(config))         // Indexed → Processed
    .then(HtmlRenderer::new())                // Processed → Rendered
    .finish()
    .html;

// Transform 实现只需一个方法
impl Transform<Raw> for Indexer<'_> {
    type To = Indexed;
    fn transform(self, doc: Document<Raw>) -> Document<Indexed> {
        // 实现逻辑
    }
}

// 简单修改用闭包，无需定义 struct
fn process_links(doc: &mut Document<Indexed>, config: &SiteConfig) {
    doc.for_each_element_mut(|elem| {
        if elem.tag == "a" {
            if let Some(href) = elem.get_attr("href") {
                elem.set_attr("href", process_link_value(href, config));
            }
        }
    });
}
```

**对比表**：
| 维度 | 旧 Visitor/Folder | 新 Pipeline + Transform |
|------|-------------------|-------------------------|
| **Trait 数量** | 5 | **1** |
| **样板代码** | 多（实现多个方法） | **少**（只需一个方法） |
| **简单修改** | 需定义 struct | **用闭包** |
| **类型安全** | ✅ | ✅ |
| **链式调用** | ❌ | ✅ |

##### v8 改进：更简洁的链式调用

```rust
// v8 新增：doc.pipe(T) 快捷方法
let rendered = raw_doc
    .pipe(Indexer)
    .pipe(LinkProcessor::new().with_prefix("blog"))
    .pipe(FrameExpander)
    .pipe(HtmlRenderer);

// 等价于 Pipeline::new(raw_doc).then(Indexer)...finish()
// 但更简洁
```

##### v8 改进：查询 API

```rust
// 查找第一个满足条件的元素
let link = doc.find_element(|e| e.ext.is_link());

// 查找所有满足条件的元素
let all_svgs = doc.find_all(|e| e.ext.is_svg());

// 检查是否存在
let has_heading = doc.has_element(|e| e.ext.is_heading());

// 检查是否有 Frame 节点
let has_frames = doc.has_frames();
```

##### v8 改进：LinkProcessor 作为 Transform

```rust
// 旧方式（闭包）
.apply(|doc| process_links(doc, Some("blog")))

// 新方式（Transform，类型更安全）
.pipe(LinkProcessor::new().with_prefix("blog"))

// LinkProcessor 实现
impl Transform<Indexed> for LinkProcessor {
    type To = Indexed;  // 同阶段转换
    fn transform(self, mut doc: Document<Indexed>) -> Document<Indexed> {
        // 处理链接
        doc
    }
}
```


#### 0.8.3 错误处理策略

```rust
// 自定义错误类型
#[derive(Debug, thiserror::Error)]
pub enum VdomError {
    #[error("NodeId overflow: max nodes exceeded")]
    NodeIdOverflow,

    #[error("Invalid HtmlDocument structure: {0}")]
    InvalidDocument(String),

    #[error("Frame rendering failed: {0}")]
    FrameRenderError(#[from] typst_render::RenderError),

    #[error("Link processing failed: {0}")]
    LinkError(String),
}

pub type VdomResult<T> = Result<T, VdomError>;

// Pipeline 错误汇总
#[derive(Debug)]
pub struct PipelineErrors {
    errors: Vec<(NodeId, VdomError)>,
}
```

#### 0.8.4 测试策略

**1. 单元测试 (每个模块)**
```rust
#[cfg(test)]
mod tests {
    // 覆盖每个 TagFamily 的识别
    #[test]
    fn test_identify_family_all_svg_tags() { ... }

    // 覆盖每个阶段转换
    #[test]
    fn test_indexer_assigns_unique_node_ids() { ... }

    // 边缘 case
    #[test]
    fn test_element_with_100k_children() { ... }
}
```

**2. 集成测试 (pipeline)**
```rust
// tests/vdom_integration.rs
#[test]
fn test_full_pipeline_matches_old_output() {
    let old_html = process_html(content, config)?;
    let new_html = process_vdom(content, config)?;

    // 规范化后比较（忽略空白）
    assert_eq!(normalize(old_html), normalize(new_html));
}
```

**3. 属性测试 (fuzzing)**
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_render_never_panics(doc: Document<Raw>) {
        let _ = Pipeline::new()
            .index(doc)
            .process()
            .render();
    }
}
```

**4. 基准测试**
```rust
// benches/vdom_bench.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_pipeline(c: &mut Criterion) {
    c.bench_function("full_pipeline", |b| {
        b.iter(|| process_vdom(&SAMPLE_HTML, &config))
    });
}
```

#### 0.8.5 激进重构检查清单

**第一阶段：VDOM 核心 (Week 1)**
- [ ] `src/vdom/` 模块创建
- [ ] 所有 Phase/Family 类型
- [ ] `from_typst_html()` 转换（跳过 `typst_html::html()`）
- [ ] 基本 Visitor/Folder
- [ ] 17 个 ttg_demo 测试迁移到新模块

**第二阶段：Transform Pipeline (Week 2)**
- [ ] Indexer, LinkProcessor, HeadingProcessor
- [ ] FrameExpander（直接访问 Frame，不解析 SVG 字符串）
- [ ] HeadInjector, HtmlRenderer
- [ ] 集成测试：输出与旧代码一致

**第三阶段：切换与删除 (Week 3)**
- [ ] `compile_vdom()` 替代 `compile_meta()`
- [ ] `pages.rs` 使用新 pipeline
- [ ] 删除 `utils/xml/processor.rs`
- [ ] 删除 `utils/xml/head.rs`
- [ ] 基准测试确认性能提升

**第四阶段：热更新 (Week 4)**
- [ ] WebSocket 端点
- [ ] Server-side diff
- [ ] 极简 applier (< 300B)
- [ ] Frame 渲染缓存

#### 0.8.6 性能目标

| 指标 | 当前 | 目标 | 验证方式 |
|------|------|------|----------|
| **序列化次数** | 3 | **1** | 代码审查 |
| **解析次数** | 2 | **0** | 代码审查 |
| **内存峰值** | - | **< 2x 输入大小** | DHAT profiler |
| **编译时间** | baseline | **< +10%** | `cargo build --timings` |
| **运行时间** | baseline | **< -20%** | criterion bench |

#### 0.8.7 删除代码清单

| 文件 | 行数 | 原因 |
|------|------|------|
| `utils/xml/processor.rs` | ~200 | VDOM pipeline 替代 |
| `utils/xml/head.rs` | ~100 | HeadInjector 替代 |
| `utils/svg/extract.rs` | ~230 | FrameExpander 替代 |

**保留**：
- `utils/xml/link.rs` - 复用 `process_link_value()`
- `utils/svg/optimize.rs` - 复用 `optimize_svg()`
- `utils/svg/compress.rs` - 复用 `compress_svgs_parallel()`





### 1.1 typst-library HtmlNode 定义

```rust
// typst-library/src/html/dom.rs
pub struct HtmlDocument {
    pub root: HtmlElement,           // 根元素
    pub info: DocumentInfo,          // 文档信息
    pub introspector: Introspector,  // 查询能力
}

pub enum HtmlNode {
    Tag(Tag),                    // ⚠️ Introspection 标记（需过滤）
    Text(EcoString, Span),       // 文本节点
    Element(HtmlElement),        // 元素节点
    Frame(Frame),                // ⚠️ Typst 渲染帧（SVG 来源！）
}

pub struct HtmlElement {
    pub tag: HtmlTag,            // 标签（PicoStr intern）
    pub attrs: HtmlAttrs,        // 属性（EcoVec）
    pub children: Vec<HtmlNode>, // 子节点
    pub span: Span,              // 源码位置
}
```

**关键发现**:
- `HtmlNode::Tag` 是 introspection 标记，转换时应过滤
- `HtmlNode::Frame` 包含 Typst 渲染帧（图表、公式），是 SVG 的**真正来源**

### 1.2 当前处理流程（问题分析）

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        当前流程（5 次解析/序列化）                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. Typst 编译                                                              │
│     typst::compile() → typst::Document                                      │
│                        ↓                                                    │
│  2. typst_html 转换                                                         │
│     typst_html::realize() → HtmlDocument                                    │
│     ├─ Frame → 此时仍为 Frame 结构                                           │
│     └─ 输出: HtmlDocument (结构化 DOM)                                       │
│                        ↓                                                    │
│  3. typst_html::html() 序列化  ❌ 第1次序列化                                │
│     ├─ Frame → inline SVG string                                            │
│     └─ 输出: String                                                         │
│                        ↓                                                    │
│  4. quick_xml::Reader 解析    ❌ 第1次解析                                   │
│     输出: Event stream                                                      │
│                        ↓                                                    │
│  5. process_html() 处理                                                     │
│     ├─ SVG → capture bytes     ❌ 第2次序列化                                │
│     ├─ SVG → usvg::Tree        ❌ 第2次解析                                  │
│     └─ Tree → serialize        ❌ 第3次序列化                                │
│                        ↓                                                    │
│  6. minify_html()                                                           │
│     输出: Vec<u8>                                                           │
│                                                                             │
│  总计: 3 次序列化 + 2 次解析 = 5 次转换开销                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.3 目标流程

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        目标流程（1 次序列化）                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. Typst 编译                                                              │
│     typst::compile() → typst::Document                                      │
│                        ↓                                                    │
│  2. typst_html 转换                                                         │
│     typst_html::realize() → HtmlDocument                                    │
│                        ↓                                                    │
│  3. from_typst_html()                    ✓ 零拷贝结构转换                    │
│     HtmlDocument → Document<Raw>                                            │
│     ├─ Frame 保持为 Frame（延迟处理）                                        │
│     └─ 过滤 Tag 节点                                                         │
│                        ↓                                                    │
│  4. Pipeline transforms                  ✓ 内存中 DOM 操作                   │
│     ├─ Indexer: Raw → Indexed                                               │
│     ├─ LinkProcessor: in-place                                              │
│     ├─ FrameExpander: Indexed → Processed                                   │
│     │   └─ Frame → typst_render::render_svg() → Element                     │
│     └─ HeadInjector: in-place                                               │
│                        ↓                                                    │
│  5. HtmlRenderer                         ✓ 唯一1次序列化                     │
│     Document<Processed> → Vec<u8>                                           │
│                                                                             │
│  总计: 1 次序列化，0 次中间解析                                               │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 2. 核心类型系统

### 2.1 标签族系统（真正的 GAT 特化）

```rust
/// 标签族 - 用于 GAT 按标签族特化扩展数据
///
/// 关键洞察：不需要为每个标签定义类型，而是按"族"分组
/// 同一族的标签共享相同的扩展数据结构
pub trait TagFamily: 'static + Send + Sync {
    /// 族名称
    const NAME: &'static str;
    /// 族特定数据类型
    type Data: Clone + Send + Sync + Default;
}

// ════════════════════════════════════════════════════════════════════════
// 标签族定义
// ════════════════════════════════════════════════════════════════════════

/// SVG 族: <svg> 及其所有子元素
pub struct SvgFamily;
impl TagFamily for SvgFamily {
    const NAME: &'static str = "svg";
    type Data = SvgFamilyData;
}

#[derive(Clone, Default)]
pub struct SvgFamilyData {
    pub is_root: bool,           // 是否为 <svg> 根元素
    pub viewbox: Option<ViewBox>,
    pub dimensions: Option<(f32, f32)>,
}

/// 链接族: <a>, 及所有有 href/src 属性的元素
pub struct LinkFamily;
impl TagFamily for LinkFamily {
    const NAME: &'static str = "link";
    type Data = LinkFamilyData;
}

#[derive(Clone, Default)]
pub struct LinkFamilyData {
    pub link_type: LinkType,
    pub original_href: Option<String>,
    pub processed: bool,
}

#[derive(Clone, Default)]
pub enum LinkType {
    #[default]
    None,
    Absolute,   // /path
    Relative,   // ./file, ../file
    Fragment,   // #anchor
    External,   // https://...
}

/// 标题族: <h1> - <h6>
pub struct HeadingFamily;
impl TagFamily for HeadingFamily {
    const NAME: &'static str = "heading";
    type Data = HeadingFamilyData;
}

#[derive(Clone, Default)]
pub struct HeadingFamilyData {
    pub level: u8,              // 1-6
    pub original_id: Option<String>,
    pub slugified_id: Option<String>,
}

/// 媒体族: <img>, <video>, <audio>
pub struct MediaFamily;
impl TagFamily for MediaFamily {
    const NAME: &'static str = "media";
    type Data = MediaFamilyData;
}

#[derive(Clone, Default)]
pub struct MediaFamilyData {
    pub src: Option<String>,
    pub is_svg_image: bool,     // 用于 color-invert 判断
}

/// 其他元素
pub struct OtherFamily;
impl TagFamily for OtherFamily {
    const NAME: &'static str = "other";
    type Data = ();
}

// ════════════════════════════════════════════════════════════════════════
// 运行时族识别
// ════════════════════════════════════════════════════════════════════════

pub fn identify_family(tag: &str, attrs: &Attrs) -> &'static str {
    match tag {
        // SVG 族
        "svg" | "g" | "path" | "circle" | "rect" | "line" | "polyline"
        | "polygon" | "ellipse" | "text" | "tspan" | "defs" | "use"
        | "clipPath" | "mask" | "pattern" | "image" | "foreignObject"
        | "linearGradient" | "radialGradient" | "stop" | "symbol" => "svg",

        // 标题族
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",

        // 媒体族
        "img" | "video" | "audio" | "source" | "track" | "picture" => "media",

        // 链接族 - 需要检查属性
        "a" => "link",
        _ if attrs.has_any(&["href", "src"]) => "link",

        // 其他
        _ => "other",
    }
}
```

### 2.2 Phase 系统

```rust
/// Phase trait - 阶段标记，零运行时开销
pub trait Phase: 'static + Clone + Send + Sync {
    const NAME: &'static str;
}

/// PhaseData - 阶段扩展数据
/// 使用 GAT 实现按标签族特化
pub trait PhaseData: Phase {
    /// 文档级扩展
    type DocExt: Clone + Send + Sync + Default;

    /// 元素级扩展 - 按标签族特化（真正的 GAT 用法）
    type ElemExt<F: TagFamily>: Clone + Send + Sync + Default;

    /// 文本节点扩展
    type TextExt: Clone + Send + Sync + Default;

    /// Frame 节点扩展（Typst 特有）
    type FrameExt: Clone + Send + Sync;
}
```

### 2.3 阶段定义

```rust
// ════════════════════════════════════════════════════════════════════════
// Phase 1: Raw - 直接从 typst-html 转换
// ════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct Raw;

impl Phase for Raw {
    const NAME: &'static str = "raw";
}

impl PhaseData for Raw {
    type DocExt = RawDocExt;
    type ElemExt<F: TagFamily> = ();  // Raw 阶段无元素扩展
    type TextExt = ();
    type FrameExt = RawFrameExt;
}

#[derive(Clone, Default, Debug)]
pub struct RawDocExt {
    pub source_path: PathBuf,
    pub dependencies: Vec<PathBuf>,
    pub content_meta: Option<ContentMeta>,
    pub is_index: bool,
}

#[derive(Clone, Debug)]
pub struct RawFrameExt {
    // 保持对原始 Frame 的引用，延迟渲染
    // 注意：实际实现可能需要 Arc 来共享
}

// ════════════════════════════════════════════════════════════════════════
// Phase 2: Indexed - 节点已分配 ID，建立索引
// ════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct Indexed;

impl Phase for Indexed {
    const NAME: &'static str = "indexed";
}

impl PhaseData for Indexed {
    type DocExt = IndexedDocExt;
    type ElemExt<F: TagFamily> = IndexedElemExt<F>;
    type TextExt = NodeId;
    type FrameExt = IndexedFrameExt;
}

#[derive(Clone, Default, Debug)]
pub struct IndexedDocExt {
    pub base: RawDocExt,
    pub node_count: u32,
    /// 按族分类的节点 ID 列表（用于快速访问）
    pub svg_nodes: Vec<NodeId>,
    pub link_nodes: Vec<NodeId>,
    pub heading_nodes: Vec<NodeId>,
    pub media_nodes: Vec<NodeId>,
    pub frame_nodes: Vec<NodeId>,
}

/// 带族特定数据的元素扩展
#[derive(Clone, Default, Debug)]
pub struct IndexedElemExt<F: TagFamily> {
    pub node_id: NodeId,
    pub family_data: F::Data,  // 族特定数据
}

#[derive(Clone, Debug)]
pub struct IndexedFrameExt {
    pub node_id: NodeId,
    pub estimated_svg_size: usize,
}

// ════════════════════════════════════════════════════════════════════════
// Phase 3: Processed - 所有转换已应用
// ════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct Processed;

impl Phase for Processed {
    const NAME: &'static str = "processed";
}

impl PhaseData for Processed {
    type DocExt = ProcessedDocExt;
    type ElemExt<F: TagFamily> = ProcessedElemExt;
    type TextExt = ();
    type FrameExt = std::convert::Infallible;  // Frame 已全部转换为 Element
}

#[derive(Clone, Default, Debug)]
pub struct ProcessedDocExt {
    pub head_injections: Vec<HeadInjection>,
    pub extracted_assets: Vec<ExtractedAsset>,
}

#[derive(Clone, Default, Debug)]
pub struct ProcessedElemExt {
    pub modified: bool,
}

#[derive(Clone, Debug)]
pub enum HeadInjection {
    Title(String),
    Meta { name: String, content: String },
    Stylesheet { href: String, inline: Option<String> },
    Script { src: Option<String>, content: Option<String>, defer: bool },
    Raw(String),
}

#[derive(Clone, Debug)]
pub struct ExtractedAsset {
    pub kind: AssetKind,
    pub content: Vec<u8>,
    pub output_path: PathBuf,
    pub url: String,
}

#[derive(Clone, Debug)]
pub enum AssetKind {
    Svg,
    Avif,
    Svgz,
}
```

### 2.4 VDOM 节点

#### 2.4.1 零开销族扩展（enum dispatch 替代 Box<dyn Any>）

```rust
/// 族扩展枚举 - 编译期确定，零运行时开销
///
/// 关键设计：用 enum 替代 Box<dyn Any>
/// - ✅ 栈分配（无堆开销）
/// - ✅ 编译期大小已知
/// - ✅ 模式匹配（无 downcast 开销）
/// - ✅ 完全内联优化
#[derive(Clone, Debug)]
pub enum FamilyExt<P: PhaseData> {
    Svg(P::ElemExt<SvgFamily>),
    Link(P::ElemExt<LinkFamily>),
    Heading(P::ElemExt<HeadingFamily>),
    Media(P::ElemExt<MediaFamily>),
    Other(P::ElemExt<OtherFamily>),
}

impl<P: PhaseData> FamilyExt<P> {
    /// 获取族名
    pub fn family_name(&self) -> &'static str {
        match self {
            Self::Svg(_) => SvgFamily::NAME,
            Self::Link(_) => LinkFamily::NAME,
            Self::Heading(_) => HeadingFamily::NAME,
            Self::Media(_) => MediaFamily::NAME,
            Self::Other(_) => OtherFamily::NAME,
        }
    }
}

impl<P: PhaseData> Default for FamilyExt<P> {
    fn default() -> Self {
        Self::Other(Default::default())
    }
}
```

#### 2.4.2 节点类型

```rust
/// 节点 ID - 用于索引和跨阶段引用
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct NodeId(pub u32);

/// VDOM 节点 - 完整映射 typst-html::HtmlNode
#[derive(Clone, Debug)]
pub enum Node<P: PhaseData> {
    /// 元素节点
    Element(Element<P>),
    /// 文本节点
    Text(Text<P>),
    /// Typst 渲染帧（图表、公式等）
    /// 在 Processed 阶段会被转换为 Element
    Frame(Frame<P>),
    // 注意：HtmlNode::Tag 是 introspection 标记，转换时过滤
}

/// 元素节点 - 零开销设计
///
/// 关键改进：ext 使用 FamilyExt 枚举而非 Box<dyn Any>
/// 这确保：
/// - 无堆分配（栈上 enum）
/// - 无虚函数表开销
/// - 编译期类型检查
#[derive(Clone, Debug)]
pub struct Element<P: PhaseData> {
    pub tag: CompactString,
    pub attrs: Attrs,
    pub children: Vec<Node<P>>,
    pub span: Option<u64>,
    /// 族特定扩展数据（enum dispatch，非类型擦除）
    pub ext: FamilyExt<P>,
}

impl<P: PhaseData> Element<P> {
    /// 创建 SVG 族元素
    pub fn svg(
        tag: impl Into<CompactString>,
        attrs: Attrs,
        children: Vec<Node<P>>,
        ext: P::ElemExt<SvgFamily>,
    ) -> Self {
        Self { tag: tag.into(), attrs, children, span: None, ext: FamilyExt::Svg(ext) }
    }

    /// 创建 Link 族元素
    pub fn link(
        tag: impl Into<CompactString>,
        attrs: Attrs,
        children: Vec<Node<P>>,
        ext: P::ElemExt<LinkFamily>,
    ) -> Self {
        Self { tag: tag.into(), attrs, children, span: None, ext: FamilyExt::Link(ext) }
    }

    /// 创建 Heading 族元素
    pub fn heading(
        tag: impl Into<CompactString>,
        attrs: Attrs,
        children: Vec<Node<P>>,
        ext: P::ElemExt<HeadingFamily>,
    ) -> Self {
        Self { tag: tag.into(), attrs, children, span: None, ext: FamilyExt::Heading(ext) }
    }

    /// 创建 Media 族元素
    pub fn media(
        tag: impl Into<CompactString>,
        attrs: Attrs,
        children: Vec<Node<P>>,
        ext: P::ElemExt<MediaFamily>,
    ) -> Self {
        Self { tag: tag.into(), attrs, children, span: None, ext: FamilyExt::Media(ext) }
    }

    /// 创建 Other 族元素
    pub fn other(
        tag: impl Into<CompactString>,
        attrs: Attrs,
        children: Vec<Node<P>>,
        ext: P::ElemExt<OtherFamily>,
    ) -> Self {
        Self { tag: tag.into(), attrs, children, span: None, ext: FamilyExt::Other(ext) }
    }

    /// 获取族标识
    #[inline]
    pub fn family(&self) -> &'static str {
        self.ext.family_name()
    }

    /// 尝试获取 SVG 族扩展
    #[inline]
    pub fn as_svg(&self) -> Option<&P::ElemExt<SvgFamily>> {
        match &self.ext { FamilyExt::Svg(e) => Some(e), _ => None }
    }

    /// 尝试获取 Link 族扩展
    #[inline]
    pub fn as_link(&self) -> Option<&P::ElemExt<LinkFamily>> {
        match &self.ext { FamilyExt::Link(e) => Some(e), _ => None }
    }

    /// 尝试获取 Heading 族扩展
    #[inline]
    pub fn as_heading(&self) -> Option<&P::ElemExt<HeadingFamily>> {
        match &self.ext { FamilyExt::Heading(e) => Some(e), _ => None }
    }

    /// 尝试获取可变 SVG 族扩展
    #[inline]
    pub fn as_svg_mut(&mut self) -> Option<&mut P::ElemExt<SvgFamily>> {
        match &mut self.ext { FamilyExt::Svg(e) => Some(e), _ => None }
    }

    /// 尝试获取可变 Link 族扩展
    #[inline]
    pub fn as_link_mut(&mut self) -> Option<&mut P::ElemExt<LinkFamily>> {
        match &mut self.ext { FamilyExt::Link(e) => Some(e), _ => None }
    }
}

/// 文本节点
#[derive(Clone, Debug)]
pub struct Text<P: PhaseData> {
    pub content: CompactString,
    pub span: Option<u64>,
    pub ext: P::TextExt,
}

/// Frame 节点
#[derive(Clone, Debug)]
pub struct Frame<P: PhaseData> {
    pub ext: P::FrameExt,
    // 实际的 Frame 数据通过 ext 访问
}

/// 文档
#[derive(Clone, Debug)]
pub struct Document<P: PhaseData> {
    pub root: Element<P>,
    pub ext: P::DocExt,
}
```

## 3. Visitor/Folder 模式

### 3.1 Visitor（只读遍历）

```rust
/// 只读访问者 - 用于收集信息
pub trait Visitor<P: PhaseData> {
    /// 访问元素前，返回是否继续遍历子节点
    fn visit_element(&mut self, _elem: &Element<P>) -> bool {
        true  // 默认继续
    }

    /// 访问元素后（仅当 visit_element 返回 true 时调用）
    fn leave_element(&mut self, _elem: &Element<P>) {}

    /// 访问文本节点
    fn visit_text(&mut self, _text: &Text<P>) {}

    /// 访问 Frame 节点
    fn visit_frame(&mut self, _frame: &Frame<P>) {}
}

/// 执行遍历（零拷贝，传递引用）
pub fn visit<P: PhaseData, V: Visitor<P>>(doc: &Document<P>, visitor: &mut V) {
    visit_element_ref(&doc.root, visitor);
}

fn visit_element_ref<P: PhaseData, V: Visitor<P>>(elem: &Element<P>, visitor: &mut V) {
    let should_descend = visitor.visit_element(elem);
    if should_descend {
        for child in &elem.children {
            visit_node_ref(child, visitor);
        }
        // leave_element 仅在遍历子节点后调用
        visitor.leave_element(elem);
    }
}

fn visit_node_ref<P: PhaseData, V: Visitor<P>>(node: &Node<P>, visitor: &mut V) {
    match node {
        Node::Element(elem) => visit_element_ref(elem, visitor),
        Node::Text(text) => visitor.visit_text(text),
        Node::Frame(frame) => visitor.visit_frame(frame),
    }
}
```

### 3.2 Folder（变换遍历）

```rust
/// 变换折叠器 - 用于阶段转换
pub trait Folder<From: PhaseData, To: PhaseData> {
    /// 转换文档扩展
    fn fold_doc_ext(&mut self, ext: From::DocExt) -> To::DocExt;

    /// 转换元素（需要处理族特定数据）
    fn fold_element(&mut self, elem: Element<From>) -> Element<To>;

    /// 转换文本
    fn fold_text(&mut self, text: Text<From>) -> Text<To>;

    /// 转换 Frame - 可能变成 Element 或多个节点
    /// 注意：返回 Vec 以支持展开为多个节点
    fn fold_frame(&mut self, frame: Frame<From>) -> Vec<Node<To>>;

    /// 转换节点（默认分发）
    fn fold_node(&mut self, node: Node<From>) -> Vec<Node<To>> {
        match node {
            Node::Element(e) => vec![Node::Element(self.fold_element(e))],
            Node::Text(t) => vec![Node::Text(self.fold_text(t))],
            Node::Frame(f) => self.fold_frame(f),
        }
    }

    /// 转换子节点列表（默认递归，展平多结果）
    fn fold_children(&mut self, children: Vec<Node<From>>) -> Vec<Node<To>> {
        children.into_iter().flat_map(|n| self.fold_node(n)).collect()
    }
}

/// 执行折叠变换
pub fn fold<From, To, F>(doc: Document<From>, folder: &mut F) -> Document<To>
where
    From: PhaseData,
    To: PhaseData,
    F: Folder<From, To>,
{
    Document {
        root: folder.fold_element(doc.root),
        ext: folder.fold_doc_ext(doc.ext),
    }
}
```

### 3.3 MutVisitor（原地修改）

```rust
/// 原地修改访问者 - 用于同阶段转换
pub trait MutVisitor<P: PhaseData> {
    /// 修改元素，返回是否继续遍历子节点
    fn visit_element_mut(&mut self, elem: &mut Element<P>) -> bool {
        true
    }

    /// 修改文本
    fn visit_text_mut(&mut self, text: &mut Text<P>) {}

    /// 修改 Frame
    fn visit_frame_mut(&mut self, frame: &mut Frame<P>) {}
}

/// 执行原地修改遍历
pub fn visit_mut<P: PhaseData, V: MutVisitor<P>>(doc: &mut Document<P>, visitor: &mut V) {
    visit_node_mut(&mut Node::Element(std::mem::take(&mut doc.root)), visitor);
    // ... restore root
}
```

## 4. Transform 系统

### 4.1 核心 Trait

```rust
/// 阶段转换
pub trait Transform {
    type From: PhaseData;
    type To: PhaseData;
    type Config: Send + Sync;
    type Error: std::error::Error + Send + Sync;

    fn transform(
        &self,
        doc: Document<Self::From>,
        config: &Self::Config,
    ) -> Result<Document<Self::To>, Self::Error>;

    fn name(&self) -> &'static str;
}

/// 同阶段原地转换
pub trait InPlaceTransform<P: PhaseData> {
    type Config: Send + Sync;
    type Error: std::error::Error + Send + Sync;

    fn transform_in_place(
        &self,
        doc: &mut Document<P>,
        config: &Self::Config,
    ) -> Result<(), Self::Error>;

    fn name(&self) -> &'static str;
}
```

### 4.2 零开销 Pipeline

```rust
/// 零开销 Pipeline - 使用泛型链而非动态分发
///
/// 编译后为直接函数调用链，无运行时开销
pub struct Pipeline<Start, End, Steps> {
    steps: Steps,
    _phantom: PhantomData<(Start, End)>,
}

/// 空 Pipeline
pub struct EmptyPipeline;

/// Pipeline 步骤
pub struct Step<T, Rest> {
    transform: T,
    rest: Rest,
}

impl<P: PhaseData> Pipeline<P, P, EmptyPipeline> {
    pub fn new() -> Self {
        Pipeline {
            steps: EmptyPipeline,
            _phantom: PhantomData,
        }
    }
}

impl<Start: PhaseData, Current: PhaseData, Steps> Pipeline<Start, Current, Steps> {
    /// 添加阶段转换
    pub fn then<T>(self, transform: T) -> Pipeline<Start, T::To, Step<T, Steps>>
    where
        T: Transform<From = Current>,
    {
        Pipeline {
            steps: Step {
                transform,
                rest: self.steps,
            },
            _phantom: PhantomData,
        }
    }
}

/// Pipeline 执行 trait
pub trait Execute<Start: PhaseData> {
    type End: PhaseData;
    type Config;
    type Error;

    fn execute(
        self,
        doc: Document<Start>,
        config: &Self::Config,
    ) -> Result<Document<Self::End>, Self::Error>;
}

impl<P: PhaseData> Execute<P> for EmptyPipeline {
    type End = P;
    type Config = ();
    type Error = std::convert::Infallible;

    fn execute(self, doc: Document<P>, _: &()) -> Result<Document<P>, Self::Error> {
        Ok(doc)
    }
}

impl<T, Rest, Start> Execute<Start> for Step<T, Rest>
where
    Start: PhaseData,
    T: Transform<From = Start>,
    Rest: Execute<T::To>,
{
    type End = Rest::End;
    type Config = (T::Config, Rest::Config);
    type Error = PipelineError<T::Error, Rest::Error>;

    fn execute(
        self,
        doc: Document<Start>,
        config: &Self::Config,
    ) -> Result<Document<Self::End>, Self::Error> {
        let intermediate = self.transform
            .transform(doc, &config.0)
            .map_err(PipelineError::Current)?;
        self.rest
            .execute(intermediate, &config.1)
            .map_err(PipelineError::Rest)
    }
}

#[derive(Debug)]
pub enum PipelineError<E1, E2> {
    Current(E1),
    Rest(E2),
}
```

### 4.3 Transform 实现示例

```rust
// ════════════════════════════════════════════════════════════════════════
// Indexer: Raw → Indexed
// ════════════════════════════════════════════════════════════════════════

pub struct Indexer;

impl Transform for Indexer {
    type From = Raw;
    type To = Indexed;
    type Config = ();
    type Error = std::convert::Infallible;

    fn transform(
        &self,
        doc: Document<Raw>,
        _: &(),
    ) -> Result<Document<Indexed>, Self::Error> {
        let mut folder = IndexerFolder::new();
        Ok(fold(doc, &mut folder))
    }

    fn name(&self) -> &'static str { "indexer" }
}

struct IndexerFolder {
    next_id: u32,
    doc_ext: IndexedDocExt,
}

impl IndexerFolder {
    fn new() -> Self {
        Self {
            next_id: 0,
            doc_ext: IndexedDocExt::default(),
        }
    }

    fn next_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }
}

impl Folder<Raw, Indexed> for IndexerFolder {
    fn fold_doc_ext(&mut self, ext: RawDocExt) -> IndexedDocExt {
        self.doc_ext.base = ext;
        self.doc_ext.node_count = self.next_id;
        std::mem::take(&mut self.doc_ext)
    }

    fn fold_element(&mut self, elem: Element<Raw>) -> Element<Indexed> {
        let node_id = self.next_node_id();
        let family = identify_family(&elem.tag, &elem.attrs);

        // 记录到族列表
        match family {
            "svg" => self.doc_ext.svg_nodes.push(node_id),
            "link" => self.doc_ext.link_nodes.push(node_id),
            "heading" => self.doc_ext.heading_nodes.push(node_id),
            "media" => self.doc_ext.media_nodes.push(node_id),
            _ => {}
        }

        // 构建族特定扩展...
        let children = self.fold_children(elem.children);

        // 根据 family 创建正确类型的扩展
        match family {
            "svg" => Element::new::<SvgFamily>(
                elem.tag,
                elem.attrs,
                children,
                IndexedElemExt {
                    node_id,
                    family_data: SvgFamilyData {
                        is_root: true,  // 顶层 SVG
                        viewbox: parse_viewbox(&elem.attrs),
                        dimensions: None,
                    },
                },
            ),
            // ... 其他族
            _ => Element::new::<OtherFamily>(
                elem.tag,
                elem.attrs,
                children,
                IndexedElemExt {
                    node_id,
                    family_data: (),
                },
            ),
        }
    }

    fn fold_text(&mut self, text: Text<Raw>) -> Text<Indexed> {
        Text {
            content: text.content,
            span: text.span,
            ext: self.next_node_id(),
        }
    }

    fn fold_frame(&mut self, frame: Frame<Raw>) -> Node<Indexed> {
        let node_id = self.next_node_id();
        self.doc_ext.frame_nodes.push(node_id);

        Node::Frame(Frame {
            ext: IndexedFrameExt {
                node_id,
                estimated_svg_size: 0, // 实际实现会估算大小
            },
        })
    }
}
```

## 5. typst-html 转换

```rust
/// 从 typst-html 构建 VDOM
///
/// 关键：
/// - 过滤 HtmlNode::Tag（introspection 标记）
/// - 保持 HtmlNode::Frame（延迟渲染）
pub fn from_typst_html(
    doc: &typst_library::html::HtmlDocument,
    source: PathBuf,
    deps: Vec<PathBuf>,
    meta: Option<ContentMeta>,
) -> Document<Raw> {
    fn convert_node(node: &typst_library::html::HtmlNode) -> Option<Node<Raw>> {
        use typst_library::html::HtmlNode;

        match node {
            // 过滤 introspection 标记
            HtmlNode::Tag(_) => None,

            // 文本节点
            HtmlNode::Text(text, span) => Some(Node::Text(Text {
                content: text.as_str().into(),
                span: Some(span.into_raw()),
                ext: (),
            })),

            // 元素节点
            HtmlNode::Element(elem) => {
                let children: Vec<_> = elem.children
                    .iter()
                    .filter_map(convert_node)
                    .collect();

                Some(Node::Element(Element::new::<OtherFamily>(
                    elem.tag.as_str(),
                    convert_attrs(&elem.attrs),
                    children,
                    (),
                )))
            }

            // Frame - 保持原样
            HtmlNode::Frame(frame) => Some(Node::Frame(Frame {
                ext: RawFrameExt {
                    // 存储 Frame 引用
                },
            })),
        }
    }

    let is_index = source.file_stem()
        .map(|s| s == "index")
        .unwrap_or(false);

    let root = convert_node(&typst_library::html::HtmlNode::Element(doc.root.clone()))
        .and_then(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
        .expect("Root should be an element");

    Document {
        root,
        ext: RawDocExt {
            source_path: source,
            dependencies: deps,
            content_meta: meta,
            is_index,
        },
    }
}

fn convert_attrs(attrs: &typst_library::html::HtmlAttrs) -> Attrs {
    let items: SmallVec<[Attr; 4]> = attrs.0
        .iter()
        .map(|(name, value)| Attr {
            name: AttrName::from(name.as_str()),
            value: AttrValue::String(value.as_str().into()),
        })
        .collect();
    Attrs(items)
}
```

### 5.1 Frame 处理详解 🔑

#### 5.1.1 typst Frame 是什么？

`typst::layout::Frame` 是 Typst 渲染的核心输出，包含：
- 图表（通过 `cetz`, `diagraph` 等包）
- 数学公式
- 自定义图形

```rust
// typst-library/src/layout/frame.rs（简化）
pub struct Frame {
    /// 帧尺寸
    size: Size,
    /// 帧内容
    items: Vec<(Point, FrameItem)>,
}

pub enum FrameItem {
    /// 文本分组
    Group(GroupItem),
    /// 形状
    Shape(Shape, Option<Paint>),
    /// 图片
    Image(Image, Size, Option<Paint>),
    /// 嵌套帧
    Frame(Frame),
    // ...
}
```

#### 5.1.2 Frame → SVG 的真实流程

```rust
use typst_render::render_svg;
use typst_library::layout::Frame;

/// FrameExpander 的真实实现
impl Folder<Indexed, Processed> for FrameExpanderFolder {
    fn fold_frame(&mut self, frame: Frame<Indexed>) -> Node<Processed> {
        // 1. 获取原始 typst Frame（通过 Arc 共享）
        let typst_frame: &typst::layout::Frame = &frame.ext.frame_ref;

        // 2. 调用 typst-render 生成 SVG
        let svg_string = typst_render::render_svg(typst_frame);

        // 3. 可选：使用 usvg 优化
        let optimized = optimize_svg(&svg_string);

        // 4. 解析 SVG 为 DOM 节点
        let svg_element = parse_svg_to_element(&optimized);

        self.frames_expanded += 1;
        self.svg_count += 1;
        self.total_svg_bytes += optimized.len();

        Node::Element(svg_element)
    }
}

/// 解析 SVG 字符串为 Element
fn parse_svg_to_element(svg: &str) -> Element<Processed> {
    // 使用 quick_xml 或手动解析
    // 关键：不需要完整 XML 解析，只需提取根 <svg> 属性和内容
    let attrs = extract_svg_attrs(svg);
    let inner_html = extract_svg_content(svg);

    Element {
        tag: "svg".into(),
        attrs,
        children: vec![Node::Text(Text {
            content: inner_html.into(),
            ext: (),
        })],
        ext: FamilyExt::Svg(ProcessedElemExt { modified: true }),
    }
}
```

#### 5.1.3 延迟渲染的意义

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        为什么延迟 Frame 渲染？                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  问题：typst_html::html() 在序列化时立即渲染所有 Frame                        │
│        但此时我们还需要对生成的 SVG 做处理（优化、提取、压缩）                    │
│        如果立即序列化，之后还要再解析 SVG → 双重开销                            │
│                                                                             │
│  方案：                                                                      │
│  ┌──────────────────┐   ┌──────────────────┐   ┌──────────────────┐        │
│  │ typst_html       │   │ TTG Pipeline     │   │ HtmlRenderer     │        │
│  │ realize()        │──▶│ Frame 保持原样   │──▶│ Frame→SVG        │        │
│  │ Frame 结构保持   │   │ 其他节点处理     │   │ 最后统一渲染      │        │
│  └──────────────────┘   └──────────────────┘   └──────────────────┘        │
│                                                                             │
│  优点：                                                                      │
│  ✅ SVG 只生成一次（在需要时）                                                │
│  ✅ 可以批量优化所有 SVG                                                      │
│  ✅ 可以决定是否内联或提取为外部文件                                           │
│  ✅ 避免双重解析（生成时不解析，直接嵌入最终 HTML）                             │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 5.1.4 RawFrameExt 的真实结构

```rust
/// Raw 阶段的 Frame 扩展 - 持有 typst Frame 的引用
#[derive(Clone)]
pub struct RawFrameExt {
    /// 共享的 typst Frame 引用
    /// 使用 Arc 因为 Frame 可能很大，避免深拷贝
    pub frame_ref: Arc<typst::layout::Frame>,
    /// 帧的原始尺寸（pt）
    pub size: (f32, f32),
    /// 源码位置（用于错误报告）
    pub span: Option<u64>,
}

impl Debug for RawFrameExt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawFrameExt")
            .field("size", &self.size)
            .field("span", &self.span)
            .finish_non_exhaustive() // 不打印 frame_ref 的全部内容
    }
}
```

---

## 6. 两阶段编译与虚拟文件系统集成 ⚠️ 关键

### 6.1 现状

Tola SSG 采用**两阶段编译**来支持虚拟数据文件（`/_data/pages.json`, `/_data/tags.json`）:

```
Phase 1: Metadata Collection
  ┌─────────────────────────────────────────────────────────────────┐
  │ for each .typ file:                                             │
  │   compile() → typst_html                                        │
  │             → from_typst_html()                        [Raw]    │
  │             → extract metadata only                             │
  │             → GLOBAL_SITE_DATA.insert(metadata)                │
  │             → discard HTML (incomplete, virtual JSON empty)     │
  │                                                                 │
  │ Result: GLOBAL_SITE_DATA.pages, GLOBAL_SITE_DATA.tags populated │
  └─────────────────────────────────────────────────────────────────┘
           ↓

Phase 2: HTML Generation
  ┌─────────────────────────────────────────────────────────────────┐
  │ for each .typ file:                                             │
  │   compile() → typst_html                                        │
  │             → from_typst_html()                        [Raw]    │
  │             → Pipeline transforms             [Indexed,...]    │
  │             → HtmlRenderer → write to disk                      │
  │             (now /_data/*.json returns COMPLETE data)           │
  └─────────────────────────────────────────────────────────────────┘
```

### 6.2 TTG Pipeline 与两阶段的配合

**问题**：TTG Pipeline 需要在两个不同的编译阶段运行

**方案**：

```rust
/// Phase 1: 提取元数据，丢弃 HTML
pub fn extract_metadata(doc: Document<Raw>) -> ContentMeta {
    // 只需要执行到某个中间阶段
    // 不需要完整的 link transform、frame expand 等
    let indexed = Indexer.transform(doc);
    // 提取元数据（来自 tola-meta 标签）
    indexed.ext.content_meta
}

/// Phase 2: 生成完整 HTML（虚拟数据已可用）
pub fn render_html(doc: Document<Raw>) -> Vec<u8> {
    // 完整的 pipeline
    let indexed = Indexer.transform(doc);
    let processed = FrameExpander.transform(indexed);
    let processed = LinkProcessor.transform(processed);  // 依赖 GLOBAL_SITE_DATA
    let processed = HeadingProcessor.transform(processed);
    let processed = HeadInjector.transform(processed);    // 依赖 GLOBAL_SITE_DATA
    HtmlRenderer.transform(processed)
}
```

### 6.3 GLOBAL_SITE_DATA 的 Pipeline 依赖

某些 Transform 需要读取全局数据:

| Transform | 依赖 | 阶段 |
|-----------|------|------|
| `LinkProcessor` | ❌ None | Phase 1/2 |
| `HeadingProcessor` | ❌ None | Phase 1/2 |
| `HeadInjector` | ✅ tags, pages (for TOC, breadcrumb) | Phase 2 only |
| `FrameExpander` | ❌ None (Frame → SVG 是静态的) | Phase 1/2 |

### 6.4 集成建议

```rust
// src/compiler/pages.rs - Phase 1 集成
pub fn collect_metadata(...) -> Result<...> {
    for page in typ_files {
        let html_bytes = compile_meta(page, config)?;  // HtmlDocument
        let doc = from_typst_html(html_bytes, ...);    // Document<Raw>

        // 只执行部分 pipeline，快速路径
        let metadata = extract_metadata(doc);
        GLOBAL_SITE_DATA.insert_page(metadata);
    }
}

// src/compiler/pages.rs - Phase 2 集成
pub fn compile_pages_with_data(...) -> Result<...> {
    for page in typ_files {
        let html_bytes = compile_meta(page, config)?;
        let doc = from_typst_html(html_bytes, ...);

        // 完整的 pipeline，此时 GLOBAL_SITE_DATA 已可用
        let html = render_html(doc);
        fs::write(&page_path, html)?;
    }
}
```

**重要**：HeadInjector 必须在 Phase 2，因为它需要 GLOBAL_SITE_DATA 完整数据

### 6.5 virtual_fs 与 Pipeline 的具体集成 🔑

**目的**：明确 `src/data/virtual_fs.rs` 如何与 TTG Pipeline 交互

#### 6.5.1 现有 virtual_fs 模块结构

```rust
// src/data/virtual_fs.rs - 现有实现
pub const VIRTUAL_DATA_DIR: &str = "/_data";

/// 检查是否虚拟数据路径
pub fn is_virtual_data_path(path: &Path) -> bool { ... }

/// 读取虚拟数据（返回 JSON bytes）
pub fn read_virtual_data(path: &Path) -> Option<Vec<u8>> { ... }

/// 获取所有虚拟文件路径
pub fn virtual_data_paths() -> Vec<PathBuf> { ... }
```

#### 6.5.2 HeadInjector 如何使用 GLOBAL_SITE_DATA

```rust
// src/transform/head.rs
use crate::data::store::GLOBAL_SITE_DATA;

pub struct HeadInjector;

impl Transform<Processed, Processed> for HeadInjector {
    fn transform(&self, doc: Document<Processed>) -> Document<Processed> {
        let mut doc = doc;

        // 1. 收集当前页面的元数据
        let page_meta = &doc.ext.content_meta;

        // 2. 从 GLOBAL_SITE_DATA 获取导航数据
        let nav_data = self.build_navigation_data(page_meta);

        // 3. 生成 <head> 注入内容
        let head_elements = self.generate_head_elements(page_meta, &nav_data);

        // 4. 使用 MutVisitor 注入到 <head>
        let mut injector = HeadInjectorVisitor { elements: head_elements };
        visit_mut(&mut doc, &mut injector);

        doc
    }
}

impl HeadInjector {
    fn build_navigation_data(&self, current: &ContentMeta) -> NavigationData {
        // 从 GLOBAL_SITE_DATA 获取页面列表（用于 breadcrumb）
        let pages = GLOBAL_SITE_DATA.get_pages();

        // 获取标签索引（用于相关文章）
        let tags_index = GLOBAL_SITE_DATA.get_tags_index();

        // 构建当前页面的 breadcrumb
        let breadcrumb = self.compute_breadcrumb(&pages, current);

        // 构建相关文章列表
        let related = self.find_related_articles(&tags_index, current);

        NavigationData { breadcrumb, related }
    }

    fn generate_head_elements(
        &self,
        meta: &ContentMeta,
        nav: &NavigationData,
    ) -> Vec<Node<Processed>> {
        let mut elements = vec![];

        // Open Graph
        elements.push(self.og_meta("og:title", &meta.title));
        elements.push(self.og_meta("og:description", meta.description.as_deref().unwrap_or("")));

        // JSON-LD Breadcrumb
        if !nav.breadcrumb.is_empty() {
            elements.push(self.jsonld_breadcrumb(&nav.breadcrumb));
        }

        // Canonical URL
        if let Some(url) = &meta.canonical_url {
            elements.push(self.canonical_link(url));
        }

        elements
    }
}
```

#### 6.5.3 依赖追踪与 HeadInjector

```rust
// HeadInjector 运行时，自动记录依赖
impl HeadInjector {
    fn transform_with_deps(
        &self,
        doc: Document<Processed>,
        deps: &mut Vec<Dependency>,
    ) -> Document<Processed> {
        // 访问 GLOBAL_SITE_DATA 意味着依赖虚拟文件
        deps.push(Dependency::Virtual(VirtualFile::PagesJson));
        deps.push(Dependency::Virtual(VirtualFile::TagsJson));

        // 实际 transform
        self.transform(doc)
    }
}
```

#### 6.5.4 完整的 Pipeline 依赖记录

```rust
// src/transform/pipeline.rs
pub struct PipelineContext {
    /// 收集的依赖（用于增量构建）
    pub dependencies: Vec<Dependency>,
    /// 当前页面路径
    pub source_path: PathBuf,
}

impl PipelineContext {
    /// 运行完整 pipeline 并收集依赖
    pub fn run_with_deps(&mut self, doc: Document<Raw>) -> Vec<u8> {
        // Phase 1 transforms（无虚拟数据依赖）
        let indexed = Indexer.transform(doc);
        let processed = FrameExpander.transform(indexed);
        let processed = LinkProcessor.transform(processed);
        let processed = HeadingProcessor.transform(processed);

        // Phase 2 only transform（有虚拟数据依赖）
        let processed = HeadInjector.transform_with_deps(processed, &mut self.dependencies);

        // 渲染
        HtmlRenderer.transform(processed)
    }
}
```

#### 6.5.5 Watch 模式下的虚拟数据更新

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Watch 模式虚拟数据更新流程                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. 用户修改 page_a.typ                                                     │
│     └─▶ 触发 watch event                                                   │
│                                                                             │
│  2. Phase 1 重跑 page_a                                                    │
│     └─▶ extract_metadata(page_a)                                           │
│     └─▶ GLOBAL_SITE_DATA.insert_page(new_meta)                             │
│     └─▶ JSON cache 失效                                                    │
│                                                                             │
│  3. 检查元数据是否有显著变化                                                  │
│     └─▶ is_metadata_change_significant(old, new)?                          │
│                                                                             │
│  4a. 无显著变化 → 只重编译 page_a                                            │
│      └─▶ Phase 2: render_html(page_a)                                      │
│                                                                             │
│  4b. 有显著变化（标题/标签/日期改变）→ 重编译所有依赖虚拟数据的页面             │
│      └─▶ dep_graph.on_virtual_data_change(PagesJson)                       │
│      └─▶ 返回 {page_a, page_b, page_c, ...}                                │
│      └─▶ Phase 2: 批量 render_html()                                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 6.5.6 GLOBAL_SITE_DATA 类型定义（现有）

```rust
// src/data/store.rs - 已存在的实现
pub static GLOBAL_SITE_DATA: LazyLock<SiteDataStore> = LazyLock::new(SiteDataStore::new);

pub struct SiteDataStore {
    pages: RwLock<BTreeMap<String, PageData>>,
    json_cache: RwLock<JsonCache>,  // 缓存避免 O(N²) 序列化
}

impl SiteDataStore {
    pub fn insert_page(&self, page: PageData);     // Phase 1 写入
    pub fn get_pages(&self) -> Vec<PageData>;      // Phase 2 读取
    pub fn get_tags_index(&self) -> TagsIndex;     // Phase 2 读取
    pub fn pages_to_json(&self) -> String;         // virtual_fs 调用
    pub fn tags_to_json(&self) -> String;          // virtual_fs 调用
    pub fn clear(&self);                           // Watch 模式重置
}
```

---

## 7. 模块结构

```
src/
├── vdom/                      # VDOM 核心
│   ├── mod.rs                 # 公开 API
│   ├── node.rs                # Node, Element, Text, Frame, Document
│   ├── phase.rs               # Phase, PhaseData, 阶段定义
│   ├── family.rs              # TagFamily, 族定义和数据
│   ├── attr.rs                # Attrs, AttrName, AttrValue
│   ├── visitor.rs             # Visitor trait
│   ├── folder.rs              # Folder trait
│   └── convert.rs             # from_typst_html()
│
├── transform/                 # Transform 系统
│   ├── mod.rs                 # Transform traits
│   ├── pipeline.rs            # 零开销 Pipeline
│   │
│   ├── indexer.rs             # Indexer: Raw → Indexed
│   ├── link.rs                # LinkProcessor (Indexed in-place)
│   ├── heading.rs             # HeadingProcessor (Indexed in-place)
│   ├── frame.rs               # FrameExpander: Indexed → Processed
│   ├── head.rs                # HeadInjector (Processed in-place) ⚠️ Phase 2 only
│   └── render.rs              # HtmlRenderer: Processed → bytes
│
└── ... existing modules (保持兼容)
```

## 8. 关键设计决策总结

| 决策 | 理由 |
|------|------|
| **TagFamily 而非单标签类型** | 减少类型爆炸，同族元素共享处理逻辑 |
| **保持 Frame 节点** | 延迟 SVG 渲染，避免双重处理 |
| **FamilyExt enum 而非 Box<dyn Any>** | 零开销，栈分配，编译期类型安全 |
| **Visitor/Folder 分离** | 只读遍历和变换分开，职责清晰 |
| **3 阶段而非 4 阶段** | Raw/Indexed/Processed 足够，避免过度设计 |
| **两部分 render_html()** | Phase 1 提取元数据，Phase 2 完整渲染（虚拟数据就绪） |

---

## 9. 增量构建与 DependencyGraph 🔑

### 9.1 依赖类型

```rust
/// 文件依赖类型
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Dependency {
    /// Typst 源文件
    TypstSource(PathBuf),
    /// Typst 包（来自 @preview 或本地）
    TypstPackage { name: String, version: String },
    /// 图片资源
    Image(PathBuf),
    /// 模板文件
    Template(PathBuf),
    /// 数据文件（YAML, JSON, TOML）
    Data(PathBuf),
    /// 虚拟文件（/_data/pages.json 等）
    Virtual(VirtualFile),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VirtualFile {
    PagesJson,
    TagsJson,
    SitemapXml,
    RssFeed,
}
```

### 9.2 依赖图结构

```rust
/// 增量构建依赖图
pub struct DependencyGraph {
    /// 页面 → 依赖
    page_deps: HashMap<PathBuf, HashSet<Dependency>>,
    /// 依赖 → 影响的页面（反向索引）
    dep_to_pages: HashMap<Dependency, HashSet<PathBuf>>,
    /// 文件修改时间缓存
    mtimes: HashMap<PathBuf, SystemTime>,
    /// 文件内容哈希（用于检测实际变更）
    hashes: HashMap<PathBuf, u64>,
}

impl DependencyGraph {
    /// 记录页面依赖
    pub fn add_dependency(&mut self, page: &Path, dep: Dependency) {
        self.page_deps.entry(page.to_path_buf())
            .or_default()
            .insert(dep.clone());
        self.dep_to_pages.entry(dep)
            .or_default()
            .insert(page.to_path_buf());
    }

    /// 获取需要重新编译的页面
    pub fn get_dirty_pages(&self, changed_files: &[PathBuf]) -> HashSet<PathBuf> {
        let mut dirty = HashSet::new();
        for file in changed_files {
            // 查找依赖此文件的所有页面
            let dep = self.file_to_dependency(file);
            if let Some(pages) = self.dep_to_pages.get(&dep) {
                dirty.extend(pages.iter().cloned());
            }
        }
        dirty
    }

    /// 检测变更（mtime + hash）
    pub fn detect_changes(&mut self) -> Vec<PathBuf> {
        let mut changed = Vec::new();
        for (path, old_mtime) in &self.mtimes {
            if let Ok(meta) = fs::metadata(path) {
                let new_mtime = meta.modified().unwrap();
                if new_mtime > *old_mtime {
                    // mtime 变了，检查 hash
                    let new_hash = hash_file(path);
                    if self.hashes.get(path) != Some(&new_hash) {
                        changed.push(path.clone());
                    }
                }
            }
        }
        changed
    }
}
```

### 9.3 依赖收集时机

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        依赖收集流程                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Phase 1: Typst 编译时                                                      │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │ typst::compile() 时，World trait 的 file() 方法被调用                  │  │
│  │ → 记录所有被访问的文件路径                                              │  │
│  │ → 包括 #import, #include, #image 等                                   │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  Phase 2: Pipeline 处理时                                                   │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │ HeadInjector 访问 GLOBAL_SITE_DATA                                    │  │
│  │ → 隐式依赖 VirtualFile::PagesJson, VirtualFile::TagsJson              │  │
│  │ → 任何页面元数据变化都可能影响 head 注入                                 │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  存储: RawDocExt.dependencies                                               │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │ pub struct RawDocExt {                                                │  │
│  │     pub source_path: PathBuf,                                        │  │
│  │     pub dependencies: Vec<Dependency>,  // ← 收集的依赖               │  │
│  │     ...                                                               │  │
│  │ }                                                                     │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 9.4 虚拟文件依赖的特殊处理

```rust
/// 虚拟文件变更检测
impl DependencyGraph {
    /// 当 GLOBAL_SITE_DATA 更新时，检测哪些页面需要重新编译
    pub fn on_virtual_data_change(&self, vf: VirtualFile) -> HashSet<PathBuf> {
        // 只有依赖此虚拟文件的页面需要重编译
        // 例如：只有使用 TOC 或 breadcrumb 的页面依赖 PagesJson
        self.dep_to_pages.get(&Dependency::Virtual(vf))
            .cloned()
            .unwrap_or_default()
    }

    /// 智能判断：元数据变化是否影响其他页面
    pub fn is_metadata_change_significant(
        &self,
        old_meta: &ContentMeta,
        new_meta: &ContentMeta,
    ) -> bool {
        // 标题变化 → 可能影响其他页面的 breadcrumb
        old_meta.title != new_meta.title ||
        // 标签变化 → 可能影响标签页
        old_meta.tags != new_meta.tags ||
        // 日期变化 → 可能影响列表排序
        old_meta.date != new_meta.date
    }
}
```

### 9.5 增量编译流程

```
文件变更
    │
    ▼
┌───────────────────┐
│ detect_changes()  │  检测 mtime + hash
└───────────────────┘
    │
    ▼
┌───────────────────┐
│ get_dirty_pages() │  通过反向索引找到受影响页面
└───────────────────┘
    │
    ▼
┌───────────────────────────────────────────────────────────────┐
│ 是否影响虚拟数据？                                              │
│                                                               │
│   YES → Phase 1 重跑所有 dirty pages                           │
│       → 更新 GLOBAL_SITE_DATA                                  │
│       → Phase 2 重跑所有依赖虚拟数据的页面                        │
│                                                               │
│   NO  → 只 Phase 2 重跑 dirty pages（跳过 Phase 1）              │
└───────────────────────────────────────────────────────────────┘
```

### 9.6 Watch 模式集成

```rust
// src/watch.rs
impl Watcher {
    pub fn on_file_change(&mut self, path: &Path) -> Result<()> {
        let dirty = self.dep_graph.get_dirty_pages(&[path.to_path_buf()]);

        if dirty.is_empty() {
            return Ok(());  // 无关文件变更
        }

        // 检查是否需要重跑 Phase 1
        let needs_phase1 = self.is_source_change(path) ||
                           self.is_data_change(path);

        if needs_phase1 {
            // 更新元数据
            for page in &dirty {
                let doc = self.compile_to_raw(page)?;
                let meta = extract_metadata(doc);
                self.update_site_data(page, meta);
            }
        }

        // Phase 2：渲染
        let affected = if needs_phase1 {
            self.dep_graph.on_virtual_data_change(VirtualFile::PagesJson)
        } else {
            dirty
        };

        for page in affected {
            let doc = self.compile_to_raw(&page)?;
            let html = render_html(doc, &self.site_data);
            self.write_output(&page, &html)?;
        }

        Ok(())
    }
}
```

---

## 10. 性能预期

| 指标 | 现状 | TTG 后 |
|------|------|--------|
| 序列化次数 | 3 | 1 ✅ |
| XML 解析次数 | 2 | 0 ✅ |
| SVG 重复优化 | 2 | 1 ✅ |
| 类型安全（编译期错误） | 0 | ✅ 全部 |
| 热重载增量化 | ❌ 全量 | ✅ 依赖图追踪 |

---

## 11. 附录：FamilyExt 内存布局

```
// FamilyExt<Indexed> 的内存布局
// 大小 = max(各变体大小) + 判别符(1-8字节)

┌─────────────────────────────────────────────────────────────────┐
│ FamilyExt<Indexed>                                              │
├─────────────────────────────────────────────────────────────────┤
│ discriminant: u8 (Svg=0, Link=1, Heading=2, Media=3, Other=4)   │
│ ─────────────────────────────────────────────────────────────── │
│ payload (union of):                                             │
│   Svg:     IndexedElemExt<SvgFamily>     ≈ 40 bytes             │
│   Link:    IndexedElemExt<LinkFamily>    ≈ 56 bytes             │
│   Heading: IndexedElemExt<HeadingFamily> ≈ 40 bytes             │
│   Media:   IndexedElemExt<MediaFamily>   ≈ 32 bytes             │
│   Other:   IndexedElemExt<OtherFamily>   ≈ 8 bytes              │
│ ─────────────────────────────────────────────────────────────── │
│ Total: ~64 bytes (栈分配，无堆开销)                               │
└─────────────────────────────────────────────────────────────────┘

对比 Box<dyn Any>:
┌─────────────────────────────────────────────────────────────────┐
│ Box<dyn Any + Send + Sync>                                      │
├─────────────────────────────────────────────────────────────────┤
│ data_ptr: *mut ()        8 bytes                                │
│ vtable_ptr: *const ()    8 bytes                                │
│ ─────────────────────────────────────────────────────────────── │
│ Total: 16 bytes (+ 堆分配 + vtable 间接调用)                      │
└─────────────────────────────────────────────────────────────────┘

✅ FamilyExt 优势:
- 无堆分配
- 无 vtable 间接调用
- 模式匹配直接分发
- 编译器可完全内联
```


