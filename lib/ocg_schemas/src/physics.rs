//! Physics-related types

use bitflags::bitflags;

bitflags! {
    /// Types of possible objects to hit via a raycast query
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    struct RaycastGroup: u32 {
        /// Any type of block
        const BLOCKS = Self::BLOCKS_SOLID.bits() | Self::BLOCKS_AIRY.bits() | Self::BLOCKS_TRANSPARENT.bits();
        /// Any type of fluid
        const FLUIDS = 0x2;
        /// Any type of entity
        const ENTITIES = 0x4;

        /// Solid blocks
        const BLOCKS_SOLID = 0x10;
        /// "Airy" blocks like grass blades
        const BLOCKS_AIRY = 0x20;
        /// Transparent blocks like glass
        const BLOCKS_TRANSPARENT = 0x40;
    }
}
