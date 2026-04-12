package com.prism.daemon

import com.getcapacitor.JSObject
import com.getcapacitor.Plugin
import com.getcapacitor.PluginCall
import com.getcapacitor.PluginMethod
import com.getcapacitor.annotation.CapacitorPlugin
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer

/**
 * Capacitor plugin that exposes `prism-daemon`'s C ABI to the Android
 * WebView.
 *
 * ## How the Rust library is loaded
 *
 * The Rust kernel is cross-compiled to `libprism_daemon.so` (one per
 * ABI: arm64-v8a / armeabi-v7a / x86_64) by
 * `packages/prism-daemon/scripts/build-android.sh` via
 * `cargo ndk ... rustc --lib --crate-type cdylib`. Gradle picks the
 * right slice based on the device ABI at install time.
 *
 * ## Why JNA and not JNI
 *
 * JNI would require a shim with `Java_com_prism_daemon_...`-mangled
 * entry points, which means either a second C source file compiled
 * into the .so or a separate Rust module with
 * `#[export_name = "Java_..."]` attributes wrapping the plain C ABI.
 * Both are fragile: they couple the Rust symbol names to the Kotlin
 * package path, and any refactor on the Kotlin side would force a
 * matching Rust-side rename.
 *
 * JNA (Java Native Access) skips the mangling entirely — we declare a
 * Java interface whose methods map 1:1 to the C ABI by name
 * (`prism_daemon_create`, `prism_daemon_invoke`, …) and JNA resolves
 * them at runtime using the same `dlsym` the OS loader would. The
 * trade-off is a ~500 KB runtime dependency and a small per-call
 * overhead, both negligible for the invocation rate of a Capacitor
 * plugin (one call per user action, not a hot loop).
 */
@CapacitorPlugin(name = "PrismDaemon")
class PrismDaemonPlugin : Plugin() {
    /**
     * JNA-bound view of the Rust C ABI. Method names must match the
     * `#[no_mangle] pub extern "C"` symbols in
     * `packages/prism-daemon/src/wasm.rs` exactly — JNA resolves them
     * via `dlsym("libprism_daemon.so", "prism_daemon_create")` etc.
     */
    interface PrismDaemonLib : Library {
        fun prism_daemon_create(): Pointer?
        fun prism_daemon_destroy(kernel: Pointer?)
        fun prism_daemon_invoke(
            kernel: Pointer,
            name: String,
            payloadJson: String,
        ): Pointer?
        fun prism_daemon_free_string(ptr: Pointer?)

        companion object {
            val INSTANCE: PrismDaemonLib = Native.load(
                "prism_daemon",
                PrismDaemonLib::class.java,
            )
        }
    }

    private var kernel: Pointer? = null
    private val kernelLock = Any()

    override fun load() {
        // Lazy kernel creation; see `invoke()`.
    }

    override fun handleOnDestroy() {
        synchronized(kernelLock) {
            kernel?.let { PrismDaemonLib.INSTANCE.prism_daemon_destroy(it) }
            kernel = null
        }
    }

    @PluginMethod
    fun invoke(call: PluginCall) {
        val name = call.getString("name") ?: run {
            call.reject("invoke: missing `name`")
            return
        }
        val payloadJson = call.getString("payloadJson") ?: run {
            call.reject("invoke: missing `payloadJson`")
            return
        }

        val responseJson: String = synchronized(kernelLock) {
            if (kernel == null) {
                val created = PrismDaemonLib.INSTANCE.prism_daemon_create()
                    ?: run {
                        call.reject("prism_daemon_create returned null")
                        return
                    }
                kernel = created
            }

            val ptr = PrismDaemonLib.INSTANCE.prism_daemon_invoke(
                kernel!!,
                name,
                payloadJson,
            ) ?: run {
                call.reject("prism_daemon_invoke returned null")
                return
            }

            // Read the nul-terminated UTF-8 string out of the Rust
            // buffer, then hand the pointer back to Rust to free it.
            // Kotlin strings are copies, so the free is safe after
            // `getString`.
            try {
                ptr.getString(0, "UTF-8")
            } finally {
                PrismDaemonLib.INSTANCE.prism_daemon_free_string(ptr)
            }
        }

        val ret = JSObject().apply { put("responseJson", responseJson) }
        call.resolve(ret)
    }
}
