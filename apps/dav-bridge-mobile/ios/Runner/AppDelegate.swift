import BackgroundTasks
import Flutter
import UIKit
import workmanager_apple

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    WorkmanagerPlugin.setPluginRegistrantCallback { registry in
      GeneratedPluginRegistrant.register(with: registry)
    }
    WorkmanagerPlugin.registerPeriodicTask(
      withIdentifier: "kamori_background_sync_task",
      earliestBeginInSeconds: 4 * 60 * 60
    )
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    let channel = FlutterMethodChannel(
      name: "kamori/background-sync",
      binaryMessenger: engineBridge.applicationRegistrar.messenger()
    )
    channel.setMethodCallHandler { call, result in
      guard call.method == "isScheduled" else {
        result(FlutterMethodNotImplemented)
        return
      }
      if #available(iOS 13.0, *) {
        BGTaskScheduler.shared.getPendingTaskRequests { requests in
          let admitted = requests.contains { $0.identifier == "kamori_background_sync_task" }
          DispatchQueue.main.async { result(admitted) }
        }
      } else {
        result(false)
      }
    }
  }
}
