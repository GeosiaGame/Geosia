// Based on bevy_pbr/render/mesh.wgsl
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{Vertex, VertexOutput, FragmentOutput},
    prepass::vertex,
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct ChunkVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    #ifdef VERTEX_UVS_A
        @location(2) uv: vec2<f32>,
    #endif
    #ifdef VERTEX_UVS_B
        @location(3) uv_b: vec2<f32>,
    #endif
    #ifdef VERTEX_TANGENTS
        @location(4) tangent: vec4<f32>,
    #endif
    @location(5) color: vec4<f32>,
    @location(6) barycentric_color_offset: vec3<f32>,
    @location(7) block_index_with_flags: u32,
}

struct ChunkVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
#ifdef VERTEX_UVS_A
    @location(2) uv: vec2<f32>,
#endif
#ifdef VERTEX_UVS_B
    @location(3) uv_b: vec2<f32>,
#endif
#ifdef VERTEX_TANGENTS
    @location(4) world_tangent: vec4<f32>,
#endif
    @location(5) color: vec4<f32>,
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    @location(6) @interpolate(flat) instance_index: u32,
#endif
    @location(7) barycentric_coords: vec3<f32>,
    @location(8) barycentric_color_offset: vec3<f32>,
    @location(9) block_index: u32,
}

/*
// If we decide we need material-wide uniforms, here's how to add them

struct ChunkMeshMaterial {
}

@group(2) @binding(100)
var<uniform> chunk_mesh_material: ChunkMeshMaterial;
*/

#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions,
    skinning,
    morph::morph,
    view_transformations::position_world_to_clip,
}

@vertex
fn vertex(vertex: ChunkVertex) -> ChunkVertexOutput {
    var out: ChunkVertexOutput;

    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);

    // Use vertex.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
    // See https://github.com/gfx-rs/naga/issues/2416 .
    var world_from_local = mesh_world_from_local;

#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        // Use vertex.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
        // See https://github.com/gfx-rs/naga/issues/2416
        vertex.instance_index
    );
#endif

    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif

#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        vertex.tangent,
        // Use vertex.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
        // See https://github.com/gfx-rs/naga/issues/2416
        vertex.instance_index
    );
#endif

    out.color = vertex.color;

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    // Use vertex.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
    // See https://github.com/gfx-rs/naga/issues/2416
    out.instance_index = vertex.instance_index;
#endif

#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index, mesh_world_from_local[3]);
#endif

    var index = vertex.block_index_with_flags;
    var baryx = f32((index & (1u << 17u)) > 0u);
    var baryy = f32((index & (1u << 18u)) > 0u);
    var baryz = f32((index & (1u << 19u)) > 0u);
    out.barycentric_coords = vec3<f32>(baryx, baryy, baryz);
    out.barycentric_color_offset = vertex.barycentric_color_offset;
    out.block_index = vertex.block_index_with_flags & ((1u << 17u) - 1u);

    return out;
}

@fragment
fn fragment(
    chunk_in: ChunkVertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in: VertexOutput;
    in.position = chunk_in.position;
    in.world_position = chunk_in.world_position;
    in.world_normal = chunk_in.world_normal;
#ifdef VERTEX_UVS_A
    in.uv = chunk_in.uv;
#endif
#ifdef VERTEX_UVS_B
    in.uv_b = chunk_in.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    in.world_tangent = chunk_in.world_tangent;
#endif
    in.color = chunk_in.color;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    in.instance_index = chunk_in.instance_index;
#endif

    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // we can optionally modify the input before lighting and alpha_discard is applied
    var corrected_v_color = pbr_input.material.base_color.xyz + chunk_in.barycentric_coords.x * chunk_in.barycentric_coords.y * chunk_in.barycentric_color_offset;
    pbr_input.material.base_color = vec4(corrected_v_color, pbr_input.material.base_color.w);

    // alpha discard
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    // in deferred mode we can't modify anything after that, as lighting is run in a separate fullscreen shader.
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    // apply lighting
    out.color = apply_pbr_lighting(pbr_input);

    // apply in-shader post processing (fog, alpha-premultiply, and also tonemapping, debanding if the camera is non-hdr)
    // note this does not include fullscreen postprocessing effects like bloom.
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
