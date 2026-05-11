//! 这个文件实现主反编译 pipeline 的统一入口。
//!
//! 这里负责按固定顺序线性推进各阶段、收集 debug output 与 timing，
//! 并把 `DecompileState` 作为调度层内部的共享状态容器。阶段方法负责读写对应槽位，
//! 业务层入口仍然只接收明确事实，避免调度层类型扩散到具体恢复算法里。

use crate::ast::{
    AstFeature, AstModule, AstTargetDialect, collect_ast_features, lower_ast, make_readable,
};
use crate::cfg::{analyze_dataflow, analyze_graph_facts, build_cfg_proto};
use crate::generate::{
    GenerateChunkCommentMetadata, GenerateCommentMetadata, GenerateFunctionCommentMetadata,
    GenerateMode, generate_chunk,
};
use crate::hir::analyze_hir;
use crate::naming::{assign_names_with_evidence, collect_naming_evidence};
use crate::structure::analyze_structure;
use crate::timing::{TimingCollector, TimingReport};
use crate::transformer::lower_chunk;

use super::debug::{
    DebugOptions, StageDebugOutput, dump_ast, dump_cfg, dump_dataflow, dump_generate,
    dump_graph_facts, dump_hir, dump_lir, dump_naming, dump_parser, dump_readability,
    dump_structure,
};
use super::error::DecompileError;
use super::options::DecompileOptions;
use super::state::{DecompileStage, DecompileState};

/// 一次主 pipeline 调用的返回值。
#[derive(Debug, Clone, PartialEq)]
pub struct DecompileResult {
    pub state: DecompileState,
    pub debug_output: Vec<StageDebugOutput>,
    pub timing_report: Option<TimingReport>,
}

/// 对外暴露唯一的主入口，统一完成默认值补齐和阶段调度。
pub fn decompile(
    bytes: &[u8],
    options: DecompileOptions,
) -> Result<DecompileResult, DecompileError> {
    DecompilerPipeline.run(bytes, options)
}

struct DecompilerPipeline;

struct PipelineContext<'a> {
    options: &'a DecompileOptions,
    timings: &'a TimingCollector,
    requested_target: AstTargetDialect,
}

struct GenerateStagePlan {
    target: AstTargetDialect,
    mode: GenerateMode,
    warnings: Vec<String>,
}

impl DecompilerPipeline {
    fn run(
        self,
        bytes: &[u8],
        options: DecompileOptions,
    ) -> Result<DecompileResult, DecompileError> {
        let options = options.normalized();

        let mut debug_output = Vec::new();
        let timings = TimingCollector::new(options.debug.enable && options.debug.timing);
        let requested_target = AstTargetDialect::new(options.dialect.into());
        let context = PipelineContext {
            options: &options,
            timings: &timings,
            requested_target,
        };

        let mut state = DecompileState::new(options.dialect, options.target_stage);

        self.parse_stage(bytes, &mut state, &context)?;
        push_stage_dump(
            &mut debug_output,
            DecompileStage::Parse,
            &context.options.debug,
            |debug_options| dump_parser(state.raw_chunk(), debug_options),
        )?;
        if context.options.target_stage == DecompileStage::Parse {
            return Ok(finish_result(state, debug_output, &context));
        }

        self.transform_stage(&mut state, &context)?;
        push_stage_dump(
            &mut debug_output,
            DecompileStage::Transform,
            &context.options.debug,
            |debug_options| dump_lir(state.lowered(), debug_options),
        )?;
        if context.options.target_stage == DecompileStage::Transform {
            return Ok(finish_result(state, debug_output, &context));
        }

        self.cfg_stage(&mut state, &context);
        push_stage_dump(
            &mut debug_output,
            DecompileStage::Cfg,
            &context.options.debug,
            |debug_options| dump_cfg(state.cfg(), debug_options),
        )?;
        if context.options.target_stage == DecompileStage::Cfg {
            return Ok(finish_result(state, debug_output, &context));
        }

        self.graph_facts_stage(&mut state, &context);
        push_stage_dump(
            &mut debug_output,
            DecompileStage::GraphFacts,
            &context.options.debug,
            |debug_options| dump_graph_facts(state.graph_facts(), debug_options),
        )?;
        if context.options.target_stage == DecompileStage::GraphFacts {
            return Ok(finish_result(state, debug_output, &context));
        }

        self.dataflow_stage(&mut state, &context);
        push_stage_dump(
            &mut debug_output,
            DecompileStage::Dataflow,
            &context.options.debug,
            |debug_options| {
                dump_dataflow(
                    state.lowered(),
                    state.cfg(),
                    state.dataflow(),
                    debug_options,
                )
            },
        )?;
        if context.options.target_stage == DecompileStage::Dataflow {
            return Ok(finish_result(state, debug_output, &context));
        }

        self.structure_stage(&mut state, &context);
        push_stage_dump(
            &mut debug_output,
            DecompileStage::StructureFacts,
            &context.options.debug,
            |debug_options| dump_structure(state.structure_facts(), debug_options),
        )?;
        if context.options.target_stage == DecompileStage::StructureFacts {
            return Ok(finish_result(state, debug_output, &context));
        }

        self.hir_stage(&mut state, &context);
        push_stage_dump(
            &mut debug_output,
            DecompileStage::Hir,
            &context.options.debug,
            |debug_options| dump_hir(state.hir(), debug_options),
        )?;
        if context.options.target_stage == DecompileStage::Hir {
            return Ok(finish_result(state, debug_output, &context));
        }

        self.ast_stage(&mut state, &context)?;
        push_stage_dump(
            &mut debug_output,
            DecompileStage::Ast,
            &context.options.debug,
            |debug_options| dump_ast(state.ast(), debug_options),
        )?;
        if context.options.target_stage == DecompileStage::Ast {
            return Ok(finish_result(state, debug_output, &context));
        }

        let generate_plan = self.readability_stage(&mut state, &context);
        push_stage_dump(
            &mut debug_output,
            DecompileStage::Readability,
            &context.options.debug,
            |debug_options| dump_readability(state.readability(), debug_options),
        )?;
        if context.options.target_stage == DecompileStage::Readability {
            return Ok(finish_result(state, debug_output, &context));
        }

        self.naming_stage(&mut state, &context)?;
        push_stage_dump(
            &mut debug_output,
            DecompileStage::Naming,
            &context.options.debug,
            |debug_options| dump_naming(state.naming(), debug_options),
        )?;
        if context.options.target_stage == DecompileStage::Naming {
            return Ok(finish_result(state, debug_output, &context));
        }

        self.generate_stage(&mut state, generate_plan, &context)?;
        push_stage_dump(
            &mut debug_output,
            DecompileStage::Generate,
            &context.options.debug,
            |debug_options| dump_generate(state.generated(), debug_options),
        )?;

        Ok(finish_result(state, debug_output, &context))
    }

