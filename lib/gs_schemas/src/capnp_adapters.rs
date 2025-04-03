//! Contains conversion implementations for simple capnp readers.
//! These should only be used if the Rust struct is necessary for passing into a method,
//! as constructing a rust struct from capnp data removes the main benefit of using capnp.

use bevy_math::{IVec3, Vec3};

use crate::actions::BlockAction;
use crate::actions::BlockAction::{BreakBlock, PlaceBlock};
use crate::schemas::game_types_capnp::{block_action, i_vec3, vec3};

pub fn adapt_i_vec3(reader: i_vec3::Reader) -> IVec3 {
    IVec3 {
        x: reader.get_x(),
        y: reader.get_y(),
        z: reader.get_z(),
    }
}

pub fn adapt_vec3(reader: vec3::Reader) -> Vec3 {
    Vec3 {
        x: reader.get_x(),
        y: reader.get_y(),
        z: reader.get_z(),
    }
}

pub fn adapt_block_action(reader: block_action::Reader) -> Result<BlockAction, ()> {
    match reader.which() {
        Ok(block_action::Which::BreakBlock(_)) => Ok(BreakBlock()),
        Ok(block_action::Which::PlaceBlock(_)) => Ok(PlaceBlock()),
        _ => Err(()),
    }
}
