# VDOM 多阶段处理架构设计

> TTG (Trees That Grow) + GATs 实现类型安全、语义化、架构分离的多阶段文档处理
>
> **设计目标**：作为独立 crate 发布，支持用户自定义扩展，同时保持类型安全

## 目录

1. [问题分析](#1-问题分析)
2. [设计目标](#2-设计目标)
3. [方案对比](#3-方案对比)
4. [推荐方案：分层状态设计](#4-推荐方案分层状态设计)
5. [具体实现](#5-具体实现)
6. [迁移路径](#6-迁移路径)
7. [**Crate 扩展性设计**](#7-crate-扩展性设计) ⭐ NEW

---

## 1. 问题分析

### 1.1 当前架构

```
Raw ──────► Indexed ──────► Processed ──────► Rendered
     Indexer        Processor         HtmlRenderer
```

当前实现：
- `Indexer`: Raw → Indexed（生成 StableId，识别 Family）
- `Processor`: Indexed → Processed（**单体黑盒**，内部处理所有逻辑）
- `HtmlRenderer`: Processed → HTML bytes

### 1.2 核心问题

#### 问题 1：Processor 是单体黑盒

```rust
// 当前：所有处理逻辑塞在一个 Processor 里
impl Transform<Indexed> for Processor {
    type To = Processed;
    fn transform(self, doc: Document<Indexed>) -> Document<Processed> {
        // 内部一次性做完所有事：
        // - 链接检查
        // - 链接解析
        // - SVG 优化
        // - Heading 锚点
        // ...
    }
}
```

**后果**：
- 无法单独测试各处理步骤
- 无法灵活组合/跳过步骤
- 类型系统无法表达 "链接已检查但未优化 SVG" 这种中间状态

#### 问题 2：同阶段转换失去类型安全

```rust
// 假设拆分 Processor 为多个 Transform
HeadProcessor:  Transform<Indexed, To=Indexed>
LinkChecker:    Transform<Indexed, To=Indexed>
LinkResolver:   Transform<Indexed, To=Indexed>
SvgOptimizer:   Transform<Indexed, To=Indexed>

// 问题：编译器无法区分！以下代码不会报错：
let doc = doc.pipe(LinkResolver::new());  // 跳过了 LinkChecker!
```

#### 问题 3：FamilyData 无中间状态

```rust
// 当前：LinkIndexedData 是"扁平"的
pub struct LinkIndexedData {
    pub link_type: LinkType,
    pub original_href: Option<String>,
}

// 无法表达：
// - "已检查为空链接"
// - "已解析路径"
// - "已标记为外部链接"
```

### 1.3 理想的多阶段处理

```
Raw → Indexed → [HeadProcessed] → [LinksChecked] → [LinksResolved]
                                                 → [SvgOptimized] → Processed
```

每个 `[]` 步骤应该是：
1. **类型可区分** — 编译器能阻止错误的调用顺序
2. **可独立测试** — 单独验证每个 Transform
3. **可灵活组合** — 按需启用/禁用步骤

---

## 2. 设计目标

| 目标 | 说明 | 优先级 |
|------|------|--------|
| **类型安全** | 编译时阻止错误的处理顺序 | P0 |
| **语义清晰** | 类型名称表达处理进度 | P0 |
| **零运行时开销** | 不引入额外 enum/dyn/Box | P1 |
| **架构分离** | Transform 独立，可测试 | P1 |
| **渐进式迁移** | 可分步实现，不破坏现有代码 | P2 |

---

## 3. 方案对比

### 方案 A：阶段爆炸（❌ 不推荐）

```rust
// 为每个处理步骤定义新 Phase
pub struct HeadProcessed;
pub struct LinksChecked;
pub struct LinksResolved;
pub struct SvgOptimized;

impl Phase for HeadProcessed { ... }
impl PhaseData for HeadProcessed { ... }
// 每个都要定义 ElemExt<F>, DocExt, TextExt...
```

**优点**：完全类型安全
**缺点**：
- 代码膨胀严重（每个 Phase 约 100 行样板）
- PhaseData 的 GAT 类型变得极其复杂
- 违反 DRY 原则

### 方案 B：Type State 泛型（⚠️ 复杂）

```rust
// 用泛型参数标记处理状态
pub struct Indexed<S: ProcessingState = Initial>;

pub trait ProcessingState {}
pub struct Initial;
pub struct HeadDone;
pub struct LinksDone;

// Transform 改变状态参数
impl Transform<Document<Indexed<Initial>>> for HeadProcessor {
    type To = Document<Indexed<HeadDone>>;
}
```

**优点**：类型安全，不需要新 Phase
**缺点**：
- 泛型传播到所有使用处
- `Document<Indexed<HeadDone>>` 类型冗长
- 与现有 PhaseData GAT 设计冲突

### 方案 C：Progress Wrapper（✅ 推荐）

```rust
// 用新类型包装标记处理进度
pub struct HeadProcessed<P: PhaseData>(Document<P>);
pub struct LinksChecked<P: PhaseData>(Document<P>);
pub struct LinksResolved<P: PhaseData>(Document<P>);
pub struct SvgOptimized<P: PhaseData>(Document<P>);

// Transform 的输入/输出类型强制顺序
impl Transform<Document<Indexed>> for HeadProcessor {
    type To = HeadProcessed<Indexed>;
}

impl Transform<HeadProcessed<Indexed>> for LinkChecker {
    type To = LinksChecked<Indexed>;
}
```

**优点**：
- 类型安全，编译时保证顺序
- 不修改现有 Phase/PhaseData 结构
- 零运行时开销（newtype pattern）
- 可渐进式迁移

**缺点**：
- 需要定义 wrapper 类型
- 需要 Deref/DerefMut 实现

### 方案 D：Family 级状态机（✅ 推荐，与 C 组合）

```rust
// FamilyData 本身是状态机
pub enum LinkState {
    /// 刚索引，未处理
    Indexed { href: Option<String>, link_type: LinkType },
    /// 已检查（空链接已标记）
    Checked { href: Option<String>, is_empty: bool, link_type: LinkType },
    /// 已解析（路径完成）
    Resolved { href: String, resolved_path: PathBuf, is_external: bool },
}

pub struct LinkIndexedData {
    pub state: LinkState,
}
```

**优点**：
- Family 内部状态演进自描述
- 支持渐进式处理（部分元素已处理，部分未处理）
- 运行时可查询处理进度

**缺点**：
- 需要重构现有 FamilyData
- enum 增加内存占用（通常可接受）

---

## 4. 推荐方案：分层状态设计

### 4.1 三层架构

```
┌─────────────────────────────────────────────────────────────────┐
│ 层 1: 文档阶段 (Phase)                                          │
│       Raw → Indexed → Processed → Rendered                      │
│       控制整体生命周期，决定 ElemExt<F> 的类型                    │
├─────────────────────────────────────────────────────────────────┤
│ 层 2: 处理进度 (Progress Wrapper)                               │
│       Document<Indexed> → HeadProcessed → LinksChecked → ...    │
│       编译时保证 Transform 调用顺序                              │
├─────────────────────────────────────────────────────────────────┤
│ 层 3: Family 状态 (FamilyData State)                            │
│       LinkState::Indexed → ::Checked → ::Resolved               │
│       运行时状态，支持渐进式处理和状态查询                        │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 层次职责

| 层次 | 职责 | 类型表达 | 运行时/编译时 |
|------|------|----------|---------------|
| **Phase** | 决定 ElemExt 类型族 | `Document<Indexed>` | 编译时 |
| **Progress** | 强制 Transform 顺序 | `LinksChecked<Indexed>` | 编译时（零开销） |
| **FamilyState** | 跟踪元素处理进度 | `LinkState::Resolved` | 运行时 |

### 4.3 为什么需要三层？

**单独 Phase 不够**：Phase 控制的是 GAT 类型（`ElemExt<F>`），但同一 Phase 内的多步处理无法区分。

**单独 Progress 不够**：Progress 只能表达 "文档整体进度"，无法表达 "部分元素已处理"。

**单独 FamilyState 不够**：FamilyState 是运行时的，无法在编译时阻止错误的 Transform 调用。

**三层组合**：
- Phase 决定类型结构
- Progress 保证调用顺序
- FamilyState 支持细粒度状态查询

---

## 5. 具体实现

### 5.1 Progress Wrapper 定义

```rust
// src/vdom/progress.rs

use std::ops::{Deref, DerefMut};
use crate::vdom::{Document, PhaseData};

/// Progress wrapper 基础 trait
pub trait Progress<P: PhaseData>: Deref<Target = Document<P>> + DerefMut {
    /// 解包获取内部 Document
    fn into_inner(self) -> Document<P>;
}

/// 宏：快速定义 Progress wrapper
macro_rules! define_progress {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug)]
        pub struct $name<P: PhaseData>(Document<P>);

        impl<P: PhaseData> $name<P> {
            /// 包装一个 Document
            pub fn new(doc: Document<P>) -> Self {
                Self(doc)
            }

            /// 解包获取内部 Document
            pub fn into_inner(self) -> Document<P> {
                self.0
            }
        }

        impl<P: PhaseData> Deref for $name<P> {
            type Target = Document<P>;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<P: PhaseData> DerefMut for $name<P> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        impl<P: PhaseData> Progress<P> for $name<P> {
            fn into_inner(self) -> Document<P> {
                self.0
            }
        }
    };
}

// 定义处理进度类型
define_progress!(HeadProcessed, "Head 元素已处理（meta 提取完成）");
define_progress!(LinksChecked, "链接已检查（空链接已标记）");
define_progress!(LinksResolved, "链接已解析（路径完成）");
define_progress!(SvgOptimized, "SVG 已优化");
define_progress!(FullyProcessed, "所有处理完成，可转换为 Processed 阶段");
```

### 5.2 Family 状态机定义

```rust
// src/vdom/family.rs (扩展)

/// 链接处理状态机
#[derive(Debug, Clone)]
pub enum LinkState {
    /// 初始状态：刚从 Raw 转换
    Initial {
        href: Option<String>,
        link_type: LinkType,
    },
    /// 已检查：空链接/格式错误已标记
    Checked {
        href: Option<String>,
        link_type: LinkType,
        validation: LinkValidation,
    },
    /// 已解析：路径解析完成
    Resolved {
        original_href: String,
        resolved_url: String,
        is_external: bool,
        is_broken: bool,
    },
}

/// 链接验证结果
#[derive(Debug, Clone, Default)]
pub struct LinkValidation {
    pub is_empty: bool,
    pub is_malformed: bool,
    pub warning: Option<String>,
}

impl LinkState {
    /// 检查是否已检查
    pub fn is_checked(&self) -> bool {
        matches!(self, Self::Checked { .. } | Self::Resolved { .. })
    }

    /// 检查是否已解析
    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved { .. })
    }

    /// 获取原始 href（任何状态）
    pub fn href(&self) -> Option<&str> {
        match self {
            Self::Initial { href, .. } => href.as_deref(),
            Self::Checked { href, .. } => href.as_deref(),
            Self::Resolved { original_href, .. } => Some(original_href),
        }
    }
}

/// SVG 处理状态机
#[derive(Debug, Clone)]
pub enum SvgState {
    /// 初始状态
    Initial {
        viewbox: Option<String>,
        dimensions: Option<(f32, f32)>,
    },
    /// 已分析（结构解析完成）
    Analyzed {
        viewbox: Option<(f32, f32, f32, f32)>,
        dimensions: (f32, f32),
        element_count: usize,
    },
    /// 已优化
    Optimized {
        original_bytes: usize,
        optimized_bytes: usize,
        extracted_path: Option<String>,
    },
}

/// Heading 处理状态机
#[derive(Debug, Clone)]
pub enum HeadingState {
    /// 初始状态
    Initial {
        level: u8,
        original_id: Option<String>,
    },
    /// 已处理（锚点生成完成）
    Processed {
        level: u8,
        anchor_id: String,
        toc_text: String,
        in_toc: bool,
    },
}

// 更新 IndexedData 使用状态机
pub struct LinkIndexedData {
    pub state: LinkState,
}

pub struct SvgIndexedData {
    pub state: SvgState,
}

pub struct HeadingIndexedData {
    pub state: HeadingState,
}
```

### 5.3 Transform 实现示例

```rust
// src/vdom/transforms/head_processor.rs

use crate::vdom::{Document, Indexed, Transform};
use crate::vdom::progress::HeadProcessed;

/// Head 元素处理器
///
/// 提取 <head> 中的 meta 信息，处理 <title> 等
pub struct HeadProcessor {
    // 配置选项
}

impl HeadProcessor {
    pub fn new() -> Self {
        Self {}
    }
}

impl Transform<Document<Indexed>> for HeadProcessor {
    type To = HeadProcessed<Indexed>;

    fn transform(self, doc: Document<Indexed>) -> HeadProcessed<Indexed> {
        // 处理逻辑...
        let processed_doc = self.process_head(doc);
        HeadProcessed::new(processed_doc)
    }
}

// src/vdom/transforms/link_checker.rs

use crate::vdom::progress::{HeadProcessed, LinksChecked};

/// 链接检查器
///
/// 检查所有链接的有效性，标记空链接和格式错误
pub struct LinkChecker {
    pub strict: bool,  // 是否严格模式
}

// 注意：输入是 HeadProcessed，保证了调用顺序！
impl Transform<HeadProcessed<Indexed>> for LinkChecker {
    type To = LinksChecked<Indexed>;

    fn transform(self, doc: HeadProcessed<Indexed>) -> LinksChecked<Indexed> {
        let mut inner = doc.into_inner();

        // 遍历所有 Link 元素，更新状态
        self.check_links(&mut inner);

        LinksChecked::new(inner)
    }
}

// src/vdom/transforms/link_resolver.rs

use crate::vdom::progress::{LinksChecked, LinksResolved};

/// 链接解析器
///
/// 解析相对路径，检查链接是否存在
pub struct LinkResolver {
    pub base_url: String,
}

// 输入是 LinksChecked，保证链接已检查！
impl Transform<LinksChecked<Indexed>> for LinkResolver {
    type To = LinksResolved<Indexed>;

    fn transform(self, doc: LinksChecked<Indexed>) -> LinksResolved<Indexed> {
        let mut inner = doc.into_inner();
        self.resolve_links(&mut inner);
        LinksResolved::new(inner)
    }
}
```

### 5.4 Pipeline 使用示例

```rust
// 正确的调用顺序（编译通过）
let processed = doc
    .pipe(HeadProcessor::new())        // Document<Indexed> → HeadProcessed<Indexed>
    .pipe(LinkChecker::new())          // HeadProcessed → LinksChecked
    .pipe(LinkResolver::new())         // LinksChecked → LinksResolved
    .pipe(SvgOptimizer::new())         // LinksResolved → SvgOptimized
    .pipe(PhaseTransition::new());     // SvgOptimized<Indexed> → Document<Processed>

// 错误的调用顺序（编译失败！）
let wrong = doc
    .pipe(LinkResolver::new());  // ❌ 类型不匹配：Document<Indexed> vs HeadProcessed<Indexed>
    // error[E0277]: the trait `Transform<Document<Indexed>>` is not implemented for `LinkResolver`
```

### 5.5 最终阶段转换

```rust
// src/vdom/transforms/phase_transition.rs

use crate::vdom::{Document, Indexed, Processed, Transform};
use crate::vdom::progress::SvgOptimized;

/// 阶段转换：Indexed → Processed
///
/// 只有当所有处理步骤完成后才能调用
pub struct PhaseTransition;

impl PhaseTransition {
    pub fn new() -> Self { Self }
}

// 输入必须是 SvgOptimized（或 FullyProcessed），保证所有步骤完成
impl Transform<SvgOptimized<Indexed>> for PhaseTransition {
    type To = Document<Processed>;

    fn transform(self, doc: SvgOptimized<Indexed>) -> Document<Processed> {
        let indexed_doc = doc.into_inner();
        // 执行 Indexed → Processed 的类型转换
        convert_indexed_to_processed(indexed_doc)
    }
}
```

### 5.6 可选步骤支持

```rust
// 条件性跳过某些步骤
pub struct OptionalTransform<T, P: PhaseData> {
    inner: Option<T>,
    _phase: PhantomData<P>,
}

impl<T, P, Q> Transform<P> for OptionalTransform<T, P>
where
    P: PhaseData,
    Q: PhaseData,
    T: Transform<P, To = Q>,
{
    type To = Q;  // 或者通过 trait 约束返回 P

    fn transform(self, doc: Document<P>) -> Document<Self::To> {
        match self.inner {
            Some(t) => t.transform(doc),
            None => /* 需要类型系统技巧处理 */,
        }
    }
}

// 更实用的方案：定义跳过路径
impl Transform<HeadProcessed<Indexed>> for SkipLinkProcessing {
    type To = LinksResolved<Indexed>;  // 直接跳到最终状态

    fn transform(self, doc: HeadProcessed<Indexed>) -> LinksResolved<Indexed> {
        // 不做实际处理，直接包装
        LinksResolved::new(doc.into_inner())
    }
}
```

---

## 6. 迁移路径

### Phase 1：定义 Progress wrapper（无破坏性）

```rust
// 新增文件，不修改现有代码
src/vdom/progress.rs
```

### Phase 2：拆分 Processor（渐进式）

```rust
// 保留原 Processor 作为 "legacy" 入口
impl Transform<Document<Indexed>> for LegacyProcessor {
    type To = Document<Processed>;
    fn transform(self, doc: Document<Indexed>) -> Document<Processed> {
        // 内部调用新的分步处理
        doc.pipe(HeadProcessor::new())
           .pipe(LinkChecker::new())
           .pipe(LinkResolver::new())
           .pipe(SvgOptimizer::new())
           .pipe(PhaseTransition::new())
    }
}
```

### Phase 3：重构 FamilyData 为状态机

```rust
// 逐个 Family 迁移
// 1. LinkIndexedData → 使用 LinkState
// 2. SvgIndexedData → 使用 SvgState
// 3. HeadingIndexedData → 使用 HeadingState
```

### Phase 4：更新调用方

```rust
// compile.rs, actor/compiler.rs 等
// 将 `doc.pipe(Processor::new())` 改为显式 pipeline
```

---

## 附录 A：完整类型流程图

```
                            TTG Multi-Phase Pipeline

    typst-html                                                      HTML bytes
        │                                                               ▲
        ▼                                                               │
┌───────────────┐     ┌───────────────┐     ┌───────────────┐     ┌─────────────┐
│ Document<Raw> │ ──► │Document<Idx>  │ ──► │Document<Proc> │ ──► │ HtmlRenderer│
└───────────────┘     └───────┬───────┘     └───────────────┘     └─────────────┘
     Indexer                  │                    ▲
                              │                    │
                              ▼                    │
                    ┌─────────────────────────────────────────────────────┐
                    │              Progress Pipeline                      │
                    │                                                     │
                    │  Document<Indexed>                                  │
                    │       │                                             │
                    │       ▼ HeadProcessor                               │
                    │  HeadProcessed<Indexed>                             │
                    │       │                                             │
                    │       ▼ LinkChecker                                 │
                    │  LinksChecked<Indexed>                              │
                    │       │                                             │
                    │       ▼ LinkResolver                                │
                    │  LinksResolved<Indexed>                             │
                    │       │                                             │
                    │       ▼ SvgOptimizer                                │
                    │  SvgOptimized<Indexed>                              │
                    │       │                                             │
                    │       ▼ PhaseTransition ────────────────────────────┘
                    │
                    └─────────────────────────────────────────────────────┘
```

## 附录 B：与现有 crate 对比

| 特性 | 本方案 | typestate crate | statig crate |
|------|--------|-----------------|--------------|
| 编译时顺序检查 | ✅ | ✅ | ❌ (运行时) |
| 支持树形数据 | ✅ | ❌ | ❌ |
| GAT 支持 | ✅ | ❌ | ❌ |
| 零运行时开销 | ✅ | ✅ | ❌ |
| 渐进式处理 | ✅ (FamilyState) | ❌ | ✅ |
| 无外部依赖 | ✅ | ❌ | ❌ |

---

## 7. Crate 扩展性设计

> **核心挑战**：如何让用户自定义处理逻辑，同时保持编译时类型安全？

### 7.1 扩展性需求分析

作为公开 crate，用户可能需要：

| 扩展类型 | 示例 | 难度 |
|----------|------|------|
| **自定义 Transform** | 添加 LaTeX 公式处理 | 低 |
| **自定义 Family** | 添加 `CodeBlockFamily` | 中 |
| **自定义 Progress** | 在 LinksChecked 后插入 `MyStep` | 高 |
| **自定义 Phase** | 添加 `Validated` 阶段 | 高 |

### 7.2 设计原则

```
┌─────────────────────────────────────────────────────────────────┐
│                    Crate 扩展性分层                              │
├─────────────────────────────────────────────────────────────────┤
│  Sealed (不可扩展)                                              │
│  ├── Phase trait 核心定义                                       │
│  ├── PhaseData GAT 结构                                         │
│  └── 内置 Phase: Raw, Indexed, Processed, Rendered              │
├─────────────────────────────────────────────────────────────────┤
│  Semi-Open (有约束的扩展)                                        │
│  ├── TagFamily trait (用户可实现新 Family)                       │
│  ├── Transform trait (用户可实现新 Transform)                    │
│  └── Capability traits (用户可组合)                              │
├─────────────────────────────────────────────────────────────────┤
│  Open (完全开放)                                                 │
│  ├── FamilyData 结构                                            │
│  ├── 处理逻辑函数                                                │
│  └── Pipeline 组合                                               │
└─────────────────────────────────────────────────────────────────┘
```

### 7.3 Capability-based 类型安全

**核心思想**：用 marker traits 标记文档已完成的处理，Transform 声明依赖的能力。

```rust
// ============ Capability Traits (能力标记) ============

/// 能力标记 trait - 表示文档已完成某种处理
/// 
/// Sealed: 用户不能直接实现，但可以组合
pub trait Capability: sealed::Sealed {}

/// 内置能力
pub trait HeadProcessed: Capability {}
pub trait LinksChecked: Capability {}
pub trait LinksResolved: Capability {}
pub trait SvgOptimized: Capability {}

/// 能力组合 - 自动实现
impl<T: HeadProcessed + LinksChecked> Capability for T {}

// ============ 带能力的文档类型 ============

/// 带能力标记的文档
/// 
/// `C` 是能力类型参数，可以是单个能力或能力组合
pub struct Doc<P: PhaseData, C: Capabilities = ()> {
    inner: Document<P>,
    _capabilities: PhantomData<C>,
}

/// 能力集合 trait
pub trait Capabilities: sealed::Sealed {}

// 空能力（初始状态）
impl Capabilities for () {}

// 能力组合（类型级 HList）
impl<H: Capability, T: Capabilities> Capabilities for (H, T) {}
```

### 7.4 Transform 依赖声明

```rust
// ============ Transform with Capability Requirements ============

/// 带能力约束的 Transform
pub trait CapTransform<P: PhaseData, C: Capabilities> {
    /// 需要的能力（前置条件）
    type Requires: Capabilities;
    /// 提供的能力（后置条件）
    type Provides: Capability;
    /// 输出能力集 = 输入能力 + Provides
    type Output: Capabilities;
    
    fn transform(self, doc: Doc<P, C>) -> Doc<P, Self::Output>
    where
        C: HasCapability<Self::Requires>;  // 编译时检查！
}

// ============ 内置 Transform 实现 ============

/// Head 处理器 - 无前置要求
pub struct HeadProcessor;

impl<P: PhaseData, C: Capabilities> CapTransform<P, C> for HeadProcessor {
    type Requires = ();  // 无前置要求
    type Provides = HeadProcessedCap;
    type Output = (HeadProcessedCap, C);
    
    fn transform(self, doc: Doc<P, C>) -> Doc<P, Self::Output> {
        // 处理 head...
        Doc::with_cap(doc.inner)
    }
}

/// 链接检查器 - 需要 HeadProcessed
pub struct LinkChecker;

impl<P: PhaseData, C: Capabilities> CapTransform<P, C> for LinkChecker 
where
    C: HasCapability<HeadProcessedCap>,  // 前置条件！
{
    type Requires = HeadProcessedCap;
    type Provides = LinksCheckedCap;
    type Output = (LinksCheckedCap, C);
    
    fn transform(self, doc: Doc<P, C>) -> Doc<P, Self::Output> {
        // 检查链接...
        Doc::with_cap(doc.inner)
    }
}
```

### 7.5 用户自定义 Transform

```rust
// ============ 用户代码（下游 crate）============

use vdom::{Doc, CapTransform, Capabilities, HasCapability};
use vdom::caps::{LinksCheckedCap, HeadProcessedCap};

/// 用户自定义能力
pub struct LaTeXProcessedCap;
impl vdom::Capability for LaTeXProcessedCap {}

/// 用户自定义 Transform
pub struct LaTeXProcessor {
    pub katex_options: KaTeXOptions,
}

impl<P: PhaseData, C: Capabilities> CapTransform<P, C> for LaTeXProcessor
where
    C: HasCapability<HeadProcessedCap>,  // 需要 head 已处理
{
    type Requires = HeadProcessedCap;
    type Provides = LaTeXProcessedCap;
    type Output = (LaTeXProcessedCap, C);
    
    fn transform(self, doc: Doc<P, C>) -> Doc<P, Self::Output> {
        // 处理 LaTeX 公式...
        Doc::with_cap(process_latex(doc.inner, &self.katex_options))
    }
}

// ============ 使用示例 ============

fn main() {
    let doc: Doc<Indexed, ()> = Doc::new(indexed_document);
    
    // 正确：满足依赖
    let processed = doc
        .pipe(HeadProcessor)           // () → (HeadProcessed,)
        .pipe(LaTeXProcessor::new())   // (HeadProcessed,) → (LaTeX, HeadProcessed,)
        .pipe(LinkChecker);            // ✅ 有 HeadProcessed
    
    // 错误：缺少依赖
    let wrong = doc
        .pipe(LaTeXProcessor::new());  // ❌ 编译错误：() 没有 HeadProcessed
}
```

### 7.6 用户自定义 Family

```rust
// ============ TagFamily trait (Semi-Open) ============

/// 标签族 trait - 用户可实现
pub trait TagFamily: 'static + Send + Sync {
    const NAME: &'static str;
    
    /// Indexed 阶段的数据类型
    type IndexedData: Debug + Clone + Default + Send + Sync;
    
    /// Processed 阶段的数据类型
    type ProcessedData: Debug + Clone + Default + Send + Sync;
    
    /// 识别函数：tag + attrs → 是否属于此 Family
    fn identify(tag: &str, attrs: &[(String, String)]) -> bool;
    
    /// 处理函数：IndexedData → ProcessedData
    fn process(indexed: &Self::IndexedData) -> Self::ProcessedData;
}

// ============ 用户自定义 Family ============

/// 代码块族（用户定义）
pub struct CodeBlockFamily;

#[derive(Debug, Clone, Default)]
pub struct CodeBlockIndexedData {
    pub language: Option<String>,
    pub line_numbers: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CodeBlockProcessedData {
    pub highlighted_html: String,
    pub language: String,
}

impl TagFamily for CodeBlockFamily {
    const NAME: &'static str = "code-block";
    
    type IndexedData = CodeBlockIndexedData;
    type ProcessedData = CodeBlockProcessedData;
    
    fn identify(tag: &str, attrs: &[(String, String)]) -> bool {
        tag == "pre" && attrs.iter().any(|(k, _)| k == "data-lang")
    }
    
    fn process(indexed: &Self::IndexedData) -> Self::ProcessedData {
        // 语法高亮处理...
        CodeBlockProcessedData {
            highlighted_html: highlight(&indexed.language),
            language: indexed.language.clone().unwrap_or_default(),
        }
    }
}
```

### 7.7 FamilyExt 扩展机制

```rust
// ============ 可扩展的 FamilyExt ============

/// 核心 FamilyExt - 内置 Family
#[derive(Debug, Clone)]
pub enum CoreFamilyExt<P: PhaseData> {
    Svg(P::ElemExt<SvgFamily>),
    Link(P::ElemExt<LinkFamily>),
    Heading(P::ElemExt<HeadingFamily>),
    Media(P::ElemExt<MediaFamily>),
    Other(P::ElemExt<OtherFamily>),
}

/// 可扩展 FamilyExt - 支持用户 Family
#[derive(Debug, Clone)]
pub enum FamilyExt<P: PhaseData, U: UserFamilies = ()> {
    Core(CoreFamilyExt<P>),
    User(U::Ext<P>),
}

/// 用户 Family 集合 trait
pub trait UserFamilies {
    type Ext<P: PhaseData>: Debug + Clone;
}

// 默认：无用户 Family
impl UserFamilies for () {
    type Ext<P: PhaseData> = std::convert::Infallible;
}

// ============ 用户扩展 ============

/// 用户的 Family 集合
pub struct MyFamilies;

impl UserFamilies for MyFamilies {
    type Ext<P: PhaseData> = MyFamilyExt<P>;
}

#[derive(Debug, Clone)]
pub enum MyFamilyExt<P: PhaseData> {
    CodeBlock(P::ElemExt<CodeBlockFamily>),
    MathBlock(P::ElemExt<MathBlockFamily>),
}

// 使用
type MyDocument<P> = Document<P, MyFamilies>;
```

### 7.8 Pipeline Builder API

```rust
// ============ 类型安全的 Pipeline Builder ============

/// Pipeline builder - 流式 API
pub struct PipelineBuilder<P: PhaseData, C: Capabilities> {
    doc: Doc<P, C>,
}

impl<P: PhaseData> PipelineBuilder<P, ()> {
    /// 从 Document 创建 pipeline
    pub fn new(doc: Document<P>) -> Self {
        Self { doc: Doc::new(doc) }
    }
}

impl<P: PhaseData, C: Capabilities> PipelineBuilder<P, C> {
    /// 应用 Transform（类型安全）
    pub fn pipe<T>(self, transform: T) -> PipelineBuilder<P, T::Output>
    where
        T: CapTransform<P, C>,
        C: HasCapability<T::Requires>,
    {
        PipelineBuilder {
            doc: transform.transform(self.doc),
        }
    }
    
    /// 条件应用
    pub fn pipe_if<T>(self, condition: bool, transform: T) -> PipelineBuilder<P, C>
    where
        T: CapTransform<P, C, Output = C>,  // 可选步骤不改变能力
    {
        if condition {
            // 需要特殊处理...
        }
        self
    }
    
    /// 完成并提取 Document
    pub fn finish(self) -> Document<P> {
        self.doc.into_inner()
    }
    
    /// 转换为 Processed 阶段（需要所有必要能力）
    pub fn into_processed(self) -> Document<Processed>
    where
        C: HasCapability<LinksResolvedCap> + HasCapability<SvgOptimizedCap>,
    {
        convert_to_processed(self.doc.into_inner())
    }
}

// ============ 使用示例 ============

// 标准 pipeline
let result = PipelineBuilder::new(doc)
    .pipe(HeadProcessor)
    .pipe(LinkChecker)
    .pipe(LinkResolver::new(&config))
    .pipe(SvgOptimizer::new())
    .into_processed();

// 带用户自定义步骤
let result = PipelineBuilder::new(doc)
    .pipe(HeadProcessor)
    .pipe(LaTeXProcessor::new())      // 用户自定义！
    .pipe(LinkChecker)
    .pipe(MyCustomStep::new())        // 用户自定义！
    .pipe(LinkResolver::new(&config))
    .into_processed();
```

### 7.9 预设 Pipeline（便捷 API）

```rust
// ============ 预设 Pipeline（用户无需了解细节）============

/// 标准处理 pipeline
pub fn standard_pipeline<P: PhaseData>(doc: Document<P>) -> Document<Processed> {
    PipelineBuilder::new(doc)
        .pipe(HeadProcessor)
        .pipe(LinkChecker)
        .pipe(LinkResolver::default())
        .pipe(SvgOptimizer::default())
        .into_processed()
}

/// 最小 pipeline（跳过优化）
pub fn minimal_pipeline<P: PhaseData>(doc: Document<P>) -> Document<Processed> {
    PipelineBuilder::new(doc)
        .pipe(HeadProcessor)
        .pipe(LinkChecker)
        .pipe(LinkResolver::default())
        .pipe(SkipSvgOptimization)  // 跳过但标记能力
        .into_processed()
}

/// 自定义 pipeline builder
pub fn custom_pipeline<P: PhaseData>(doc: Document<P>) -> PipelineBuilder<P, ()> {
    PipelineBuilder::new(doc)
}
```

### 7.10 类型安全保证总结

| 检查项 | 机制 | 时机 |
|--------|------|------|
| Transform 顺序 | `HasCapability<T::Requires>` | 编译时 |
| Phase 兼容性 | `P: PhaseData` 约束 | 编译时 |
| Family 类型安全 | GAT `ElemExt<F>` | 编译时 |
| 能力完整性 | `into_processed()` 约束 | 编译时 |
| 自定义 Transform | trait bounds | 编译时 |

### 7.11 扩展性 vs 类型安全权衡

```
                    类型安全
                        │
                        │    ┌─────────────────┐
                   高 ──┼────│  Capability     │
                        │    │  System         │
                        │    └─────────────────┘
                        │
                        │    ┌─────────────────┐
                   中 ──┼────│  Progress       │
                        │    │  Wrapper        │
                        │    └─────────────────┘
                        │
                        │    ┌─────────────────┐
                   低 ──┼────│  Runtime        │
                        │    │  Dispatch       │
                        │    └─────────────────┘
                        │
                        └────────────────────────► 扩展性
                             低      中      高
```

**推荐策略**：

1. **核心 crate**：使用 Capability System（高类型安全）
2. **用户扩展**：通过 `impl CapTransform` 和 `impl TagFamily`
3. **便捷 API**：提供预设 pipeline，隐藏复杂性

---

## 附录 C：设计决策记录

### 为什么不用 `enum` 包装所有进度状态？

```rust
// 不推荐：运行时分发，失去类型安全
enum ProgressWrapper<P: PhaseData> {
    Raw(Document<P>),
    HeadProcessed(Document<P>),
    LinksChecked(Document<P>),
    // ...
}
```

**理由**：
1. 运行时才知道当前状态，无法编译时检查
2. 需要 match/if-let 解包，代码冗长
3. 无法利用类型系统保证 Transform 顺序

### 为什么 FamilyState 用 `enum` 而不是泛型？

```rust
// 不推荐：泛型传播
pub struct LinkIndexedData<S: LinkState> { ... }

// 推荐：enum 状态机
pub struct LinkIndexedData {
    pub state: LinkState,  // enum
}
```

**理由**：
1. FamilyState 是运行时概念（部分元素可能处于不同状态）
2. enum 支持运行时查询（`state.is_resolved()`）
3. 避免泛型传播到 Element, Node, Document

### 为什么用 Capability System 而不是简单的 newtype wrapper？

```rust
// 简单 wrapper（之前的方案）
struct LinksChecked<P>(Document<P>);

// Capability System（现在的方案）
struct Doc<P, C: Capabilities>(Document<P>, PhantomData<C>);
```

**理由**：
1. **用户扩展性**：Capability 可以任意组合，wrapper 是固定顺序
2. **依赖图 vs 线性链**：Capability 支持 DAG 依赖，wrapper 只支持线性
3. **可选步骤**：Capability 可以跳过某步骤仍满足依赖
4. **类型推导友好**：避免深层嵌套类型 `A<B<C<D<...>>>>`

---

## 附录 D：Crate 公开 API 设计

### D.1 模块结构

```
vdom/
├── lib.rs              # 公开 API 入口
├── phase.rs            # Phase, PhaseData (sealed)
├── family.rs           # TagFamily (semi-open)
├── node/               # Document, Element, Node, Text
├── transform.rs        # Transform trait
├── capability.rs       # Capability system
├── pipeline.rs         # PipelineBuilder
├── transforms/         # 内置 Transform 实现
│   ├── indexer.rs
│   ├── head.rs
│   ├── links.rs
│   ├── svg.rs
│   └── render.rs
└── prelude.rs          # 常用类型重导出
```

### D.2 Prelude 导出

```rust
// src/prelude.rs - 用户只需 `use vdom::prelude::*;`

pub use crate::{
    // 核心类型
    Document, Element, Node, Text,
    
    // Phase
    Raw, Indexed, Processed, Rendered,
    Phase, PhaseData,
    
    // Family
    TagFamily, FamilyExt,
    SvgFamily, LinkFamily, HeadingFamily, MediaFamily, OtherFamily,
    
    // Transform
    Transform, CapTransform,
    
    // Capability
    Capability, Capabilities, HasCapability,
    HeadProcessedCap, LinksCheckedCap, LinksResolvedCap, SvgOptimizedCap,
    
    // Pipeline
    PipelineBuilder,
    
    // 内置 Transform
    Indexer, HeadProcessor, LinkChecker, LinkResolver, SvgOptimizer, HtmlRenderer,
    
    // 便捷函数
    standard_pipeline, minimal_pipeline, custom_pipeline,
};
```

### D.3 最小使用示例

```rust
// 用户代码 - 最简单的使用方式
use vdom::prelude::*;

fn process_document(html: &str) -> String {
    let raw = vdom::parse(html);
    let indexed = raw.pipe(Indexer::new());
    let processed = standard_pipeline(indexed);
    HtmlRenderer::new().render(processed)
}
```

### D.4 自定义扩展示例

```rust
// 用户代码 - 完全自定义
use vdom::prelude::*;

// 1. 定义自定义能力
pub struct MyProcessedCap;
impl Capability for MyProcessedCap {}

// 2. 定义自定义 Transform
pub struct MyTransform;

impl<P: PhaseData, C: Capabilities> CapTransform<P, C> for MyTransform
where
    C: HasCapability<HeadProcessedCap>,
{
    type Requires = HeadProcessedCap;
    type Provides = MyProcessedCap;
    type Output = (MyProcessedCap, C);
    
    fn transform(self, doc: Doc<P, C>) -> Doc<P, Self::Output> {
        // 自定义处理逻辑...
        Doc::with_cap(doc.into_inner())
    }
}

// 3. 使用自定义 pipeline
fn my_pipeline(doc: Document<Indexed>) -> Document<Processed> {
    PipelineBuilder::new(doc)
        .pipe(HeadProcessor)
        .pipe(MyTransform)        // 自定义步骤
        .pipe(LinkChecker)
        .pipe(LinkResolver::default())
        .pipe(SvgOptimizer::default())
        .into_processed()
}
```

### D.5 自定义 Family 示例

```rust
// 用户代码 - 添加新的元素族
use vdom::prelude::*;

// 1. 定义 Family
pub struct CodeBlockFamily;

impl TagFamily for CodeBlockFamily {
    const NAME: &'static str = "code-block";
    
    type IndexedData = CodeBlockIndexed;
    type ProcessedData = CodeBlockProcessed;
    
    fn identify(tag: &str, attrs: &[(String, String)]) -> bool {
        tag == "pre" && attrs.iter().any(|(k, _)| k == "class")
    }
    
    fn process(indexed: &Self::IndexedData) -> Self::ProcessedData {
        CodeBlockProcessed {
            highlighted: highlight_code(&indexed.code, &indexed.lang),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CodeBlockIndexed {
    pub lang: Option<String>,
    pub code: String,
}

#[derive(Debug, Clone, Default)]
pub struct CodeBlockProcessed {
    pub highlighted: String,
}

// 2. 注册到 FamilyExt（需要宏支持或 feature flag）
vdom::register_family!(CodeBlockFamily);
```

---

## 附录 E：实现复杂度估算

| 组件 | 代码行数估算 | 复杂度 | 优先级 |
|------|-------------|--------|--------|
| Capability traits | ~100 | 低 | P0 |
| HasCapability impl | ~150 | 中 | P0 |
| Doc wrapper | ~80 | 低 | P0 |
| CapTransform trait | ~50 | 低 | P0 |
| PipelineBuilder | ~200 | 中 | P0 |
| 内置 Transform 重构 | ~500 | 中 | P1 |
| FamilyExt 扩展机制 | ~300 | 高 | P2 |
| 宏支持 | ~400 | 高 | P2 |

**总计**：约 1800 行新代码

---

## 附录 F：与 Rust 生态对比

| 特性 | 本方案 | tower (Service) | axum (Handler) | bevy (System) |
|------|--------|-----------------|----------------|---------------|
| 类型安全组合 | ✅ Capability | ✅ Layer | ✅ Extractors | ✅ System params |
| 用户扩展 | ✅ impl Trait | ✅ impl Service | ✅ impl Handler | ✅ impl System |
| 编译时检查 | ✅ | ✅ | ✅ | ⚠️ 部分运行时 |
| 树形数据 | ✅ | ❌ | ❌ | ❌ |
| GAT 支持 | ✅ | ❌ | ❌ | ❌ |

**设计灵感来源**：
- **tower**：Service + Layer 组合模式
- **axum**：Extractor 依赖注入
- **bevy**：System 参数约束

---

*文档版本: 2026-01-03 v2*
*作者: Copilot 基于 TTG/GATs 架构分析*
*目标: 可发布为独立 crate，支持用户扩展*
