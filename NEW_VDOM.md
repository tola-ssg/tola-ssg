# VDOM 多阶段处理架构设计 (v2.0)

> TTG (Trees That Grow) + GATs 实现类型安全、语义化、架构分离的多阶段文档处理
>
> **设计目标**：构建一个工业级、零开销、类型安全的 SSG 文档处理流水线。作为独立 crate 发布。

## 目录

1. [问题分析](#1-问题分析)
2. [核心理念与目标](#2-核心理念与目标)
3. [架构总览](#3-架构总览)
4. [关键设计决策 (Critical Decisions)](#4-关键设计决策)
    - [决策 1: 三层状态模型](#决策-1-三层状态模型)
    - [决策 2: 诊断与错误累积](#决策-2-诊断与错误累积)
    - [决策 3: 基于 Capability 的扩展性](#决策-3-基于-capability-的扩展性)
    - [决策 4: 同步核心与异步 IO 分离](#决策-4-同步核心与异步-io-分离)
5. [详细实现方案](#5-详细实现方案)
6. [迁移与落地](#6-迁移与落地)

---

## 1. 问题分析

### 1.1 现状痛点

当前 VDOM (`vdom/mod.rs`) 虽然引入了 Raw/Indexed/Processed 阶段，但中间过程仍然是一个黑盒：

1.  **"超级转换器" (God Processor)**: `Processor` 结构体承载了太多职责（链接检查、SVG 优化、Heading 处理等），导致代码耦合，无法单独测试或复用。
2.  **隐式依赖链**: 必须先进行 "Link Check" 才能进行 "Link Resolve"，但这种依赖关系仅存在于代码逻辑中，编译器无法感知。假如开发者调整了顺序，可能会在运行时才发现 Panic 或逻辑错误。
3.  **状态丢失**: 元素从 `Indexed` 变到 `Processed` 是瞬间的。我们无法表达 "这个链接已经检查过是死链，但还没有解析最终路径" 这种中间状态。
4.  **配置传递混乱**: 各种 Transform 需要不同的全局配置（asset map, config, routes），目前缺乏统一且优雅的注入方式。
5.  **缺乏诊断信息**: 转换过程目前主要关注快乐路径，缺乏统一的 Warning/Error 收集机制。

### 1.2 理想形态

我们希望这一条流水线像工厂流水线一样：
*   **每一步都被类型系统监控**: 如果你试图打包一个还没组装的产品，流水线应该拒绝运行（编译失败）。
*   **自带质检报告**: 每个产品（Document）出来时都带着一份详细的质检报告（Diagnostics）。
*   **模块化插槽**: 可以随意拆卸、替换某个工序（Transform），只要接口对得上。

---

## 2. 核心理念与目标

| 维度 | 目标 | 策略 |
|------|------|------|
| **Safety** | **编译时阻止逻辑错误** |利用 Rust 类型系统 (GATs, Newtypes, Marker Traits) 编码处理顺序。 |
| **Observability** | **全链路诊断** | Document 携带 `Vec<Diagnostic>`，所有 Transform 只能追加不能丢弃诊断。 |
| **Flexibility** | **可组合、可插拔** | Transform 是独立的，通过 Builder 或 Pipeline 组合。 |
| **Performance** | **零抽象开销** | 大量使用 Zero-sized types (ZST) 和 phantom data，运行时无额外内存分配。 |

---

## 3. 架构总览

我们将架构分为三个层次，解决不同粒度的问题：

```mermaid
graph TD
    Level1[Level 1: Phase (生命周期)] --> Level2[Level 2: Progress (流水线编排)]
    Level2 --> Level3[Level 3: Family State (微观状态)]

    style Level1 fill:#e1f5fe,stroke:#01579b
    style Level2 fill:#fff3e0,stroke:#ff6f00
    style Level3 fill:#f3e5f5,stroke:#4a148c
```

1.  **Level 1: Phase (生命周期)**
    *   控制 GATs (`ElemExt<F>`) 的具体类型。
    *   阶段：`Raw` (纯数据) -> `Indexed` (有 ID) -> `Processed` (准备渲染) -> `Rendered` (HTML)。

2.  **Level 2: Progress (流水线编排)**
    *   控制 `Document<P>` 的外层包装，确保 Transform 顺序。
    *   类型：`HeadProcessed<P>`, `LinksChecked<P>`, `SvgOptimized<P>`。

3.  **Level 3: Family State (微观状态)**
    *   `TagFamily` 内部的数据状态机。
    *   例如 LinkFamily: `Initial` -> `Checked` -> `Resolved`。

---

## 4. 关键设计决策

### 决策 1: 三层状态模型

这是本设计的核心。我们不应该用 Phase 去表达每一个微小的处理步骤（那会导致 Phase 爆炸）。也不应该完全依赖运行时状态（不安全）。

**方案**:
*   `Phase` 依然只有 3-4 个大阶段。
*   `Progress` (Newtype Wrapper) 用来在同一 Phase 内标记进度。
*   `FamilyState` Enum 用来存储具体的数据演变。

### 决策 2: 诊断与错误累积

SSG 构建不应因为发现一个死链就立即崩溃（Panic），而应该收集所有错误并在最后统一报告。

**方案**:
在 `Document` 结构体中内置诊断通道。

```rust
pub struct Document<P: PhaseData> {
    pub root: Element<P>,
    pub ext: P::DocExt,
    /// 累积的诊断信息（错误、警告、Lint 建议）
    pub diagnostics: Vec<Diagnostic>,
}
```

任何 `Transform` 在处理过程中：
1.  **必须** 接力传递现有的 `diagnostics`。
2.  **可以** 向其中 push 新的 `Diagnostic`。
3.  **不应** 随意清空它。

### 决策 3: 基于 Capability 的扩展性

为了让 crate 对外提供良好的扩展性，我们不能把 Progress Wrapper 写死。我们引入 "能力 (Capabilities)" 的概念。

```rust
// 标记 trait
pub trait HeadProcessed: Capability {}
pub trait LinksChecked: Capability {}

// 携带能力的文档容器
pub struct Doc<P: PhaseData, C: Capabilities> {
    inner: Document<P>,
    _cap: PhantomData<C>,
}

// Transform 声明：我需要 C1，我提供 C2
impl<P, C> Transform<Doc<P, C>> for MyTransform
where C: Has<OutputOf<MyTransform::Requires>>
{ ... }
```
*(注：为了保持内部实现简单，初期可以使用具体的 Progress Newtype，但在对外 API 上可以预留 Adapter)*。

**本次落地建议**: 优先使用 **Progress Newtype** 模式（方案 C），因为更直观且错误提示更友好。但在 `vdom` 库层面，可以通过 trait alias 暴露类似 Capability 的语义。

### 决策 4: 同步核心与异步 IO 分离

`LinkChecker` 检查外部链接通常也是异步的 (IO Bound)。如果强行塞入同步 Pipeline，会阻塞 CPU 密集型的转换。

**方案**:
*   **Core VDOM Pipeline**: 保持 **纯同步**。只做内存中的数据变换（URL 解析、SVG 优化、锚点生成）。
*   **Linters / Side Effects**: 独立于 Pipeline 之外。
    *   例如：`ExternalLinkChecker` 不修改 VDOM，只读取 links 并产生 `Diagnostics`。它应当是一个 `async` 任务，与 Pipeline 并行或在 Pipeline 之后运行。
    *   对于必须在 Pipeline 中进行的 IO (如读取本地图片尺寸)，如果是本地 fs 操作，且有缓存，可以视为准同步操作接受；或者预先加载 AssetMetadata。

---

## 5. 详细实现方案

### 5.1 增强的 Document 定义

```rust
// src/vdom/node.rs

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel, // Error, Warning, Info
    pub message: String,
    pub span: Option<Span>,     // 关联源码位置
    pub element_id: Option<StableId>, // 关联 DOM 节点
}

pub struct Document<P: PhaseData> {
    pub root: Element<P>,
    pub ext: P::DocExt,
    pub diagnostics: Vec<Diagnostic>,
}

impl<P: PhaseData> Document<P> {
    pub fn push_error(&mut self, msg: impl Into<String>) { ... }
    pub fn push_warning(&mut self, msg: impl Into<String>) { ... }

    // 转换 phase 时保留 diagnostics
    pub fn map_phase<Q: PhaseData>(self, root: Element<Q>, ext: Q::DocExt) -> Document<Q> {
        Document {
            root,
            ext,
            diagnostics: self.diagnostics,
        }
    }
}
```

### 5.2 改进的 Pipeline 与 Context

Transform 不再是孤立的函数，它是 Configurable 的 Struct。

```rust
// src/vdom/transforms/mod.rs

/// 链接解析器配置
pub struct LinkResolverConfig {
    pub base_url: String,
    pub route_table: Arc<RouteTable>,
}

pub struct LinkResolver {
    config: LinkResolverConfig,
}

impl LinkResolver {
    pub fn new(config: LinkResolverConfig) -> Self { Self { config } }
}

impl Transform<LinksChecked<Indexed>> for LinkResolver {
    type To = LinksResolved<Indexed>;

    fn transform(self, mut doc: LinksChecked<Indexed>) -> Self::To {
        let mut inner = doc.into_inner();

        // 遍历并在原地修改 FamilyData
        // 如果发现无法解析的链接，Push diagnostic，而不是 panic
        self.resolve_recursive(&mut inner.root, &mut inner.diagnostics);

        LinksResolved::new(inner)
    }
}
```

### 5.3 完整的 Progress Chain

我们在 `src/vdom/pipeline.rs` (或 `progress.rs`) 中定义完整的流水线阶段。

```rust
define_progress!(RawDoc, "初始 Raw 状态");
define_progress!(IndexedDoc, "已分配 StableID");
define_progress!(HeadProcessed, "Head 元数据已处理");
define_progress!(LinksChecked, "链接完整性已检查");
define_progress!(LinksResolved, "路径已解析");
define_progress!(MediaProcessed, "媒体资源已处理");
define_progress!(ReadyToRender, "所有处理完成");
```

### 5.4 Family State Machine 示例

```rust
// src/vdom/family/link.rs

#[derive(Debug, Clone)]
pub enum LinkState {
    Indexed(LinkRawData),
    Checked(LinkCheckedData),
    Resolved(LinkResolvedData),
}

// 辅助方法，用于在 transform 中安全地 switch state
impl LinkIndexedData {
    pub fn as_checked_mut(&mut self) -> Result<&mut LinkCheckedData, E> { ... }
}
```

---

## 6. 迁移与落地路线图

### Phase 1: 基础设施 (Infrastructure)
1.  **Diagnostic System**: 给 `Document` 加上 `diagnostics` 字段。
    *   *影响*: 所有创建 `Document` 的地方都需要初始化这个字段 (很简单)。
2.  **Progress Wrappers**: 创建 `src/vdom/progress.rs`，定义 Newtypes。
    *   *影响*: 无，纯新增。

### Phase 2: 拆解 Processor (Deconstruct)
1.  **Extract Transforms**: 将 `Processor` 中的私有方法 (`process_links`, `process_svg` 等) 提取为独立的 Struct (`LinkProcessor`, `SvgOptimizer`)。
2.  **Legacy Bridge**: `Processor` 暂时保留，但在内部调用这些新 Struct。
    *   *此时尚不强制 Progress 类型检查，先保证逻辑拆分*。

### Phase 3: 引入状态 (Stateify)
1.  **Refactor FamilyData**: 将 `LinkIndexedData` 等结构体改为 Enum 状态机 (`LinkState`)。
    *   *影响*: 这是一个**Breaking Change**。所有访问 `indexed.link_data` 的代码都需要修改为 pattern matching。这是最痛的一步，需要集中精力完成。

### Phase 4: 强制流水线 (Enforce)
1.  **Apply Progress Types**: 修改 Transform 的 `transform` 签名，使用 `HeadProcessed` 等 Wrapper 类型。
2.  **Update Pipeline**: 在 `compile.rs` 中使用 `.pipe().pipe()` 链式调用构建正式流水线。
