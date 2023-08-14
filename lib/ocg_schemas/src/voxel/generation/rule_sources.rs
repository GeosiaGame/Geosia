//! Default Rule Source types.

use serde::{Serialize, Deserialize};

use crate::voxel::biome::{RuleSrc, ConditionSrc};
use crate::voxel::voxeltypes::BlockEntry;

use super::RuleSource;

/// Empty Rule source. Does nothing.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EmptyRuleSource();

impl RuleSource for EmptyRuleSource {
    fn place(self: &mut Self, pos: &bevy_math::IVec3, context: &super::Context, block_registry: &crate::voxel::voxeltypes::BlockRegistry) -> Option<BlockEntry> {
        None
    }
}

#[derive(Clone, Debug)]
/// Rule source that runs `result`, but only if `condition` succeeds.
pub struct ConditionRuleSource {
    /// The condition to test for.
    condition: &'static ConditionSrc,
    /// The Function to run.
    result: &'static RuleSrc,
}

impl ConditionRuleSource {
    /// Helper for creating new `ConditionRuleSource`s
    pub fn new(condition: &'static ConditionSrc, result: &'static RuleSrc) -> Self {
        Self {
            condition: condition,
            result: result
        }
    }
}

impl RuleSource for ConditionRuleSource {
    fn place(self: &mut Self, pos: &bevy_math::IVec3, context: &super::Context, block_registry: &crate::voxel::voxeltypes::BlockRegistry) -> Option<BlockEntry> {
        if self.condition.test(*pos, context) {
            self.result.place(pos, context, block_registry)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
/// Rule source that runs `result`s until one returns non-empty.
pub struct ChainRuleSource {
    /// The Function(s) to run.
    rules: Vec<&'static RuleSrc>,
}

impl ChainRuleSource {
    /// Helper for creating new `ChainRuleSource`s
    pub fn new(rules: Vec<&'static RuleSrc>) -> Self {
        Self {
            rules: rules
        }
    }
}

impl RuleSource for ChainRuleSource {
    fn place(self: &mut Self, pos: &bevy_math::IVec3, context: &super::Context, block_registry: &crate::voxel::voxeltypes::BlockRegistry) -> Option<BlockEntry> {
        for rule in self.rules.iter_mut() {
            let result = rule.place(pos, context, block_registry);
            if result.is_some() {
                return result;
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
/// Rule source that returns a BlockEntry.
pub struct BlockRuleSource {
    /// The `BlockEntry` this returns.
    entry: BlockEntry,
}

impl BlockRuleSource {
    /// Helper for creating new `BlockRuleSource`s
    pub fn new(entry: BlockEntry) -> Self {
        Self {
            entry: entry
        }
    }
}

impl RuleSource for BlockRuleSource {
    fn place(self: &mut Self, _pos: &bevy_math::IVec3, _context: &super::Context, _block_registry: &crate::voxel::voxeltypes::BlockRegistry) -> Option<BlockEntry> {
        Some(self.entry)
    }
}