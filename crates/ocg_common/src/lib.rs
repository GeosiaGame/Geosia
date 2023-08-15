#![warn(missing_docs)]
#![deny(clippy::disallowed_types)]

#![feature(arc_unwrap_or_clone)]
//! The common client&server code for OpenCubeGame

pub mod voxel;

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::mpsc::{sync_channel, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;

use bevy::diagnostic::DiagnosticsPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::time::TimePlugin;

/// A struct to communicate with the "server"-side engine that runs the game simulation.
/// It has its own bevy App with a very limited set of plugins enabled to be able to run without a graphical user interface.
pub struct GameEngine {
    thread: JoinHandle<()>,
    pause: AtomicBool,
}

impl GameEngine {
    /// Spawns a new thread that runs the engine in a paused state, and returns a handle to control it.
    pub fn new() -> Arc<Self> {
        let (tx, rx) = sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("OCG Engine Thread".to_owned())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || GameEngine::thread_main(rx))
            .expect("Could not create a thread for the engine");
        let engine = Self {
            thread,
            pause: AtomicBool::new(true),
        };
        let engine = Arc::new(engine);
        tx.send(Arc::clone(&engine))
            .expect("Could not pass initialization data to the engine thread");
        engine
    }

    /// Checks if the game logic is paused.
    pub fn is_paused(&self) -> bool {
        self.pause.load(SeqCst)
    }

    /// Sets the paused state for game logic, returns the previous state.
    pub fn set_paused(&mut self, paused: bool) -> bool {
        self.pause.swap(paused, SeqCst)
    }

    /// Checks if the engine thread is still alive.
    pub fn is_alive(&self) -> bool {
        !self.thread.is_finished()
    }

    fn thread_main(engine: Receiver<Arc<GameEngine>>) {
        let _engine = {
            let e = engine
                .recv()
                .expect("Could not receive initialization data in the engine thread");
            drop(engine); // force-drop the receiver early to not hold onto its memory
            e
        };
        let mut app = App::new();
        app.add_plugins(LogPlugin::default())
            .add_plugins(TaskPoolPlugin::default())
            .add_plugins(TypeRegistrationPlugin)
            .add_plugins(FrameCountPlugin)
            .add_plugins(TimePlugin)
            .add_plugins(TransformPlugin)
            .add_plugins(HierarchyPlugin)
            .add_plugins(DiagnosticsPlugin)
            .add_plugins(AssetPlugin::default())
            .add_plugins(AnimationPlugin);
        app.run();
    }
}
