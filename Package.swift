// swift-tools-version: 5.9

import PackageDescription

let package = Package(
  name: "morphogen-av",
  platforms: [
    .macOS(.v14)
  ],
  products: [
    .executable(name: "MorphogenMacApp", targets: ["MorphogenMacApp"])
  ],
  targets: [
    .executableTarget(
      name: "MorphogenMacApp",
      path: "apps/macos/Sources/MorphogenMacApp"
    ),
    .testTarget(
      name: "MorphogenMacAppTests",
      dependencies: ["MorphogenMacApp"],
      path: "apps/macos/Tests/MorphogenMacAppTests"
    )
  ]
)