    fn parse_stage(
        &self,
        bytes: &[u8],
        state: &mut DecompileState,
        context: &PipelineContext<'_>,
    ) -> Result<(), DecompileError> {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::Parse));
        state.raw_chunk = Some(
            context
                .options
                .dialect
                .parse_chunk(bytes, context.options.parse)?,
        );
        state.mark_completed(DecompileStage::Parse);
        Ok(())
    }

    fn transform_stage(
        &self,
        state: &mut DecompileState,
        context: &PipelineContext<'_>,
    ) -> Result<(), DecompileError> {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::Transform));
        let lowered = lower_chunk(state.raw_chunk())?;
        state.lowered = Some(lowered);
        state.mark_completed(DecompileStage::Transform);
        Ok(())
    }

    fn cfg_stage(&self, state: &mut DecompileState, context: &PipelineContext<'_>) {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::Cfg));
        state.cfg = Some(build_cfg_proto(&state.lowered().main));
        state.mark_completed(DecompileStage::Cfg);
    }

    fn graph_facts_stage(&self, state: &mut DecompileState, context: &PipelineContext<'_>) {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::GraphFacts));
        state.graph_facts = Some(analyze_graph_facts(state.cfg()));
        state.mark_completed(DecompileStage::GraphFacts);
    }

    fn dataflow_stage(&self, state: &mut DecompileState, context: &PipelineContext<'_>) {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::Dataflow));
        let dataflow = analyze_dataflow(
            &state.lowered().main,
            &state.cfg().cfg,
            state.graph_facts(),
            &state.cfg().children,
        );
        state.dataflow = Some(dataflow);
        state.mark_completed(DecompileStage::Dataflow);
    }

    fn structure_stage(&self, state: &mut DecompileState, context: &PipelineContext<'_>) {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::StructureFacts));
        let structure_facts = analyze_structure(state, context.options);
        state.structure_facts = Some(structure_facts);
        state.mark_completed(DecompileStage::StructureFacts);
    }

    fn hir_stage(&self, state: &mut DecompileState, context: &PipelineContext<'_>) {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::Hir));
        let hir = analyze_hir(state, context.options, context.timings);
        state.hir = Some(hir);
        state.mark_completed(DecompileStage::Hir);
    }

    fn ast_stage(
        &self,
        state: &mut DecompileState,
        context: &PipelineContext<'_>,
    ) -> Result<(), DecompileError> {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::Ast));
        let target = match context.options.generate.mode {
            GenerateMode::Strict => context.requested_target,
            GenerateMode::Permissive => {
                AstTargetDialect::relaxed_for_lowering(context.requested_target.version)
            }
        };
        let ast = lower_ast(state.hir(), target, context.options.generate.mode)?;
        state.ast = Some(ast);
        state.mark_completed(DecompileStage::Ast);
        Ok(())
    }

    fn readability_stage(
        &self,
        state: &mut DecompileState,
        context: &PipelineContext<'_>,
    ) -> GenerateStagePlan {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::Readability));
        let readability = make_readable(
            state.ast(),
            context.requested_target,
            context.options.readability,
            context.timings,
            &context.options.debug.dump_passes,
        );
        let warnings = generate_stage_warnings(
            &readability,
            context.requested_target,
            context.options.generate.mode,
        );
        let generate_plan = GenerateStagePlan {
            target: context.requested_target,
            mode: context.options.generate.mode,
            warnings,
        };
        state.readability = Some(readability);
        state.mark_completed(DecompileStage::Readability);
        generate_plan
    }

    fn naming_stage(
        &self,
        state: &mut DecompileState,
        context: &PipelineContext<'_>,
    ) -> Result<(), DecompileError> {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::Naming));
        let evidence = {
            let _timing = context.timings.scope("collect-evidence");
            collect_naming_evidence(state.hir())
        }?;
        let naming = assign_names_with_evidence(
            state.readability(),
            state.hir(),
            &evidence,
            context.options.naming,
        )?;
        state.naming = Some(naming);
        state.mark_completed(DecompileStage::Naming);
        Ok(())
    }

    fn generate_stage(
        &self,
        state: &mut DecompileState,
        plan: GenerateStagePlan,
        context: &PipelineContext<'_>,
    ) -> Result<(), DecompileError> {
        let _timing = context
            .timings
            .scope(<&'static str>::from(DecompileStage::Generate));
        let mut generate_options = context.options.generate;
        generate_options.mode = plan.mode;
        let comment_metadata = if generate_options.comment {
            Some(build_generate_comment_metadata(
                state.hir(),
                context.options.parse.string_encoding.as_str(),
            ))
        } else {
            None
        };
        let mut generated = generate_chunk(
            state.readability(),
            state.naming(),
            plan.target,
            comment_metadata.as_ref(),
            generate_options,
        )?;
        generated.warnings = plan.warnings;
        state.generated = Some(generated);
        state.mark_completed(DecompileStage::Generate);
        Ok(())
    }
}

