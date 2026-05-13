//! 这个文件定义主 pipeline 的阶段枚举和状态容器。
//!
//! 这里选择“固定阶段枚举 + 强类型槽位”，是因为当前项目的阶段顺序天然固定，
//! 用静态结构能把每层的输入输出边界尽早钉死，后续排错和调试也更直接。

use crate::ast::AstTargetDialect;
use crate::debug::DebugDetail;
use crate::parser::RawChunk;
use crate::timing::TimingCollector;
use strum_macros::{Display, EnumString, IntoStaticStr};

use super::contracts::{
    AstChunk, CfgGraph, DataflowFacts, GeneratedChunk, GraphFacts, HirChunk, LoweredChunk,
    NamingResult, ReadabilityResult, StructureFacts,
};
use super::options::{DecompileDialect, DecompileOptions};

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

/// 某个阶段导出的调试文本。
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StageDebugOutput {
    pub stage: DecompileStage,
    pub detail: DebugDetail,
    pub content: String,
}

/// 一次主 pipeline 调用期间，各阶段共享的只读上下文。
///
/// 阶段主入口会直接接收 `DecompileState + DecompileContext`：前者承载已完成产物和当前
/// 阶段输出槽位，后者承载本轮调用的字节输入、选项、目标方言和 timing collector。
/// 这样每一层可以在自己的主体方法里读取真正需要的事实，调度表只负责顺序和生命周期。
pub(crate) struct DecompileContext<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) options: &'a DecompileOptions,
    pub(crate) timings: &'a TimingCollector,
    pub(crate) requested_target: AstTargetDialect,
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
}
