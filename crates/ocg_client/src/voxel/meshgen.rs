//! Mesh generators taking in voxel data and producing vertex data.

use anyhow::Context;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use ocg_schemas::coordinates::{AbsBlockPos, AbsChunkPos, RelBlockPos, CHUNK_DIM};
use ocg_schemas::dependencies::itertools::iproduct;
use ocg_schemas::direction::ALL_DIRECTIONS;
use ocg_schemas::voxel::chunk_storage::ChunkStorage;
use ocg_schemas::voxel::neighborhood::ChunkRefNeighborhood;
use ocg_schemas::voxel::standard_shapes::{StandardShapeMetadata, VOXEL_NO_SHAPE};
use ocg_schemas::voxel::voxeltypes::{BlockEntry, BlockRegistry};

use crate::voxel::{ClientChunk, ClientChunkData};

/// Returns is a chunk has any blocks that require rendering a chunk mesh.
pub fn does_chunk_need_rendering(chunk: &ClientChunk, registry: &BlockRegistry) -> bool {
    chunk.blocks.palette_entries().iter().any(|pe| {
        registry
            .lookup_id_to_object(pe.id)
            .map(|blk| blk.has_drawable_mesh)
            .unwrap_or(false)
    })
}

const AO_OCCLUSION_FACTOR: f32 = 0.88;

/// Creates a bevy mesh from a chunk, using neighboring chunks to determine culling&ambient occlusion information.
#[allow(clippy::cognitive_complexity)]
#[inline(never)]
pub fn mesh_from_chunk(
    registry: &BlockRegistry,
    chunks: &ChunkRefNeighborhood<ClientChunkData>,
) -> anyhow::Result<Mesh> {
    // position relative to the central chunk
    #[inline(always)]
    fn get_block(chunks: &ChunkRefNeighborhood<ClientChunkData>, position: AbsBlockPos) -> BlockEntry {
        let (chunk_pos, in_pos) = position.split_chunk_component();
        chunks.get(chunk_pos).unwrap().blocks.get_copy(in_pos)
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);
    let mut pos_buf: Vec<[f32; 3]> = Vec::with_capacity(6144);
    let mut normal_buf: Vec<[f32; 3]> = Vec::with_capacity(6144);
    let mut color_buf: Vec<[f32; 4]> = Vec::with_capacity(6144);
    let mut ibuf: Vec<u32> = Vec::with_capacity(6144);

    let block_origin = AbsBlockPos::from(chunks.center_coord());
    for (cell_y, cell_z, cell_x) in iproduct!(0..CHUNK_DIM, 0..CHUNK_DIM, 0..CHUNK_DIM) {
        let ipos: AbsBlockPos = block_origin + RelBlockPos::new(cell_x, cell_y, cell_z);
        let ventry = get_block(chunks, ipos);
        let vdef = registry.lookup_id_to_object(ventry.id).context("invalid block")?;
        let vstdmeta = StandardShapeMetadata::from_meta(ventry.metadata);
        let vshape = if vdef.has_drawable_mesh {
            vstdmeta.shape()
        } else {
            &VOXEL_NO_SHAPE
        };
        let vor = vstdmeta.orientation();

        if !vdef.has_drawable_mesh {
            continue;
        }

        for &side_dir in &ALL_DIRECTIONS {
            let rot_side_dir = vor.unapply_to_dir(side_dir);
            let side = &vshape.sides[rot_side_dir.to_index()];
            if side.indices.is_empty() {
                continue;
            }
            let ioffset = RelBlockPos::from(side_dir.to_ivec());

            // hidden face removal
            let touchside = side_dir.opposite();
            let touchpos = ipos + ioffset;
            let tentry = get_block(chunks, touchpos);
            let tdef = registry.lookup_id_to_object(tentry.id).context("invalid block")?;
            let tstdmeta = StandardShapeMetadata::from_meta(tentry.metadata);
            let tshape = if tdef.has_drawable_mesh {
                tstdmeta.shape()
            } else {
                &VOXEL_NO_SHAPE
            };
            let tor = tstdmeta.orientation();
            let touchrotside = tor.unapply_to_dir(touchside);
            let tside = &tshape.sides[touchrotside.to_index()];

            if side.can_be_clipped && tdef.has_drawable_mesh && tside.can_clip {
                continue;
            }

            let voff = pos_buf.len() as u32;
            let mut barycentric_color_sum: Vec4 = Vec4::ZERO;
            let vor_matf = vor.to_matrix();
            for vtx in side.vertices.iter() {
                // Ambient Occlusion
                let mut ao = 1.0;
                for &ao_off in vtx.ao_offsets.iter() {
                    let pos = ipos + RelBlockPos::from(vor.apply_to_ivec(ao_off));
                    let bentry = get_block(chunks, pos);
                    let bdef = registry.lookup_id_to_object(bentry.id).context("invalid block")?;
                    let bstdmeta = StandardShapeMetadata::from_meta(bentry.metadata);
                    let bshape = if bdef.has_drawable_mesh {
                        bstdmeta.shape()
                    } else {
                        &VOXEL_NO_SHAPE
                    };
                    if bshape.causes_ambient_occlusion {
                        ao *= AO_OCCLUSION_FACTOR;
                    }
                }

                let voffset = vor_matf * vtx.offset;
                let position: [f32; 3] = [
                    ipos.x as f32 + voffset.x + 0.5,
                    ipos.y as f32 + voffset.y + 0.5,
                    ipos.z as f32 + voffset.z + 0.5,
                ];
                let normal: [f32; 3] = vtx.normal.to_array();
                // let texid = *vdef.texture_mapping.at_direction(rot_side_dir);
                let color = [
                    vdef.representative_color.r as f32 * ao,
                    vdef.representative_color.g as f32 * ao,
                    vdef.representative_color.b as f32 * ao,
                    1.0,
                ];
                barycentric_color_sum += vtx.barycentric_sign as f32 * Vec4::from(color);

                pos_buf.push(position);
                color_buf.push(color);
                normal_buf.push(normal);
                /*
                let mut idx_flags = ic_vidx as i32;
                if vtx.barycentric.x > 0.1 {
                    idx_flags |= 1 << 17;
                }
                if vtx.barycentric.y > 0.1 {
                    idx_flags |= 1 << 18;
                }
                if vtx.barycentric.z > 0.1 {
                    idx_flags |= 1 << 19;
                }
                vbuf.push(VoxelVertex {
                    position: as_wxyz10(position.map(|p| p / CHUNK_DIM as f32)),
                    color: as_rgba8(color),
                    texcoord: [vtx.texcoord.x, vtx.texcoord.y, texid as f32],
                    index: idx_flags,
                    barycentric_color_offset: [0.0; 4], // initialized after the loop
                });
                 */
            }
            /*for v in &mut vbuf[voff as usize..] {
                v.barycentric_color_offset = barycentric_color_sum.into();
            }*/
            ibuf.extend(side.indices.iter().map(|x| x + voff));
        }
    }

    //warn!("Mesh of {} indices", ibuf.len());

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos_buf);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normal_buf);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, color_buf);
    mesh.set_indices(Some(Indices::U32(ibuf)));

    Ok(mesh)
}
