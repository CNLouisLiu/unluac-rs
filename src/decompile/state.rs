//! 这个文件定义主 pipeline 的阶段枚举和状态容器。
//!
//! 这里选择“固定阶段枚举 + 强类型槽位”，是因为当前项目的阶段顺序天然固定，
//! 用静态结构能把每层的输入输出边界尽早钉死，后续排错和调试也更直接。

use crate::parser::RawChunk;
use strum_macros::{Display, EnumString, IntoStaticStr};

use super::contracts::{
    AstChunk, CfgGraph, DataflowFacts, GeneratedChunk, GraphFacts, HirChunk, LoweredChunk,
    NamingResult, ReadabilityResult, StructureFacts,
};
use super::options::DecompileDialect;

/// 主反编译 pipeline 的固定阶段顺序。
#[derive(
    Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Display, EnumString, IntoStaticStr,
)]
pub enum DecompileStage {
    #[strum(serialize = "parse")]
    Parse,
    #[strum(serialize = "transform")]
    Transform,
    #[strum(serialize = "cfg")]
    Cfg,
    #[strum(
        serialize = "graph-facts",
        serialize = "graph_facts",
        serialize = "graphfacts"
    )]
    GraphFacts,
    #[strum(serialize = "dataflow")]
    Dataflow,
    #[strum(
        serialize = "structure-facts",
        serialize = "structure_facts",
        serialize = "structurefacts"
    )]
    StructureFacts,
    #[strum(serialize = "hir")]
    Hir,
    #[strum(serialize = "ast")]
    Ast,
    #[strum(serialize = "readability")]
    Readability,
    #[strum(serialize = "naming")]
    Naming,
    #[strum(serialize = "generate")]
    Generate,
}

impl DecompileStage {
    /// 主 pipeline 目前固定线性推进，所以“下一个阶段”也在这里集中维护。
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Parse => Some(Self::Transform),
            Self::Transform => Some(Self::Cfg),
            Self::Cfg => Some(Self::GraphFacts),
            Self::GraphFacts => Some(Self::Dataflow),
            Self::Dataflow => Some(Self::StructureFacts),
            Self::StructureFacts => Some(Self::Hir),
            Self::Hir => Some(Self::Ast),
            Self::Ast => Some(Self::Readability),
            Self::Readability => Some(Self::Naming),
            Self::Naming => Some(Self::Generate),
            Self::Generate => None,
        }
    }
}

/// 一次 pipeline 执行期间，各层产物的统一状态容器。
#[derive(Debug, Clone, PartialEq)]
pub struct DecompileState {
    pub dialect: DecompileDialect,
    pub target_stage: DecompileStage,
    pub completed_stage: Option<DecompileStage>,
    pub raw_chunk: Option<RawChunk>,
    pub lowered: Option<LoweredChunk>,
    pub cfg: Option<CfgGraph>,
    pub graph_facts: Option<GraphFacts>,
    pub dataflow: Option<DataflowFacts>,
    pub structure_facts: Option<StructureFacts>,
    pub hir: Option<HirChunk>,
    pub ast: Option<AstChunk>,
    pub readability: Option<ReadabilityResult>,
    pub naming: Option<NamingResult>,
    pub generated: Option<GeneratedChunk>,
}

impl DecompileState {
    pub(crate) fn new(dialect: DecompileDialect, target_stage: DecompileStage) -> Self {
        Self {
            dialect,
            target_stage,
            completed_stage: None,
            raw_chunk: None,
            lowered: None,
            cfg: None,
            graph_facts: None,
            dataflow: None,
            structure_facts: None,
            hir: None,
            ast: None,
            readability: None,
            naming: None,
            generated: None,
        }
    }

    pub(crate) fn mark_completed(&mut self, stage: DecompileStage) {
        self.completed_stage = Some(stage);
    }

    /// 顺序 pipeline 内部读取已完成阶段产物时使用这些 accessor。
    ///
    /// `DecompileState` 对外仍保留 `Option<T>`，因为目标阶段可能提前停止；
    /// 但 pipeline 自己按固定顺序推进，读取前序产物时只表达这个顺序不变量，
    /// 不在每个阶段重复写一份防御式校验。
    pub(crate) fn raw_chunk(&self) -> &RawChunk {
        required_stage_output(&self.raw_chunk, DecompileStage::Parse)
    }

    pub(crate) fn lowered(&self) -> &LoweredChunk {
        required_stage_output(&self.lowered, DecompileStage::Transform)
    }

    pub(crate) fn cfg(&self) -> &CfgGraph {
        required_stage_output(&self.cfg, DecompileStage::Cfg)
    }

    pub(crate) fn graph_facts(&self) -> &GraphFacts {
        required_stage_output(&self.graph_facts, DecompileStage::GraphFacts)
    }

    pub(crate) fn dataflow(&self) -> &DataflowFacts {
        required_stage_output(&self.dataflow, DecompileStage::Dataflow)
    }

    pub(crate) fn structure_facts(&self) -> &StructureFacts {
        required_stage_output(&self.structure_facts, DecompileStage::StructureFacts)
    }

    pub(crate) fn hir(&self) -> &HirChunk {
        required_stage_output(&self.hir, DecompileStage::Hir)
    }

    pub(crate) fn ast(&self) -> &AstChunk {
        required_stage_output(&self.ast, DecompileStage::Ast)
    }

    pub(crate) fn readability(&self) -> &ReadabilityResult {
        required_stage_output(&self.readability, DecompileStage::Readability)
    }

    pub(crate) fn naming(&self) -> &NamingResult {
        required_stage_output(&self.naming, DecompileStage::Naming)
    }

    pub(crate) fn generated(&self) -> &GeneratedChunk {
        required_stage_output(&self.generated, DecompileStage::Generate)
    }
}

fn required_stage_output<T>(output: &Option<T>, stage: DecompileStage) -> &T {
    match output {
        Some(output) => output,
        None => unreachable!(
            "pipeline invariant violated: stage `{}` output is unavailable",
            stage
        ),
    }
}
