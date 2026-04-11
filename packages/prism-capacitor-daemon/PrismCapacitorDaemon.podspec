require 'json'

package = JSON.parse(File.read(File.join(__dir__, 'package.json')))

# Podspec for the Prism daemon Capacitor plugin (iOS side).
#
# Capacitor iOS drives everything through CocoaPods, so even though we
# also ship a Package.swift for non-Capacitor consumers, the podspec is
# the file the Studio iOS scaffold resolves. It wraps three things:
#
#   1. The Swift plugin source in ios/Sources/PrismDaemonPlugin/.
#   2. The C header + module map in ios/include/ (so the Swift code
#      can `import PrismDaemonFFI` and call the C ABI).
#   3. The prebuilt xcframework in ios/Frameworks/, which contains the
#      device and simulator slices of `libprism_daemon.a` built by
#      `packages/prism-daemon/scripts/build-ios.sh`.
#
# The xcframework is expected to exist before `pod install` runs. The
# `prepare_command` hook runs the build script automatically on the
# developer machine the first time the pod is resolved so nobody has
# to remember to pre-build it. CI should invoke the script explicitly
# before `cap sync ios` so the pod resolution stays fast.

Pod::Spec.new do |s|
  s.name = 'PrismCapacitorDaemon'
  s.version = package['version']
  s.summary = package['description']
  s.license = package['license']
  s.homepage = 'https://github.com/prism/prism'
  s.author = package['author']
  s.source = { :git => 'https://github.com/prism/prism.git', :tag => s.version.to_s }
  s.source_files = 'ios/Sources/**/*.{swift,h,m}'
  s.public_header_files = 'ios/include/PrismDaemon.h'
  # Matches Rust's default `aarch64-apple-ios` deployment target (iOS
  # 17+). Lowering this forces Cargo's rustc invocation to target an
  # older platform via `IPHONEOS_DEPLOYMENT_TARGET`, which isn't worth
  # the engineering cost for a build tool that runs on modern devices.
  # The linker would otherwise emit hundreds of "built for newer iOS
  # version" warnings for every Luau object file.
  s.ios.deployment_target = '17.0'
  s.dependency 'Capacitor'
  s.swift_version = '5.9'

  # The prebuilt xcframework: built by scripts/build-ios.sh and checked
  # into ios/Frameworks/ by that script's final `-output` arg. Cocoapods
  # vendors it into the resulting Xcode project, so Swift's
  # `import PrismDaemonFFI` resolves against the right arch at build
  # time (device vs simulator).
  s.vendored_frameworks = 'ios/Frameworks/PrismDaemon.xcframework'

  # Auto-rebuild the xcframework on first `pod install` if it's missing.
  # Runs from the pod's root directory (packages/prism-capacitor-daemon).
  s.prepare_command = <<-CMD
    if [ ! -d "ios/Frameworks/PrismDaemon.xcframework" ]; then
      ../prism-daemon/scripts/build-ios.sh
    fi
  CMD
end
