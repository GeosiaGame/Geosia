//! Mesh generators taking in voxel data and producing vertex data.

use bevy::asset::RenderAssetUsages;
use bevy::color::palettes::tailwind;
use bevy::material::OpaqueRendererMethod;
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError, VertexFormat,
};
use bevy::shader::ShaderRef;
use gs_schemas::coordinates::{AbsBlockPos, AbsChunkPos, CHUNK_DIM, RelBlockPos};
use gs_schemas::dependencies::itertools::iproduct;
use gs_schemas::direction::ALL_DIRECTIONS;
use gs_schemas::voxel::chunk_storage::ChunkStorage;
use gs_schemas::voxel::neighborhood::ChunkRefNeighborhood;
use gs_schemas::voxel::standard_shapes::{StandardShapeMetadata, VOXEL_NO_SHAPE};
use gs_schemas::voxel::voxeltypes::{BlockEntry, BlockRegistry};

use crate::ClientData;
use crate::prelude::*;
use crate::voxel::ClientChunk;

/// Returns is a chunk has any blocks that require rendering a chunk mesh.
pub fn does_chunk_need_rendering(chunk: &ClientChunk, registry: &BlockRegistry) -> bool {
    chunk.blocks.palette_entries().iter().any(|pe| {
        registry
            .lookup_id_to_object(pe.id)
            .is_some_and(|blk| blk.has_drawable_mesh)
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
            base_color: tailwind::GRAY_100.into(),
            opaque_render_method: OpaqueRendererMethod::Auto,
            perceptual_roughness: 1.0,
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

    fn fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }

    fn deferred_vertex_shader() -> ShaderRef {
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
            //Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            //Mesh::ATTRIBUTE_UV_1.at_shader_location(3),
            //Mesh::ATTRIBUTE_TANGENT.at_shader_location(4),
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
    let mut block_index_flag_buf: Vec<u32> = Vec::with_capacity(6144);
    let mut barycentric_buf: Vec<[f32; 3]> = Vec::with_capacity(6144);
    let mut ibuf: Vec<u32> = Vec::with_capacity(6144);

    for (cell_y, cell_z, cell_x) in iproduct!(0..CHUNK_DIM, 0..CHUNK_DIM, 0..CHUNK_DIM) {
        // Assume the chunk is at (0,0,0), mesh is translated using transforms elsewhere
        let ipos = AbsBlockPos::new(cell_x, cell_y, cell_z);
        let voxel_entry = get_block(chunks, ipos);
        let voxel_def = registry.lookup_id_to_object(voxel_entry.id).context("invalid block")?;
        let voxel_std_meta = StandardShapeMetadata::from_meta(voxel_entry.metadata);
        let voxel_shape = if voxel_def.has_drawable_mesh {
            voxel_std_meta.shape()
        } else {
            &VOXEL_NO_SHAPE
        };
        let voxel_orientation = voxel_std_meta.orientation();
        let ipos_as_offset = ipos.split_chunk_component().1.as_index() as u32;

        if !voxel_def.has_drawable_mesh {
            continue;
        }

        for &side_dir in &ALL_DIRECTIONS {
            let rot_side_dir = voxel_orientation.unapply_to_dir(side_dir);
            let side = &voxel_shape.sides[rot_side_dir.as_index()];
            if side.indices.is_empty() {
                continue;
            }
            let side_offset = RelBlockPos::from(side_dir.as_ivec());

            // hidden face removal
            let touch_side = side_dir.opposite();
            let touch_pos = ipos + side_offset;
            let touch_entry = get_block(chunks, touch_pos);
            let touch_def = registry.lookup_id_to_object(touch_entry.id).context("invalid block")?;
            let touch_std_meta = StandardShapeMetadata::from_meta(touch_entry.metadata);
            let touch_shape = if touch_def.has_drawable_mesh {
                touch_std_meta.shape()
            } else {
                &VOXEL_NO_SHAPE
            };
            let touch_orientation = touch_std_meta.orientation();
            let touch_rot_side = touch_orientation.unapply_to_dir(touch_side);
            let touch_shape_side = &touch_shape.sides[touch_rot_side.as_index()];

            if side.can_be_clipped && touch_def.has_drawable_mesh && touch_shape_side.can_clip {
                continue;
            }

            let pos_buf_offset = pos_buf.len() as u32;
            let barycentric_buf_offset = barycentric_buf.len();
            let mut barycentric_color_sum: Vec4 = Vec4::ZERO;
            let voxel_orientation_matrix = voxel_orientation.to_matrix();
            for vtx in side.vertices.iter() {
                // Ambient Occlusion
                let mut ambient_occlusion = 1.0;
                for &ao_offset in vtx.ao_offsets.iter() {
                    let pos = ipos + RelBlockPos::from(voxel_orientation.unapply_to_ivec(ao_offset));
                    let block_entry = get_block(chunks, pos);
                    let block_def = registry.lookup_id_to_object(block_entry.id).context("invalid block")?;
                    let block_std_meta = StandardShapeMetadata::from_meta(block_entry.metadata);
                    let block_shape = if block_def.has_drawable_mesh {
                        block_std_meta.shape()
                    } else {
                        &VOXEL_NO_SHAPE
                    };
                    if block_shape.causes_ambient_occlusion {
                        ambient_occlusion *= AO_OCCLUSION_FACTOR;
                    }
                }

                let vertex_offset = voxel_orientation_matrix * vtx.offset;
                let vertex_normal = voxel_orientation_matrix * vtx.normal;
                let position: [f32; 3] = [
                    ipos.x as f32 + vertex_offset.x + 0.5,
                    ipos.y as f32 + vertex_offset.y + 0.5,
                    ipos.z as f32 + vertex_offset.z + 0.5,
                ];
                let normal: [f32; 3] = vertex_normal.to_array();
                // let texid = *vdef.texture_mapping.at_direction(rot_side_dir);
                let color = [
                    voxel_def.representative_color.red * ambient_occlusion,
                    voxel_def.representative_color.green * ambient_occlusion,
                    voxel_def.representative_color.blue * ambient_occlusion,
                    1.0,
                ];
                barycentric_color_sum += vtx.barycentric_sign * Vec4::from(color);

                let mut block_index_with_flags = ipos_as_offset;
                if vtx.barycentric.x > 0.1 {
                    block_index_with_flags |= 1 << 17;
                }
                if vtx.barycentric.y > 0.1 {
                    block_index_with_flags |= 1 << 18;
                }

                pos_buf.push(position);
                color_buf.push(color);
                normal_buf.push(normal);
                block_index_flag_buf.push(block_index_with_flags);
                barycentric_buf.push([0.0; 3]); // initialized after the loop
            }
            let final_barycentric_sum = barycentric_color_sum.xyz().into();
            barycentric_buf[barycentric_buf_offset..].fill(final_barycentric_sum);
            ibuf.extend(side.indices.iter().map(|x| x + pos_buf_offset));
        }
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos_buf);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normal_buf);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, color_buf);
    mesh.insert_attribute(VERTEX_ATTRIBUTE_BLOCK_INDEX_WITH_FLAGS, block_index_flag_buf);
    mesh.insert_attribute(VERTEX_ATTRIBUTE_BARYCENTRIC_COLOR_OFFSET, barycentric_buf);
    mesh.insert_indices(Indices::U32(ibuf));

    Ok(mesh)
}
