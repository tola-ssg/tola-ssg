# PLAN_NEXT: 架构改进计划

> 非 VDOM 相关的架构改进（VDOM 相关见 [docs/NEW_VDOM.md](docs/NEW_VDOM.md)）

## 目录

1. [模块重组](#1-模块重组)
2. [全局状态清理](#2-全局状态清理)
3. [Actor 系统优化](#3-actor-系统优化)
4. [构建系统统一](#4-构建系统统一)

---

## 1. 模块重组

### 1.1 问题：`pipeline/` 命名混淆

当前 `src/pipeline/` 模块：
- 只被 `actor/` 模块使用
- 与 VDOM 的 `Pipeline<P>` 类型命名冲突
- 实际是 "热重载业务逻辑"，不是 "处理管道"

```
当前结构:
src/
├── pipeline/           # ⚠️ 只被 actor/ 使用
│   ├── classify.rs     # → actor/fs.rs 使用
│   ├── compile.rs      # → actor/compiler.rs 使用
│   ├── diff.rs         # → actor/vdom.rs 使用
│   └── init.rs         # → actor/coordinator.rs 使用
├── actor/              # 消费 pipeline/
└── vdom/
    └── transform.rs    # Pipeline<P> 类型 ← 命名冲突！
```

### 1.2 方案 A：合并到 actor/

```
src/actor/
├── mod.rs
├── coordinator.rs
├── compiler.rs         # 合并 pipeline/compile.rs
├── fs.rs               # 合并 pipeline/classify.rs
├── vdom.rs             # 合并 pipeline/diff.rs
├── ws.rs
└── messages.rs
```

**优点**：减少模块层级，每个 actor 自包含
**缺点**：文件变大，职责不够清晰

### 1.3 方案 B：移动到 hotreload/（✅ 推荐）

```
src/hotreload/
├── mod.rs
├── message.rs          # 热重载消息格式
├── ws.rs               # WebSocket 服务
└── logic/              # 原 pipeline/ 移入
    ├── mod.rs
    ├── classify.rs     # 文件分类
    ├── compile.rs      # 增量编译
    ├── diff.rs         # VDOM diff
    └── init.rs         # 初始缓存
```

**优点**：
- 语义清晰：hotreload 子系统完整
- 避免与 VDOM Pipeline 命名冲突
- 职责边界明确

**实施步骤**：
```bash
# 1. 创建 hotreload/logic/ 目录
mkdir -p src/hotreload/logic

# 2. 移动文件
mv src/pipeline/*.rs src/hotreload/logic/

# 3. 删除旧目录
rmdir src/pipeline

# 4. 更新导入路径
# actor/fs.rs:      use crate::pipeline::classify → use crate::hotreload::logic::classify
# actor/compiler.rs: use crate::pipeline::compile → use crate::hotreload::logic::compile
# actor/vdom.rs:    use crate::pipeline::diff    → use crate::hotreload::logic::diff
# actor/coordinator.rs: use crate::pipeline::init → use crate::hotreload::logic::init
```

### 1.4 方案 C：重命名为 incremental/

```
src/incremental/       # 增量构建逻辑
├── classify.rs
├── compile.rs
├── diff.rs
└── init.rs
```

**优点**：语义准确（增量构建）
**缺点**：仍是顶层模块，与 actor 关系不明显

---

## 2. 全局状态清理

### 2.1 当前全局状态

| 全局变量 | 位置 | 用途 | 清理难度 |
|----------|------|------|----------|
| `GLOBAL_SITE_DATA` | `data/store.rs` | 页面元数据存储 | 🔴 高 |
| `DEPENDENCY_GRAPH` | `compiler/deps.rs` | 依赖追踪 | 🟡 中 |
| `CONFIG` | `config/handle.rs` | 全局配置 | 🟡 中 |
| `VERBOSE` | `logger.rs` | 日志级别 | 🟢 低 |

### 2.2 问题分析

```rust
// GLOBAL_SITE_DATA 被 Typst 同步回调使用
// 这是 Typst World trait 的限制：file() 方法是 &self，不能传递 &mut

impl World for SystemWorld {
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        // 这里需要读取 GLOBAL_SITE_DATA 来处理虚拟文件
        // 但 &self 不能持有 &mut SiteData
        if is_virtual_path(id) {
            return read_from_global_site_data(id);  // 被迫使用全局状态
        }
        // ...
    }
}
```

### 2.3 清理策略

#### 阶段 1：封装访问（短期）

```rust
// 用 newtype 封装，限制访问入口
pub struct SiteDataHandle {
    // 内部仍使用全局状态，但控制访问方式
}

impl SiteDataHandle {
    pub fn read<R>(&self, f: impl FnOnce(&SiteData) -> R) -> R {
        GLOBAL_SITE_DATA.read().unwrap()(f)
    }

    pub fn write<R>(&self, f: impl FnOnce(&mut SiteData) -> R) -> R {
        GLOBAL_SITE_DATA.write().unwrap()(f)
    }
}
```

#### 阶段 2：Actor 内部化（中期）

```rust
// 将 SiteData 移入 CoordinatorActor
pub struct CoordinatorActor {
    site_data: SiteData,  // 不再全局
    // ...
}

// 其他 Actor 通过消息请求数据
enum CoordinatorMsg {
    GetPageMeta(String, oneshot::Sender<Option<PageMeta>>),
    // ...
}
```

#### 阶段 3：World 重构（远期）

```rust
// 自定义 World 实现，内部持有 SiteData 引用
pub struct TolaWorld<'a> {
    base: SystemWorld,
    site_data: &'a SiteData,  // 不再全局！
}

// 需要 Typst 支持或 workaround
```

### 2.4 CONFIG 全局变量清理

```rust
// 当前：全局 OnceLock
static CONFIG: OnceLock<SiteConfig> = OnceLock::new();

// 目标：显式传递
pub struct BuildContext {
    pub config: Arc<SiteConfig>,
    pub driver: Box<dyn BuildDriver>,
}

// 所有函数接收 context 而非读取全局
fn compile_page(ctx: &BuildContext, path: &Path) -> Result<...> {
    let config = &ctx.config;
    // ...
}
```

---

## 3. Actor 系统优化

### 3.1 当前 Actor 架构

```
FsActor ──Compile──► CompilerActor ──Process──► VdomActor ──Patch──► WsActor
   │                      │                        │
   └──────────────────────┴────────────────────────┘
                    CoordinatorActor
```

### 3.2 待优化项

#### 3.2.1 消息类型精简

```rust
// 当前：预留了很多未使用的消息变体
pub enum VdomMsg {
    Populate { ... },
    Process { ... },
    Reload { ... },
    Skip,
    Invalidate { ... },  // #[allow(dead_code)] 预留
    Clear,
    Shutdown,            // #[allow(dead_code)] 预留
}

// 优化：只保留实际使用的，用 feature flag 控制扩展
#[cfg(feature = "advanced-vdom")]
Invalidate { url_path: String },
```

#### 3.2.2 错误处理统一

```rust
// 当前：各 Actor 错误处理不一致
// CompilerActor: 返回 CompileOutcome::Error
// VdomActor: 发送 Reload
// WsActor: log 后忽略

// 优化：统一错误类型
pub enum ActorError {
    Compile(CompileError),
    Vdom(VdomError),
    Ws(WsError),
    Io(std::io::Error),
}

// 统一处理策略
impl From<ActorError> for UserFacingMessage {
    fn from(err: ActorError) -> Self {
        // 转换为用户友好的消息
    }
}
```

#### 3.2.3 Backpressure 机制

```rust
// 当前：unbounded channel，可能 OOM
let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

// 优化：bounded channel + backpressure
let (tx, rx) = tokio::sync::mpsc::channel(100);

// 发送方处理背压
if tx.send(msg).await.is_err() {
    // channel 满或关闭
    log!("warn"; "message dropped due to backpressure");
}
```

---

## 4. 构建系统统一

### 4.1 当前双轨问题

```
tola build (生产)              tola serve (开发)
    │                              │
    ▼                              ▼
build.rs                       actor/coordinator.rs
    │                              │
    ▼                              ▼
compiler/pages.rs              hotreload/logic/compile.rs
    │                              │
    ▼                              ▼
process_page()                 compile_page()
```

两套代码，逻辑重复！

### 4.2 统一方案

```rust
// 统一入口点
pub fn compile_page<D: BuildDriver>(
    driver: D,
    path: &Path,
    config: &SiteConfig,
) -> CompileResult {
    // 共享逻辑
}

// build.rs 使用
pub fn build_site<D: BuildDriver>(driver: D, config: &SiteConfig) -> Result<...> {
    for path in content_files {
        compile_page(driver, &path, config)?;
    }
}

// actor/compiler.rs 使用
impl CompilerActor {
    fn handle_compile(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            let result = compile_page(Development, &path, &self.config);
            // 发送结果到 VdomActor
        }
    }
}
```

### 4.3 Driver Pattern 扩展

```rust
// 当前 Driver
pub trait BuildDriver {
    fn emit_ids(&self) -> bool;
    fn cache_vdom(&self) -> bool;
}

// 扩展：更多构建行为
pub trait BuildDriver {
    fn emit_ids(&self) -> bool;
    fn cache_vdom(&self) -> bool;

    // 新增
    fn minify_html(&self) -> bool;
    fn optimize_svg(&self) -> bool;
    fn source_maps(&self) -> bool;
    fn incremental(&self) -> bool;
}

// 预设实现
pub struct Production;
impl BuildDriver for Production {
    fn emit_ids(&self) -> bool { false }
    fn cache_vdom(&self) -> bool { false }
    fn minify_html(&self) -> bool { true }
    fn optimize_svg(&self) -> bool { true }
    fn source_maps(&self) -> bool { false }
    fn incremental(&self) -> bool { false }
}

pub struct Development;
impl BuildDriver for Development {
    fn emit_ids(&self) -> bool { true }
    fn cache_vdom(&self) -> bool { true }
    fn minify_html(&self) -> bool { false }
    fn optimize_svg(&self) -> bool { false }  // 开发时跳过优化
    fn source_maps(&self) -> bool { true }
    fn incremental(&self) -> bool { true }
}
```

---

## 实施优先级

| 任务 | 优先级 | 复杂度 | 依赖 | 状态 |
|------|--------|--------|------|------|
| pipeline/ → hotreload/logic/ | P0 | 低 | 无 | ✅ 已完成 |
| CONFIG 封装 | P1 | 低 | 无 | ✅ 已完成 (cfg() 仅入口调用) |
| 构建系统统一 | P1 | 中 | Driver Pattern | 📋 进行中 |
| Actor 错误处理统一 | P2 | 中 | 无 | 📋 待定 |
| GLOBAL_SITE_DATA 封装 | P2 | 中 | 无 | 📋 待定 |
| GLOBAL_SITE_DATA 内部化 | P3 | 高 | Actor 重构 | 📋 待定 |
| World 重构 | P4 | 高 | Typst 限制 | 📋 待定 |

---

## 相关文档

- [docs/NEW_VDOM.md](docs/NEW_VDOM.md) - VDOM 多阶段处理架构
- [docs/GLOBAL_STATE_ANALYSIS.md](docs/GLOBAL_STATE_ANALYSIS.md) - 全局状态分析
- [PLAN_VDOM.md](PLAN_VDOM.md) - 原始 VDOM 迁移计划

---

*文档版本: 2026-01-04*
