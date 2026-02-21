import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var settingsManager: SettingsManager
    @EnvironmentObject var webSocketService: WebSocketService
    @Environment(\.dismiss) var dismiss

    @State private var testResult: String?
    @State private var isTesting = false

    var body: some View {
        NavigationView {
            Form {
                Section(header: Text("Server Configuration")) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Server URL")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        TextField("https://server10.doodkin.com", text: $settingsManager.serverURL)
                            .textContentType(.URL)
                            .autocapitalization(.none)
                            .disableAutocorrection(true)
                            .keyboardType(.URL)
                    }

                    VStack(alignment: .leading, spacing: 4) {
                        Text("API Key")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        SecureField("pk_...", text: $settingsManager.apiKey)
                            .autocapitalization(.none)
                            .disableAutocorrection(true)
                            .textContentType(.password)
                    }

                    VStack(alignment: .leading, spacing: 4) {
                        Text("Device Name")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        TextField("My iPhone", text: $settingsManager.deviceName)
                    }
                }

                Section(header: Text("Connection Test")) {
                    Button(action: testConnection) {
                        HStack {
                            if isTesting {
                                ProgressView()
                                    .scaleEffect(0.8)
                                Text("Testing...")
                            } else {
                                Image(systemName: "antenna.radiowaves.left.and.right")
                                Text("Test Discovery")
                            }
                        }
                    }
                    .disabled(!settingsManager.isConfigured || isTesting)

                    if let result = testResult {
                        Text(result)
                            .font(.caption)
                            .foregroundColor(result.contains("Success") ? .green : .red)
                    }
                }

                Section(header: Text("About")) {
                    HStack {
                        Text("Platform")
                        Spacer()
                        Text("iOS (limited)")
                            .foregroundColor(.secondary)
                    }
                    HStack {
                        Text("Protocol")
                        Spacer()
                        Text("ScreenMCP WebSocket v1")
                            .foregroundColor(.secondary)
                    }
                    HStack {
                        Text("Supported Commands")
                        Spacer()
                        Text("\(SupportedCommand.allCases.count)")
                            .foregroundColor(.secondary)
                    }
                    HStack {
                        Text("Unsupported Commands")
                        Spacer()
                        Text("\(UnsupportedCommand.allCases.count)")
                            .foregroundColor(.secondary)
                    }
                }

                Section(header: Text("Help")) {
                    Text("""
                    This app connects to a ScreenMCP worker server and responds to commands. \
                    Due to iOS restrictions, only a limited set of commands are available \
                    (clipboard, camera, URL opening, shortcuts).

                    To get an API key:
                    1. Sign in at the ScreenMCP dashboard
                    2. Go to API Keys section
                    3. Create a new key (starts with pk_)
                    4. Paste it above
                    """)
                    .font(.caption)
                    .foregroundColor(.secondary)
                }
            }
            .navigationTitle("Settings")
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    private func testConnection() {
        isTesting = true
        testResult = nil

        Task {
            do {
                let wsUrl = try await settingsManager.discoverWorkerURL()
                await MainActor.run {
                    testResult = "Success! Worker: \(wsUrl)"
                    isTesting = false
                }
            } catch {
                await MainActor.run {
                    testResult = "Failed: \(error.localizedDescription)"
                    isTesting = false
                }
            }
        }
    }
}

#Preview {
    SettingsView()
        .environmentObject(SettingsManager())
        .environmentObject(WebSocketService())
}
