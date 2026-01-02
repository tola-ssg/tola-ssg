# VDOM Actor 重构问题与修复方案

## 1. 现状观察 (Current Observation)

经过对代码库的深度审查（包括 `src/serve.rs`, `src/actor/*.rs`, `src/vdom`），确认了核心问题的根源。

### A. 热重载闪烁 (The Flickering Issue) **[根源已确认]**
**表现**：每次编辑文件，浏览器都会进行全页刷新（Reload），而不是预期的增量更新（Patch）。即使是微小的修改也是如此。

**根本原因：缺少初始构建 (Missing Initial Build)**
1. **启动流程缺陷**：
   - 在 `src/serve.rs` 中启动 `Coordinator`。
   - `Coordinator::run` (在 `src/actor/coordinator.rs`) 启动了 `FsActor` 和其他 Actors。
   - **关键缺失**：系统启动后，**没有任何代码** 触发一次 "全量扫描与编译"。
2. **FsActor 行为**：
   - `FsActor` (在 `src/actor/fs.rs`) 使用 `notify` 监听文件变化。它只在 **文件发生变化** 时产生事件。
   - 它不会在启动时遍历目录发送初始的一批 `Compile` 消息。
3. **连锁反应**：
   - 因为没有初始编译，`VdomActor` 中的 `cache` 是空的。
   - 当你第一次编辑某个文件（比如 `index.typ`），`CompilerActor` 编译它。
   - `VdomActor` 接收到新 VDOM，去查缓存，发现为空（因为之前没编过）。
   - `pipeline::diff::compute_diff` 返回 `DiffOutcome::Initial`。
   - `VdomActor` 处理 `Initial` -> 发送 `WsMsg::Reload`。
   - 浏览器刷新。

**核心结论**：目前的架构下，任何文件的**首次编辑**必然导致刷新。

### C. 持续闪烁 (Persistent Flickering) **[新发现]**
**表现**：即使在其后对同一文件进行编辑，浏览器依然触发刷新 (Reload)，而不是增量更新 (Patch)。这意味着 VDOM Cache 虽然在第一次编译时通过 `compute_diff` 填充了，但后续查询仍然未命中。

**根本原因：路径规范化缺失 (Missing Path Normalization)**
1. **FsActor 传递原始路径**：`src/actor/fs.rs` 直接将 `notify` 返回的系统路径传给 `CompilerActor`。在 macOS 等系统上，这可能包含未解析的符号链接（如 `/var` vs `/private/var`）或大小写差异。
2. **Key 生成不一致**：`PageMeta::from_paths` 依赖路径去除前缀 (`strip_prefix`) 来生成 `url_path`。
3. **Canonicalization 冲突**：已确认 `src/compiler/deps.rs` 中的 `record_dependencies` 会显式调用 `canonicalize`。这意味着系统内部部分模块使用标准路径，而 `FsActor` 和可能的 `PageMeta` 逻辑使用原始路径。这种混合使用导致了 Key 不匹配。
4. **缓存未命中**：如果 `url_path` 在两次编译间发生变化（哪怕只是 `/` 的区别），`VdomCache` 就无法找到旧的 VDOM，再次触发 `DiffOutcome::Initial` -> **Reload**。

### D. 职责架构混乱 (Chaotic Architecture)
**表现**：数据流向不清晰，`CompilerActor` 越过 `VdomActor` 直接控制 WebSocket。

**原因**：
1. **CompilerActor 越权**：`src/actor/compiler.rs` 直接持有 `ws_tx`。当发生编译错误或资源文件变更（非 `.typ`）时，它直接发送 `Reload`。这使得 `VdomActor` 失去了对 "页面状态" 的完整控制权。
2. **VdomActor 被动**：它只处理成功的编译结果，无法拦截错误或决定是否显示 "编译错误覆盖层"。

## 2. 修复方案 (Proposed Fixes)

### 核心修复：实施初始构建 (Implement Initial Build)

必须在 Actor 系统启动进入事件循环之前，先进行一次全量扫描和编译，以预热 VDOM Cache。

**修改建议**：
1. 在 `FsActor` 中添加一个 `scan_all()` 方法，或者在启动时发送一组初始事件。
2. 或者更简单地，在 `Coordinator::run` 中：
   ```rust
   // src/actor/coordinator.rs

   pub async fn run(self) -> Result<()> {
       // ... 创建 channel ...

       // 1. 获取所有监听路径
       let watch_paths = self.get_watch_paths();

       // 2. [新增] 收集所有源文件 (.typ)
       let initial_files = crate::compiler::collect_all_files(&self.config.build.content); // 需适配

       // 3. [新增] 发送初始编译任务给 CompilerActor
       compiler_tx.send(CompilerMsg::Compile(initial_files)).await?;

       // 4. ... 启动 Actors ...
   }
   ```
   *注意：这需要确保 VdomActor 在编译完成前已经准备好接收消息，或者 Channel 缓冲区足够大。更好的方式是让 CompilerActor 在启动时自己扫描。*

### 架构修复：线性化数据流 (Linear Data Flow)

**原则**：`Compiler` -> `Vdom` -> `Ws`。

1. **移除 CompilerActor 的 Ws 依赖**：
   - `CompilerActor` 不再持有 `ws_tx`。
   - 所有结果（`Vdom`, `Reload`, `Error`）都封装在 `CompilerOutcome` 中发送给 `VdomActor`。
2. **VdomActor 统一决策**：
   - 处理 `Error`：可以决定不刷新浏览器，而是发送一个 Patch 显示错误信息（Overlay）。
   - 处理 `Reload`（资源变更）：转发给 WsActor。
   - 处理 `Vdom`：计算 Diff 并发送 Patch。

### E. 系统审计确认 (System Audit Confirmation)
**代码库全量审计完成**。已检查 `typst_lib`, `driver`, `config`, `main`, `data` 等模块。
- **配置一致性**：`src/config/mod.rs` 在启动时通过 `normalize_paths` 强制所有配置路径为 Canonical 路径。这进一步证实了 `FsActor` 传递原始路径是导致系统中唯非规范化输入的来源。
- **构建模式**：`src/driver.rs` 正确控制了 `emit_ids` 和 `cache_vdom`，逻辑无误。
- **数据流**：除了上述的 CompilerActor 越权问题，其他模块间的耦合（如 `data` 模块的虚拟文件）是符合设计的。

### 数据结构建议

目前 `src/vdom` 的实现（StableId, Indexed Phase）看起来是稳健的。主要问题在于 Cache 管理和路径一致性。

### 路径修复：强制规范化 (Enforce Canonicalization)

**修改建议**：
在 `FsActor` 中，利用 `std::fs::canonicalize` 或复用 `src/compiler/mod.rs` 中的 `canonicalize` 辅助函数，确保所有进入系统的路径都与 `SiteConfig` 中的根路径标准保持一致。

## 3. 具体行动路径 (Action Items)

1. **[关键] 修复初始构建**：修改 `Coordinator`，在启动 `FsActor` 之前或同时，触发一次全量 `.typ` 文件的编译。这将填充 Cache。
2. **[关键] 路径规范化**：在 `FsActor::run` 循环中，收到 `notify` 事件后，立即进行规范化处理，确保后续所有 Actor 接收到的都是 Canonical Path。
3. **标准化消息流**：重构 `CompilerMsg` 和 `VdomMsg`，确保所有编译输出流经 `VdomActor`。
4. **验证修复**：
   - 启动 `tola watch`。
   - 等待初始构建完成。
   - 修改文件 -> 观察是否 Patch。
   - **再次**修改同一文件 -> 观察是否仍然 Patch (验证路径稳定性)。
