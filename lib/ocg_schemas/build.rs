fn main() {
    #[cfg(feature = "regenerate-capnp")]
    {
        use capnpc::CompilerCommand as Capnp;

        std::fs::remove_dir_all("capnp-generated/").expect("Could not clear the capnp-generated directory");

        Capnp::new()
            .src_prefix("capnp/")
            .import_path("capnp/")
            .output_path("capnp-generated/")
            .file("capnp/game_types.capnp")
            .file("capnp/network.capnp")
            .file("capnp/voxel_mesh.capnp")
            .run()
            .expect("compiling capnp schema");
    }
}
