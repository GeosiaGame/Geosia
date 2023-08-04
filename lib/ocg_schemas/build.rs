
use capnpc::CompilerCommand as Capnp;

fn main() {
    Capnp::new()
        .src_prefix("capnp/")
        .import_path("capnp/")
        .file("capnp/game_types.capnp")
        .run()
        .expect("compiling capnp schema");
}
