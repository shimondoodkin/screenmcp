import SwiftUI

@main
struct ScreenMCPApp: App {
    @StateObject private var webSocketService = WebSocketService()
    @StateObject private var settingsManager = SettingsManager()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(webSocketService)
                .environmentObject(settingsManager)
        }
    }
}
