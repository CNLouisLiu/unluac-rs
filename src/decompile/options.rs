//! 这个文件定义主 pipeline 的公共选项。
//!
//! 入口层集中补默认值，比把默认逻辑散在各阶段里更稳；后续阶段变多后，
//! 仍然只需要维护这一处归一化逻辑。

use crate::ast::AstDialectVersion;
use crate::generate::GenerateOptions;
use crate::naming::{NamingMode, NamingOptions};
use crate::parser::{
    ParseMode, ParseOptions, RawChunk, StringDecodeMode, StringEncoding, parse_lua51_chunk,
    parse_lua52_chunk, parse_lua53_chunk, parse_lua54_chunk, parse_lua55_chunk, parse_luajit_chunk,
    parse_luau_chunk,
};
use crate::readability::ReadabilityOptions;
use strum_macros::{Display, EnumString, IntoStaticStr};

use super::debug::DebugOptions;
use super::state::DecompileStage;

/// 调用方请求解析的目标 dialect。
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Display, EnumString, IntoStaticStr)]
pub enum DecompileDialect {
    #[default]
    #[strum(serialize = "lua5.1", serialize = "lua51")]
    Lua51,
    #[strum(serialize = "lua5.2", serialize = "lua52")]
    Lua52,
    #[strum(serialize = "lua5.3", serialize = "lua53")]
    Lua53,
    #[strum(serialize = "lua5.4", serialize = "lua54")]
    Lua54,
    #[strum(serialize = "lua5.5", serialize = "lua55")]
    Lua55,
    #[strum(serialize = "luajit")]
    Luajit,
    #[strum(serialize = "luau")]
    Luau,
}

impl DecompileDialect {
    /// 按 dialect 分派到对应的字节码 parser。
    pub fn parse_chunk(
        self,
        bytes: &[u8],
        options: ParseOptions,
    ) -> Result<RawChunk, crate::parser::ParseError> {
        match self {
            Self::Lua51 => parse_lua51_chunk(bytes, options),
            Self::Lua52 => parse_lua52_chunk(bytes, options),
            Self::Lua53 => parse_lua53_chunk(bytes, options),
            Self::Lua54 => parse_lua54_chunk(bytes, options),
            Self::Lua55 => parse_lua55_chunk(bytes, options),
            Self::Luajit => parse_luajit_chunk(bytes, options),
            Self::Luau => parse_luau_chunk(bytes, options),
        }
    }
}

impl From<DecompileDialect> for AstDialectVersion {
    fn from(dialect: DecompileDialect) -> Self {
        match dialect {
            DecompileDialect::Lua51 => AstDialectVersion::Lua51,
            DecompileDialect::Lua52 => AstDialectVersion::Lua52,
            DecompileDialect::Lua53 => AstDialectVersion::Lua53,
            DecompileDialect::Lua54 => AstDialectVersion::Lua54,
            DecompileDialect::Lua55 => AstDialectVersion::Lua55,
            DecompileDialect::Luajit => AstDialectVersion::LuaJit,
            DecompileDialect::Luau => AstDialectVersion::Luau,
        }
    }
}

/// 一次主反编译调用的顶层选项。
#[derive(Debug, Clone, PartialEq)]
pub struct DecompileOptions {
    pub dialect: DecompileDialect,
    pub parse: ParseOptions,
    pub target_stage: DecompileStage,
    pub debug: DebugOptions,
    pub readability: ReadabilityOptions,
    pub naming: NamingOptions,
    pub generate: GenerateOptions,
}

impl Default for DecompileOptions {
    fn default() -> Self {
        Self {
            dialect: DecompileDialect::Lua51,
            parse: ParseOptions {
                mode: ParseMode::Permissive,
                string_encoding: StringEncoding::Utf8,
                string_decode_mode: StringDecodeMode::Strict,
            },
            // 默认更偏向直接拿到最终源码，仓库内 CLI / wasm / 集成调用方都共享这套预期。
            target_stage: DecompileStage::Generate,
            debug: DebugOptions::default(),
            readability: ReadabilityOptions {
                return_inline_max_complexity: 10,
                index_inline_max_complexity: 10,
                args_inline_max_complexity: 6,
                access_base_inline_max_complexity: 5,
            },
            naming: NamingOptions {
                mode: NamingMode::DebugLike,
                debug_like_include_function: true,
            },
            generate: GenerateOptions::default(),
        }
    }
}

impl DecompileOptions {
    pub(crate) fn normalized(mut self) -> Self {
        if self.debug.enable && self.debug.output_stages.is_empty() && !self.debug.timing {
            self.debug.output_stages.push(self.target_stage);
        }
        self
    }
}
