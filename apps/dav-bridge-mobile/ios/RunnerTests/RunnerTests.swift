import Flutter
import UIKit
import XCTest

class RunnerTests: XCTestCase {
  func testProjectionPermissionsHaveUsageDescriptions() {
    // Missing keys make iOS terminate the app when the user enables projection.
    for key in ["NSContactsUsageDescription", "NSCalendarsUsageDescription",
                "NSCalendarsFullAccessUsageDescription"] {
      let description = Bundle.main.object(forInfoDictionaryKey: key) as? String
      XCTAssertFalse(description?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true, key)
    }
  }

  func testBackgroundRefreshIsPermittedByInstalledBundle() {
    let identifiers = Bundle.main.object(forInfoDictionaryKey: "BGTaskSchedulerPermittedIdentifiers") as? [String]
    XCTAssertTrue(identifiers?.contains("kamori_background_sync_task") ?? false)
    let modes = Bundle.main.object(forInfoDictionaryKey: "UIBackgroundModes") as? [String]
    XCTAssertTrue(modes?.contains("fetch") ?? false)
  }

  @MainActor
  func testProductionEngineRegistersNativeServices() async throws {
    var engine: FlutterEngine?
    for _ in 0..<200 {
      engine = UIApplication.shared.connectedScenes
        .compactMap { $0 as? UIWindowScene }
        .flatMap { $0.windows }
        .compactMap { $0.rootViewController as? FlutterViewController }
        .first?.engine
      if engine != nil { break }
      try await Task.sleep(nanoseconds: 50_000_000)
    }
    let installedEngine = try XCTUnwrap(engine, "Production scene must attach a Flutter engine")
    for plugin in ["DeviceCalendarPlusIosPlugin", "FlutterContactsPlugin",
                   "FlutterSecureStorageDarwinPlugin", "WorkmanagerPlugin"] {
      XCTAssertTrue(installedEngine.hasPlugin(plugin), "Missing production plugin: \(plugin)")
    }
  }
}
