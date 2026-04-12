// swift-tools-version: 5.9
//
// Swift Package manifest for the Prism daemon Capacitor plugin (iOS).
//
// Capacitor 7 plugins can ship as either a CocoaPods pod or a Swift
// Package; we use Swift Package because it's the modern path and plays
// nicer with xcframeworks. The binary target points at the xcframework
// built by `scripts/build-ios.sh` — that script is responsible for
// producing `PrismDaemon.xcframework` containing the arm64-device and
// arm64-simulator slices of `libprism_daemon.a` compiled with
// `--features mobile`.

import PackageDescription

let package = Package(
    name: "PrismCapacitorDaemon",
    platforms: [.iOS(.v14)],
    products: [
        .library(
            name: "PrismCapacitorDaemon",
            targets: ["PrismDaemonPlugin"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/ionic-team/capacitor-swift-pm.git",
            from: "7.0.0"
        )
    ],
    targets: [
        // The xcframework is built by scripts/build-ios.sh and staged
        // into mobile/ios/Frameworks/. It bundles the two `libprism_daemon.a`
        // slices (device + simulator) with a shared modulemap + umbrella
        // header that re-exports the C ABI under the `PrismDaemonFFI`
        // module.
        .binaryTarget(
            name: "PrismDaemonFFI",
            path: "mobile/ios/Frameworks/PrismDaemon.xcframework"
        ),
        .target(
            name: "PrismDaemonPlugin",
            dependencies: [
                .product(
                    name: "Capacitor",
                    package: "capacitor-swift-pm"
                ),
                .product(
                    name: "Cordova",
                    package: "capacitor-swift-pm"
                ),
                "PrismDaemonFFI",
            ],
            path: "mobile/ios/Sources/PrismDaemonPlugin"
        ),
        .testTarget(
            name: "PrismDaemonPluginTests",
            dependencies: ["PrismDaemonPlugin"],
            path: "mobile/ios/Tests/PrismDaemonPluginTests"
        ),
    ]
)
