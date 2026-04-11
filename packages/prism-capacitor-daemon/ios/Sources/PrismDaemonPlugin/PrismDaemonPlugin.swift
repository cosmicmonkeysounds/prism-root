// Swift CAPPlugin wrapping prism-daemon's C ABI for iOS.
//
// The Rust library is linked as a staticlib via the PrismDaemonFFI
// binary target (see ../../../Package.swift). The C ABI symbols it
// exports (`prism_daemon_create`, `prism_daemon_invoke`,
// `prism_daemon_free_string`, `prism_daemon_destroy`) are declared in
// include/PrismDaemon.h and re-exported through module.modulemap so
// Swift can call them directly.
//
// Threading: we hold a single kernel per plugin instance. The kernel is
// Arc<...> on the Rust side so concurrent invokes are safe, but we
// serialize access to the kernel pointer with `kernelQueue` anyway —
// Capacitor dispatches plugin methods on a background queue, and the
// single-kernel-per-app model doesn't benefit from parallel invokes.

import Capacitor
import Foundation
import PrismDaemonFFI

@objc(PrismDaemonPlugin)
public class PrismDaemonPlugin: CAPPlugin, CAPBridgedPlugin {
    public let identifier = "PrismDaemonPlugin"
    public let jsName = "PrismDaemon"
    public let pluginMethods: [CAPPluginMethod] = [
        CAPPluginMethod(name: "invoke", returnType: CAPPluginReturnPromise)
    ]

    // Opaque handle returned by `prism_daemon_create`. Lazily created on
    // first `invoke` so plugin load doesn't pay the Luau-init cost
    // unless the app actually calls into the daemon.
    private var kernel: OpaquePointer?
    private let kernelQueue = DispatchQueue(label: "com.prism.daemon.kernel")

    public override func load() {
        // Intentionally empty — we want lazy kernel creation. See the
        // comment on `kernel`.
    }

    deinit {
        // `load()` and `deinit` run on the main thread; take the lock
        // anyway to keep the teardown path symmetric with `invoke`.
        kernelQueue.sync {
            if let k = kernel {
                prism_daemon_destroy(k)
                kernel = nil
            }
        }
    }

    @objc func invoke(_ call: CAPPluginCall) {
        guard let name = call.getString("name") else {
            call.reject("invoke: missing `name`")
            return
        }
        guard let payloadJson = call.getString("payloadJson") else {
            call.reject("invoke: missing `payloadJson`")
            return
        }

        kernelQueue.async { [weak self] in
            guard let self = self else { return }

            // Lazy kernel creation. `prism_daemon_create` returns nil
            // only if two modules try to register the same command —
            // a compile-time invariant of the Rust builder, so in
            // practice this never happens in a shipping build.
            if self.kernel == nil {
                guard let created = prism_daemon_create() else {
                    call.reject("prism_daemon_create returned null")
                    return
                }
                self.kernel = created
            }

            // The C ABI takes nul-terminated UTF-8 strings. `name` and
            // `payloadJson` are Swift `String` values; `withCString`
            // hands us a valid `UnsafePointer<CChar>` for the closure's
            // duration, which is exactly what we need.
            let responsePtr: UnsafeMutablePointer<CChar>? = name.withCString { namePtr in
                payloadJson.withCString { payloadPtr in
                    prism_daemon_invoke(self.kernel!, namePtr, payloadPtr)
                }
            }

            guard let ptr = responsePtr else {
                call.reject("prism_daemon_invoke returned null")
                return
            }
            // Always free the Rust-allocated string, even if parsing
            // fails downstream — this mirrors `defer` on the Rust
            // side's ownership contract.
            defer { prism_daemon_free_string(ptr) }

            let responseJson = String(cString: ptr)
            call.resolve(["responseJson": responseJson])
        }
    }
}
