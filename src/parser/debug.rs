//! 这个文件承载 parser 层对外暴露的调试入口。
//!
//! 具体某个 dialect 的 dump 逻辑放在各自目录里，这里只负责从主 pipeline state
//! 读取 parser 产物并根据解析结果做分派；另外共享 `ParserProtoEntry` 与
//! `build_parser_summary_row`，让各 dialect 在构造 elided 占位行时用同一套字段语义
//! （lines/instrs/children）。

use crate::debug::{DebugColorMode, DebugDetail, DebugFilters, ProtoSummaryRow, define_stage_dump};
use crate::decompile::DecompileDialect;

use super::dialect::lua51;
use super::dialect::lua52;
use super::dialect::lua53;
use super::dialect::lua54;
use super::dialect::lua55;
use super::dialect::luajit;
use super::dialect::luau;
use super::{RawChunk, RawProto};

define_stage_dump! {
    /// Parser 阶段的调试导出。
    pub fn dump_parser(state, options) => Parse,
        dump_parser_chunk(
            state.raw_chunk.as_ref().unwrap(),
            options.detail,
            &options.filters,
            options.color
        );
}

/// 根据 chunk 的实际 dialect 分派到对应的 parser dump 实现。
fn dump_parser_chunk(
    chunk: &RawChunk,
    detail: DebugDetail,
    filters: &DebugFilters,
    color: DebugColorMode,
) -> String {
    match chunk.header.version {
        DecompileDialect::Lua51 => lua51::dump_chunk(chunk, detail, filters, color),
        DecompileDialect::Lua52 => lua52::dump_chunk(chunk, detail, filters, color),
        DecompileDialect::Lua53 => lua53::dump_chunk(chunk, detail, filters, color),
        DecompileDialect::Lua54 => lua54::dump_chunk(chunk, detail, filters, color),
        DecompileDialect::Lua55 => lua55::dump_chunk(chunk, detail, filters, color),
        DecompileDialect::Luajit => luajit::dump_chunk(chunk, detail, filters, color),
        DecompileDialect::Luau => luau::dump_chunk(chunk, detail, filters, color),
    }
}

/// 各 dialect parser dump 复用的 `(id, parent, depth, proto)` 快照。
///
/// dialect 自己的 `ProtoEntry<'a>` 携带的方言特有字段（flags、extra 等）不在
/// 这里出现；这个投影只保留 `format_proto_summary_row` 需要的那几项。
/// `parent` 目前没有被读，但保留以便未来 breadcrumb/children dump 可以复用。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParserProtoEntry<'a> {
    pub id: usize,
    #[allow(dead_code)]
    pub parent: Option<usize>,
    pub depth: usize,
    pub proto: &'a RawProto,
}

/// 把 `RawProto` 压成 elided 行。parser 阶段拿不到函数名，只能呈现
/// `lines / instrs / children` 三项，这些都从 `RawProto::common` 里直接取得。
pub(crate) fn build_parser_summary_row(entry: &ParserProtoEntry<'_>) -> ProtoSummaryRow {
    ProtoSummaryRow {
        id: entry.id,
        depth_below_focus: entry.depth,
        name: None,
        first: None,
        lines: Some((
            entry.proto.common.line_range.defined_start,
            entry.proto.common.line_range.defined_end,
        )),
        instrs: Some(entry.proto.common.instructions.len()),
        children: Some(entry.proto.common.children.len()),
    }
}
