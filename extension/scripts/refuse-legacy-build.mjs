#!/usr/bin/env node
// 拒绝执行 legacy 构建链 —— 详见 ../BUILD-LEGACY.md
//
// 背景:本目录的 src/index.ts 是 v0.3.0 之前的 **spawn 架构**
// (extension 通过 sigma.binary 亲自拉起 sidecar),而
// release/extension/dist/index.js 是 v0.5.5 的 **Scheduled Task 架构**。
//
// 两者不是"新��旧"关系,而是两条设计路线。
// 从 src 构建会覆盖 dist,丢掉 v0.3.0 -> v0.5.5 的全部演进(含 v0.5.5/v0.5.7
// 的 sandbox 门禁、焦点归属、有护栏的地址栏导航等全部修复)。

const msg = `
================================================================
  BLOCKED: this build would DESTROY the current extension.
================================================================

  extension/src/index.ts      = v0.3.0 之前的 spawn 架构(已废弃)
  release/.../dist/index.js   = v0.5.5 Scheduled Task 架构(唯一真相源)

  从 src 构建会用废弃实现覆盖 dist,丢掉 v0.3.0 -> v0.5.5 的全部修复。

  See: extension/BUILD-LEGACY.md
================================================================
`;

console.error(msg);
process.exit(1);