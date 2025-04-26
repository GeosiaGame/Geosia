//! The clientside of Geosia - the main binary

use gs_client::client_main;
use gs_common::geosia_pre_main;

fn main() {
    geosia_pre_main();
    client_main()
}
