fn main() {
    #[cfg(feature = "regenerate-capnp")]
    {
        use std::path::Path;

        use capnpc::CompilerCommand as Capnp;

        let generated = Path::new("capnp-generated/");

        if generated.is_dir() {
            std::fs::remove_dir_all(generated).expect("Could not clear the capnp-generated directory");
        }

        Capnp::new()
            .src_prefix("capnp/")
            .import_path("capnp/")
            .output_path(generated)
            .file("capnp/game_types.capnp")
            .file("capnp/network.capnp")
            .file("capnp/voxel_mesh.capnp")
            .run()
            .expect("compiling capnp schema");
    }
}