fn push_stage_dump(
    debug_output: &mut Vec<StageDebugOutput>,
    stage: DecompileStage,
    options: &DebugOptions,
    dump: impl FnOnce(&DebugOptions) -> Result<StageDebugOutput, DecompileError>,
) -> Result<(), DecompileError> {
    if !options.enable || !options.output_stages.contains(&stage) {
        return Ok(());
    }

    debug_output.push(dump(options)?);
    Ok(())
}

fn finish_result(
    state: DecompileState,
    debug_output: Vec<StageDebugOutput>,
    context: &PipelineContext<'_>,
) -> DecompileResult {
    DecompileResult {
        state,
        debug_output,
        timing_report: context.timings.finish(),
    }
}

fn generate_stage_warnings(
    module: &AstModule,
    target: AstTargetDialect,
    mode: GenerateMode,
) -> Vec<String> {
    if mode != GenerateMode::Permissive {
        return Vec::new();
    }

    let unsupported = collect_ast_features(module)
        .into_iter()
        .filter(|feature| !target.supports_feature(*feature))
        .collect::<Vec<_>>();
    if unsupported.is_empty() {
        return Vec::new();
    }

    vec![format!(
        "requested target dialect `{}` does not support feature(s) {}; emitting permissive output.",
        target.version,
        format_ast_features(&unsupported)
    )]
}

fn format_ast_features(features: &[AstFeature]) -> String {
    features
        .iter()
        .map(|feature| <&'static str>::from(*feature))
        .collect::<Vec<_>>()
        .join(", ")
}

fn build_generate_comment_metadata(
    hir: &crate::hir::HirModule,
    encoding: &str,
) -> GenerateCommentMetadata {
    let entry_source = hir
        .protos
        .get(hir.entry.index())
        .and_then(|proto| proto.source.clone());
    GenerateCommentMetadata {
        chunk: GenerateChunkCommentMetadata {
            file_name: entry_source,
            encoding: encoding.to_owned(),
        },
        functions: hir
            .protos
            .iter()
            .map(|proto| GenerateFunctionCommentMetadata {
                function: proto.id,
                source: proto.source.clone(),
                line_range: proto.line_range,
                signature: proto.signature,
                local_count: proto.locals.len(),
                upvalue_count: proto.upvalues.len(),
            })
            .collect(),
    }
}
