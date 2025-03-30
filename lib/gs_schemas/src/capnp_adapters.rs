//! Contains conversion implementations for simple capnp readers.
//! These should only be used if the Rust struct is necessary for passing into a method,
//! as constructing a rust struct from capnp data removes the main benefit of using capnp.

use bevy_math::{IVec3, Vec3};

use crate::actions::ThrowAction;
use crate::actions::ThrowAction::{ThrowBlock, ThrowItem};
use crate::schemas::game_types_capnp::{i_vec3, throw_action, vec3};

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

pub fn adapt_throw_action(reader: throw_action::Reader) -> Result<ThrowAction, ()> {
    match reader.which() {
        Ok(throw_action::Which::ThrowItem(_)) => Ok(ThrowItem()),
        Ok(throw_action::Which::ThrowBlock(_)) => Ok(ThrowBlock()),
        _ => Err(()),
    }
}
