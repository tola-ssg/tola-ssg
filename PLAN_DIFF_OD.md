# Diff/Hot Reload/StableId 改进计划

基于 Gemini 对话的深度分析，结合当前代码实现，以下是严谨的技术评估和改进方案。

---

## 问题诊断 (Problem Diagnosis)

### 🔴 已确认的问题

#### 1. **Index Drift (索引漂移)** - 高优先级 ⚠️

**问题**: Rust 生成的 `RemoveAtPosition` 使用基于旧快照的索引，但 JS 端线性执行时，前面的删除会改变后续索引。

**当前代码** ([message.rs](src/hotreload/message.rs)):
```rust
// from_patches() 未对 ops 排序
let ops: Vec<PatchOp> = patches.iter().map(...).collect();
```

**示例**:
- 删除 index 0 和 index 2 的元素
- Rust 生成: `[RemoveAtPosition(0), RemoveAtPosition(2)]`
- JS 执行第一个后，原 index 2 变成 index 1
- JS 删除 index 2 = 删错了节点!

**修复**: 在 `from_patches()` 中对 `RemoveAtPosition` 按 position 降序排序。

---

#### 2. **JavaScript 坐标系混用** - 高优先级 ⚠️

**问题**: `insert/move` 使用 `parent.children`（仅元素），而 `remove_at_pos/text_at_pos` 使用 `parent.childNodes`（含文本）。

**当前代码** ([hotreload.js](src/embed/serve/hotreload.js)):
```javascript
// insert 用 children (忽略文本节点)
const children = parent.children;
children[op.position].insertAdjacentHTML('beforebegin', op.html);

// remove_at_pos 用 childNodes (包含文本节点)
const childNodes = parent.childNodes;
childNodes[pos].remove();
```

**后果**: 如果父元素混合了文本节点和元素节点，索引指向完全不同的内容。

**修复**: 统一使用 `childNodes` + `insertBefore`。

---

### 🟡 需要评估的问题

#### 3. **Position Paradox (位置悖论)** - 需谨慎评估

**Gemini 观点**: `StableId` hash 包含 `position`，元素移动时 ID 变化，无法检测 Move。

**当前实现** ([id.rs](src/vdom/id.rs)):
```rust
pub fn for_element(tag: &str, attrs: &[(String, String)], _children: &[StableId], position: usize) -> Self {
    // ...
    position.hash(&mut hasher);  // 问题点
}
```

**⚠️ 但是!** 我们需要仔细评估：

1. **为什么需要 position?**
   - 区分相同内容的兄弟节点: `<p>Hello</p><p>Hello</p>`
   - 如果完全移除 position，这两个 `<p>` 会有相同的 ID

2. **Occurrence Index 方案的代价**:
   - 需要在 Indexer 中维护 `HashMap<ContentHash, Count>`
   - 每次构建都要重新计算
   - 复杂度增加

3. **当前场景分析**:
   - Tola 是 SSG，文档结构通常稳定
   - 热更新主要是内容变化，不是结构重排
   - **Move 检测的实际需求有多强?**

**建议**: 分阶段处理
- Phase 1: 先修复 Index Drift 和坐标系问题 (立即可做)
- Phase 2: 如果实际使用中 Move 检测问题频繁，再改 StableId

---

#### 4. **防御性插入逻辑** - 中等优先级

