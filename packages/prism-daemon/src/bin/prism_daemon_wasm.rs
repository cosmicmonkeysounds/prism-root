//! Empty entry point for the `wasm32-unknown-emscripten` build.
//!
//! Emscripten's linker wants a `main` symbol — even though everything
//! the browser actually calls lives in [`prism_daemon::wasm`] and is
//! exposed through `#[no_mangle] extern "C"`. Without a bin target the
//! emcc link step fails with `undefined symbol: main`.
//!
//! Gated on the `wasm` feature via `required-features` in Cargo.toml,
//! so desktop / mobile / CLI builds never compile it. The CLI and this
//! wasm bin are mutually exclusive — one assumes tokio+stdio, the other
//! assumes emscripten+JS — so they live in separate binaries.

#![cfg(feature = "wasm")]

fn main() {
    // Keep the C-ABI adapter's symbols alive across rustc's LTO pass.
    // `#[no_mangle]` alone is not enough under `lto = "fat"` for a lib
    // crate whose symbols are only reachable from outside the Rust
    // world — rustc can still mark them unreachable and drop them
    // before emcc's linker (which honours `-sEXPORTED_FUNCTIONS` from
    // `.cargo/config.toml`) gets to see them. Taking a function pointer
    // to each symbol, feeding it through `black_box`, and then walking
    // away is the idiomatic "just keep this around" hint.
    std::hint::black_box(prism_daemon::wasm::prism_daemon_create as *const () as usize);
    std::hint::black_box(prism_daemon::wasm::prism_daemon_destroy as *const () as usize);
    std::hint::black_box(prism_daemon::wasm::prism_daemon_invoke as *const () as usize);
    std::hint::black_box(prism_daemon::wasm::prism_daemon_free_string as *const () as usize);
    // Emscripten's runtime keeps ticking after `main` returns because
    // `.cargo/config.toml` passes `-sNO_EXIT_RUNTIME=1`. JS calls into
    // ccall/cwrap happen after this function has already returned.
}
