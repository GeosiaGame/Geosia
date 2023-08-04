fn main() {
    #[cfg(feature = "regenerate-capnp")]
    {
        use capnpc::CompilerCommand as Capnp;

        std::fs::remove_dir_all("capnp-generated/");

        Capnp::new()
            .src_prefix("capnp/")
            .import_path("capnp/")
            .output_path("capnp-generated/")
            .file("capnp/game_types.capnp")
            .run()
            .expect("compiling capnp schema");
    }
}
