//! Mesh generators taking in voxel data and producing vertex data.

use anyhow::Context;
use bevy::color::palettes::tailwind;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError, VertexFormat,
};
use gs_schemas::coordinates::{AbsBlockPos, AbsChunkPos, RelBlockPos, CHUNK_DIM};
use gs_schemas::dependencies::itertools::iproduct;
use gs_schemas::direction::ALL_DIRECTIONS;
use gs_schemas::voxel::chunk_storage::ChunkStorage;
use gs_schemas::voxel::neighborhood::ChunkRefNeighborhood;
use gs_schemas::voxel::standard_shapes::{StandardShapeMetadata, VOXEL_NO_SHAPE};
use gs_schemas::voxel::voxeltypes::{BlockEntry, BlockRegistry};

use crate::voxel::ClientChunk;
use crate::ClientData;

/// Returns is a chunk has any blocks that require rendering a chunk mesh.
pub fn does_chunk_need_rendering(chunk: &ClientChunk, registry: &BlockRegistry) -> bool {
    chunk.blocks.palette_entries().iter().any(|pe| {
        registry
            .lookup_id_to_object(pe.id)
            .map(|blk| blk.has_drawable_mesh)
            .unwrap_or(false)
    })
}

// Dimming factor applied to the vertex color for each adjacent ambient-occluding block
const AO_OCCLUSION_FACTOR: f32 = 0.88;
// Just a random number per MeshVertexAttribute docs
const GEOSIA_VTX_ATTRIB_OFFSET: u64 = 745079851398183;
/// The vertex attribute encoding the index of the block in the chunk, for block-based shader effects.
pub const VERTEX_ATTRIBUTE_BLOCK_INDEX_WITH_FLAGS: MeshVertexAttribute = MeshVertexAttribute::new(
    "Vertex_BlockIndexWithFlags",
    GEOSIA_VTX_ATTRIB_OFFSET,
    VertexFormat::Uint32,
);
/// The vertex attribute encoding the barycentric offset for color attributes, used for correct quad color interpolation.
pub const VERTEX_ATTRIBUTE_BARYCENTRIC_COLOR_OFFSET: MeshVertexAttribute = MeshVertexAttribute::new(
    "Vertex_BarycentricColorOffset",
    GEOSIA_VTX_ATTRIB_OFFSET + 1,
    VertexFormat::Float32x3,
);

/// The [`MaterialExtension`] for chunk mesh rendering using Bevy, extending the standard PBR material.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ChunkMeshMaterialExtension {}

/// The full [`Material`] used for chunk mesh rendering.
pub type ChunkMeshMaterial = ExtendedMaterial<StandardMaterial, ChunkMeshMaterialExtension>;

/// Use this for rendering chunks without customizations.
pub fn default_chunk_material() -> ChunkMeshMaterial {
    ChunkMeshMaterial {
        base: StandardMaterial {
            base_color: tailwind::GRAY_500.into(),
            ..default()
        },
        extension: ChunkMeshMaterialExtension {},
    }
}

const SHADER_ASSET_PATH: &str = "shaders/chunk_mesh_main.wgsl";

impl MaterialExtension for ChunkMeshMaterialExtension {
    fn vertex_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }

    fn deferred_vertex_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(5),
            VERTEX_ATTRIBUTE_BARYCENTRIC_COLOR_OFFSET.at_shader_location(6),
            VERTEX_ATTRIBUTE_BLOCK_INDEX_WITH_FLAGS.at_shader_location(7),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// Creates a bevy mesh from a chunk, using neighboring chunks to determine culling&ambient occlusion information.
#[allow(clippy::cognitive_complexity)]
#[inline(never)]
pub fn mesh_from_chunk(registry: &BlockRegistry, chunks: &ChunkRefNeighborhood<ClientData>) -> anyhow::Result<Mesh> {
    // position relative to the central chunk
    #[inline(always)]
    fn get_block(chunks: &ChunkRefNeighborhood<ClientData>, position: AbsBlockPos) -> BlockEntry {
        let (chunk_pos, in_pos) = position.split_chunk_component();
        let chunk_pos = chunk_pos + (chunks.center_coord() - AbsChunkPos::ZERO);
        chunks.get(chunk_pos).unwrap().blocks.get_copy(in_pos)
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    let mut pos_buf: Vec<[f32; 3]> = Vec::with_capacity(6144);
    let mut normal_buf: Vec<[f32; 3]> = Vec::with_capacity(6144);
    let mut color_buf: Vec<[f32; 4]> = Vec::with_capacity(6144);
    let mut bidx_flag_buf: Vec<u32> = Vec::with_capacity(6144);
    let mut barycentric_buf: Vec<[f32; 3]> = Vec::with_capacity(6144);
    let mut ibuf: Vec<u32> = Vec::with_capacity(6144);

    for (cell_y, cell_z, cell_x) in iproduct!(0..CHUNK_DIM, 0..CHUNK_DIM, 0..CHUNK_DIM) {
        // Assume the chunk is at (0,0,0), mesh is translated using transforms elsewhere
        let ipos = AbsBlockPos::new(cell_x, cell_y, cell_z);
        let ventry = get_block(chunks, ipos);
        let vdef = registry.lookup_id_to_object(ventry.id).context("invalid block")?;
        let vstdmeta = StandardShapeMetadata::from_meta(ventry.metadata);
        let vshape = if vdef.has_drawable_mesh {
            vstdmeta.shape()
        } else {
            &VOXEL_NO_SHAPE
        };
        let vor = vstdmeta.orientation();
        let ipos_as_offset = ipos.split_chunk_component().1.as_index() as u32;

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
            let boff = barycentric_buf.len();
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
                let vnormal = vor_matf * vtx.normal;
                let position: [f32; 3] = [
                    ipos.x as f32 + voffset.x + 0.5,
                    ipos.y as f32 + voffset.y + 0.5,
                    ipos.z as f32 + voffset.z + 0.5,
                ];
                let normal: [f32; 3] = vnormal.to_array();
                // let texid = *vdef.texture_mapping.at_direction(rot_side_dir);
                let color = [
                    vdef.representative_color.r as f32 * ao,
                    vdef.representative_color.g as f32 * ao,
                    vdef.representative_color.b as f32 * ao,
                    1.0,
                ];
                barycentric_color_sum += vtx.barycentric_sign as f32 * Vec4::from(color);

                let mut idx_flags = ipos_as_offset;
                if vtx.barycentric.x > 0.1 {
                    idx_flags |= 1 << 17;
                }
                if vtx.barycentric.y > 0.1 {
                    idx_flags |= 1 << 18;
                }
                if vtx.barycentric.z > 0.1 {
                    idx_flags |= 1 << 19;
                }

                pos_buf.push(position);
                color_buf.push(color);
                normal_buf.push(normal);
                bidx_flag_buf.push(idx_flags);
                barycentric_buf.push([0.0; 3]); // initialized after the loop
            }
            let final_barycentric_sum = barycentric_color_sum.xyz().into();
            barycentric_buf[boff..].fill(final_barycentric_sum);
            ibuf.extend(side.indices.iter().map(|x| x + voff));
        }
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos_buf);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normal_buf);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, color_buf);
    mesh.insert_attribute(VERTEX_ATTRIBUTE_BLOCK_INDEX_WITH_FLAGS, bidx_flag_buf);
    mesh.insert_attribute(VERTEX_ATTRIBUTE_BARYCENTRIC_COLOR_OFFSET, barycentric_buf);
    mesh.insert_indices(Indices::U32(ibuf));

    Ok(mesh)
}