**当前代码** ([hotreload.js](src/embed/serve/hotreload.js#L155-L170)):
```javascript
case 'insert': {
    // 如果新 ID 已存在，执行 replace 而不是 insert
    if (newIds.some(id => document.querySelector(`[data-tola-id="${id}"]`))) {
        // 更新已存在的元素...
    } else {
        // 正常插入
    }
}
```

**问题**: 这会吞掉隐式的 Move 操作。

**但是**: 当前 diff 算法已经显式处理 Move (`Patch::Move`)，此逻辑可能是历史遗留。

**建议**: 验证是否有实际场景触发此分支，考虑简化或移除。

---

## 🟢 不需要修改的部分

### 5. **StableId 不含 Children** - 正确!

**当前实现** ([id.rs](src/vdom/id.rs#L103-L107)):
```rust
// Hash child IDs (REMOVED: StableId should not depend on children content)
// If we include children, ANY change in a leaf node changes the ENTIRE path of StableIds to the root.
```

**评估**: 这是正确的设计决策。如果 ID 包含 children hash，叶节点的任何变化都会导致整条路径的 ID 变化，diff 会退化为 Replace 根节点。

---

### 6. **CRDT/OT** - 不需要

**评估**: Tola 是单向同步场景（文件系统 → 浏览器），不存在并发写入冲突。当前基于 StableId 的方案已经借鉴了 CRDT 的"稳定寻址"思想，无需引入完整 CRDT 库。

---

## 修复计划 (Implementation Plan)

### Phase 1: Index Drift 修复 (立即执行)

**目标**: 确保 `RemoveAtPosition` 操作不会因为索引漂移而删错节点。

**修改 1** - [message.rs](src/hotreload/message.rs) `from_patches()`:

```rust
pub fn from_patches(path: &str, patches: &[crate::vdom::diff::Patch]) -> Self {
    use crate::vdom::diff::Patch;

    let mut ops: Vec<PatchOp> = patches.iter().map(|p| match p { ... }).collect();

    // 关键修复: 对操作进行智能排序
    // RemoveAtPosition 必须按 position 降序 (从后往前删)
    ops.sort_by(|a, b| {
        use std::cmp::Ordering;
        match (a, b) {
            (PatchOp::RemoveAtPosition { parent: p1, position: pos1 },
             PatchOp::RemoveAtPosition { parent: p2, position: pos2 }) if p1 == p2 => {
                pos2.cmp(pos1)  // 降序: 先删后面的
            }
            (PatchOp::RemoveAtPosition { .. }, _) => Ordering::Less,  // 删除优先
            (_, PatchOp::RemoveAtPosition { .. }) => Ordering::Greater,
            _ => Ordering::Equal,
        }
    });

    Self::Patch { path: path.to_string(), ops }
}
```

---

### Phase 2: JavaScript 坐标系统一 (立即执行)

**目标**: 所有基于索引的操作统一使用 `childNodes`。

**修改 2** - [hotreload.js](src/embed/serve/hotreload.js) `case 'insert'`:

```javascript
case 'insert': {
    const parent = this.getById(op.parent);
    if (!parent) break;

    // 使用 template 解析 HTML
    const template = document.createElement('template');
    template.innerHTML = op.html;
    const fragment = template.content;

    // 统一使用 childNodes (包含文本节点)
    const childNodes = parent.childNodes;
    const pos = parseInt(op.position, 10);

    if (pos >= childNodes.length) {
        parent.appendChild(fragment);
    } else {
        parent.insertBefore(fragment, childNodes[pos]);
    }
    break;
}
```

**修改 3** - [hotreload.js](src/embed/serve/hotreload.js) `case 'move'`:

```javascript
case 'move': {
    const target = this.getById(op.target);
    const newParent = this.getById(op.new_parent);
    if (!target || !newParent) break;

    // insertBefore 自动将节点从旧位置移动
    const childNodes = newParent.childNodes;
    const pos = parseInt(op.position, 10);

    if (pos >= childNodes.length) {
        newParent.appendChild(target);
    } else if (childNodes[pos] !== target) {
        newParent.insertBefore(target, childNodes[pos]);
    }
    break;
}
```

---

### Phase 3: 可选优化 (后续评估)

#### 3.1 StableId Occurrence Index 方案

如果 Phase 1-2 修复后仍存在 Move 检测问题，考虑修改 [indexer.rs](src/vdom/transforms/indexer.rs):

```rust
fn index_element(&mut self, elem: Element<Raw>, position: usize) -> Element<Indexed> {
    // ...

    // 计算 occurrence_index 而不是 position
    let content_key = compute_content_key(&tag, &attrs);
    let occurrence = self.occurrence_counts
        .entry(content_key)
        .or_insert(0);
    let discriminator = *occurrence;
    *occurrence += 1;

    let stable_id = StableId::for_element(&tag, &attrs, &child_stable_ids, discriminator);
}
```

#### 3.2 移除防御性插入逻辑

验证并简化 [hotreload.js](src/embed/serve/hotreload.js#L155-L170) 中的 ID 检查逻辑。

---

## 测试用例

### 必须通过的场景

1. **删除多个相邻文本节点**
   - 输入: `<div>A B C</div>` → `<div>A</div>`
   - 期望: 正确删除 B 和 C

2. **混合节点删除**
   - 输入: `<div>text<span>elem</span>text2</div>` → `<div><span>elem</span></div>`
   - 期望: 文本节点正确删除，元素保留

3. **文本更新不触发结构变化**
   - 输入: `<p>Hello</p>` → `<p>World</p>`
   - 期望: 只有 `UpdateText`，无 Remove/Insert

4. **元素移动检测 (Phase 3 后)**
   - 输入: `<div><a/><b/></div>` → `<div><b/><a/></div>`
   - 期望: 生成 `Move` 而不是 Delete+Insert

---

## 决策记录

| 问题 | Gemini 建议 | 我的评估 | 决定 |
|------|-------------|----------|------|
| Index Drift | 排序 ops | ✅ 同意 | 实施 |
| 坐标系混用 | 统一 childNodes | ✅ 同意 | 实施 |
| Position Paradox | Occurrence Index | ⚠️ 有代价，先观察 | 延后 |
| 防御性插入 | 移除 | ⚠️ 需验证场景 | 评估 |
| CRDT/OT | 不需要 | ✅ 同意 | 不实施 |
| Myers Diff (文本级) | 可选优化 | ⚠️ 收益有限 | 延后评估 |

---

## 🟠 可选优化：Myers Diff 字符级更新

### Gemini 的建议

当文本节点内容变化时，不直接替换整个 `textContent`，而是使用 Myers Diff 算法计算字符级变更：

```rust
// Rust 端：使用 similar crate 计算字符级 diff
use similar::{ChangeTag, TextDiff};

fn diff_text_content(old: &str, new: &str) -> Vec<TextPatchOp> {
    let diff = TextDiff::from_chars(old, new);
    
    // 如果差异太大，直接全量替换
    if diff.ratio() < 0.5 {
        return vec![TextPatchOp::ReplaceAll(new.to_string())];
    }
    
    // 否则生成字符级操作
    let mut ops = Vec::new();
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => ops.push(TextPatchOp::Delete { start, count }),
            ChangeTag::Insert => ops.push(TextPatchOp::Insert { start, text }),
            ChangeTag::Equal => { /* skip */ }
        }
    }
    ops
}
```

```javascript
// JS 端：使用 Text Node 的精细 API
case 'text_diff': {
    const node = this.getById(op.target);
    if (node?.nodeType === Node.TEXT_NODE) {
        if (op.action === 'delete') {
            node.deleteData(op.start, op.count);
        } else if (op.action === 'insert') {
            node.insertData(op.start, op.text);
        }
    }
}
```

### 我的评估：收益有限，暂不实施

#### ❌ 为什么暂不实施

1. **DOM API 限制**
   - `Text.deleteData()` / `Text.insertData()` 虽然存在，但很少使用
   - 浏览器底层依然需要重新布局（Layout）和绘制（Paint）
   - 对于短文本（大部分场景），`textContent = "new"` 和精细操作性能差异微乎其微

2. **复杂度 vs 收益**
   - 需要在 Rust 端引入 `similar` crate
   - 需要新增 `TextPatchOp` 类型和 JS 处理逻辑
   - **预期收益**: 可能节省几 KB 的 WebSocket 传输（对于长文本）

3. **当前瓶颈不在这里**
   - 热更新的主要延迟是 Typst 编译，不是 Diff 传输
   - 人类感知不到 1ms 和 0.1ms 的区别

4. **Text Node 合并问题**
   - 如果使用字符级 Diff，VDOM 中多个相邻 Text Node 必须先合并
   - 否则索引会错位（浏览器解析 HTML 时可能合并相邻文本）

#### ✅ 什么时候值得考虑

1. **超长文本场景**: 单个 Text Node 内容 > 10KB（如代码块、长段落）
2. **低带宽环境**: WebSocket 传输成为瓶颈
3. **动画需求**: 需要保留 DOM 节点避免 CSS transition 重置

#### 📊 建议的触发阈值

如果未来要实施，可以设置阈值：

```rust
const TEXT_DIFF_THRESHOLD: usize = 1024; // 1KB

fn should_use_text_diff(old: &str, new: &str) -> bool {
    old.len() > TEXT_DIFF_THRESHOLD || new.len() > TEXT_DIFF_THRESHOLD
}
```

### 结论

**暂不实施**。当前 `UpdateText { target, text }` 的全量替换方案足够高效。如果未来遇到长文本热更新性能问题，再考虑引入 Myers Diff。