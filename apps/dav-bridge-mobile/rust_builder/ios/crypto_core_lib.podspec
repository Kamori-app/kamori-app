#
# To learn more about a Podspec see http://guides.cocoapods.org/syntax/podspec.html.
# Run `pod lib lint crypto_core_lib.podspec` to validate before publishing.
#
Pod::Spec.new do |s|
  s.name             = 'crypto_core_lib'
  s.version          = '0.0.1'
  s.summary          = 'Kamori encrypted sync core for Android and iOS.'
  s.description      = <<-DESC
Rust cryptography and offline sync runtime embedded in the Kamori mobile app.
                       DESC
  s.homepage         = 'https://kamori.app'
  s.license          = { :type => 'Apache-2.0', :file => '../../../../LICENSES/Apache-2.0.txt' }
  s.author           = { 'Kamori contributors' => 'security@kamori.app' }

  # This will ensure the source files in Classes/ are included in the native
  # builds of apps using this FFI plugin. Podspec does not support relative
  # paths, so Classes contains a forwarder C file that relatively imports
  # `../src/*` so that the C sources can be shared among all target platforms.
  s.source           = { :path => '.' }
  s.source_files = 'Classes/**/*'
  s.dependency 'Flutter'
  s.platform = :ios, '15.0'

  # Flutter.framework does not contain a i386 slice.
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES', 'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386' }
  s.swift_version = '5.0'

  s.script_phase = {
    :name => 'Build Rust library',
    # First argument is relative path to the `rust` folder, second is name of rust library
    :script => 'sh "$PODS_TARGET_SRCROOT/../cargokit/build_pod.sh" ../../../../packages/crypto-core-lib crypto_core_lib',
    :execution_position => :before_compile,
    :input_files => ['${BUILT_PRODUCTS_DIR}/cargokit_phony'],
    # Let XCode know that the static library referenced in -force_load below is
    # created by this build step.
    :output_files => ["${BUILT_PRODUCTS_DIR}/libcrypto_core_lib.a"],
  }
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    # Flutter.framework does not contain a i386 slice.
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386',
    'OTHER_LDFLAGS' => '-force_load ${BUILT_PRODUCTS_DIR}/libcrypto_core_lib.a',
  }
end
