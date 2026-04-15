//! Empty entry point for the `wasm32-unknown-emscripten` build.
//!
//! Emscripten's linker insists on a `main` symbol even though
//! everything the browser actually calls lives in
//! [`prism_shell::web`] and is exposed through `#[no_mangle] extern
//! "C"`. Mirror the pattern used by `prism-daemon/src/bin/
//! prism_daemon_wasm.rs`: provide a tiny main that black-boxes the
//! exported functions so rustc's LTO pass cannot drop them before
//! emcc's linker gets to see them.
//!
//! Gated on the `web` feature via `required-features` in Cargo.toml,
//! so desktop / CLI builds never compile it.

#![cfg(feature = "web")]

fn main() {
    // Keep the C-ABI adapter's symbols alive across rustc's LTO
    // pass. `#[no_mangle]` alone is not enough under `lto = "fat"`
    // because the symbols are only reachable from outside the Rust
    // world — rustc can still mark them unreachable and drop them
    // before emcc's linker (which honours `-sEXPORTED_FUNCTIONS` from
    // `.cargo/config.toml`) gets to see them.
    std::hint::black_box(prism_shell::web::prism_shell_boot as *const () as usize);
    std::hint::black_box(prism_shell::web::prism_shell_shutdown as *const () as usize);
    std::hint::black_box(prism_shell::web::prism_shell_resize as *const () as usize);
    std::hint::black_box(prism_shell::web::prism_shell_pointer_move as *const () as usize);
    std::hint::black_box(prism_shell::web::prism_shell_pointer_button as *const () as usize);
    std::hint::black_box(prism_shell::web::prism_shell_wheel as *const () as usize);
    std::hint::black_box(prism_shell::web::prism_shell_key as *const () as usize);
    std::hint::black_box(prism_shell::web::prism_shell_frame as *const () as usize);
    std::hint::black_box(prism_shell::web::prism_shell_frame_len as *const () as usize);
}
