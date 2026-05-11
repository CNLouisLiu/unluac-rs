//! 这个文件承载 HIR 初始恢复的主入口。
//!
//! 外层文件只负责声明 analyze 子模块、组织跨 proto 的递归入口，并把目录内真正的
//! lowering 能力串起来。这样 `src/hir/analyze` 和 `src/hir/simplify` 的外层形状就会
//! 保持一致，后续继续拆分实现时也更容易定位“入口”与“细节”。

mod bindings;
mod exprs;
mod helpers;
mod lower;
mod short_circuit;
mod structure;
#[cfg(test)]
mod tests;

use self::lower::{ChildAnalyses, LowerArtifacts, lower_proto};
use super::simplify::{PassDumpConfig, simplify_hir};
use crate::decompile::{DecompileOptions, DecompileState};
use crate::hir::common::HirModule;
use crate::timing::TimingCollector;

use self::exprs::lower_branch_cond;
use self::helpers::{assign_stmt, branch_stmt, build_label_map_for_summary, goto_block};
use self::lower::{
    ProtoBindings, ProtoLowering, is_control_terminator, lower_control_instr,
    lower_phi_materialization_with_allowed_blocks_except, lower_regular_instr,
};

/// 对整个 lowered chunk 递归构造 HIR。
pub(crate) fn analyze_hir(
    state: &DecompileState,
    options: &DecompileOptions,
    timings: &TimingCollector,
) -> HirModule {
    let child_analyses = ChildAnalyses {
        cfg_graphs: &state.cfg().children,
        graph_facts: &state.graph_facts().children,
        dataflow: &state.dataflow().children,
        structure: &state.structure_facts().children,
    };
    let mut artifacts = LowerArtifacts::default();
    let entry = timings.record("lower", || {
        lower_proto(
            &state.lowered().main,
            &state.cfg().cfg,
            state.graph_facts(),
            state.dataflow(),
            state.structure_facts(),
            child_analyses,
            &mut artifacts,
        )
    });

    let mut module = HirModule {
        entry,
        protos: artifacts.protos,
    };

    let dump_config = PassDumpConfig {
        pass_names: options.debug.dump_passes.clone(),
        filters: options.debug.filters,
    };

    timings.record("simplify", || {
        simplify_hir(
            &mut module,
            options.readability,
            timings,
            &artifacts.promotion_facts,
            options.generate.mode,
            options.dialect.into(),
            &dump_config,
        );
    });
    module
}
