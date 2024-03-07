#![warn(missing_docs)]
#![deny(clippy::disallowed_types)]
#![allow(clippy::type_complexity)]

//! The clientside of OpenCubeGame - the main binary

use ocg_client::client_main;

fn main() {
    client_main()
}
