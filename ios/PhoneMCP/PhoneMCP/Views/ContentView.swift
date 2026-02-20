import SwiftUI

struct ContentView: View {
    @EnvironmentObject var webSocketService: WebSocketService
    @EnvironmentObject var settingsManager: SettingsManager
    @State private var showSettings = false

    var body: some View {
        NavigationView {
            VStack(spacing: 0) {
                // Connection status banner
                connectionBanner

                ScrollView {
                    VStack(spacing: 20) {
                        // Connection controls
                        connectionSection

                        // Last command log
                        if !webSocketService.lastCommandLog.isEmpty {
                            logSection
                        }

                        // Supported commands info
                        supportedCommandsSection

                        // Unsupported commands info
                        unsupportedCommandsSection

                        // iOS limitations notice
                        limitationsSection
                    }
                    .padding()
                }
            }
            .navigationTitle("PhoneMCP")
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(action: { showSettings = true }) {
                        Image(systemName: "gear")
                    }
                }
            }
            .sheet(isPresented: $showSettings) {
                SettingsView()
            }
        }
    }

    // MARK: - Connection Banner

    private var connectionBanner: some View {
        HStack {
            Circle()
                .fill(statusColor)
                .frame(width: 12, height: 12)
            Text(webSocketService.state.displayText)
                .font(.subheadline)
                .fontWeight(.medium)
            Spacer()
        }
        .padding(.horizontal)
        .padding(.vertical, 10)
        .background(statusBackgroundColor)
    }

    private var statusColor: Color {
        switch webSocketService.state.color {
        case "green": return .green
        case "red": return .red
        default: return .yellow
        }
    }

    private var statusBackgroundColor: Color {
        switch webSocketService.state.color {
        case "green": return Color.green.opacity(0.1)
        case "red": return Color.red.opacity(0.1)
        default: return Color.yellow.opacity(0.1)
        }
    }

    // MARK: - Connection Section

    private var connectionSection: some View {
        VStack(spacing: 12) {
            if !settingsManager.isConfigured {
                HStack {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundColor(.orange)
                    Text("Configure server URL and API key in Settings")
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                }
                .padding()
                .background(Color.orange.opacity(0.1))
                .cornerRadius(8)
            }

            HStack(spacing: 12) {
                Button(action: {
                    webSocketService.connect(settings: settingsManager)
                }) {
                    HStack {
                        Image(systemName: "bolt.fill")
                        Text("Connect")
                    }
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(settingsManager.isConfigured ? Color.blue : Color.gray)
                    .foregroundColor(.white)
                    .cornerRadius(10)
                }
                .disabled(!settingsManager.isConfigured || webSocketService.state.isConnected)

                Button(action: {
                    webSocketService.disconnect()
                }) {
                    HStack {
                        Image(systemName: "xmark.circle.fill")
                        Text("Disconnect")
                    }
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(Color.red.opacity(0.8))
                    .foregroundColor(.white)
                    .cornerRadius(10)
                }
                .disabled(!webSocketService.state.isConnected)
            }
        }
    }

    // MARK: - Log Section

    private var logSection: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Last Activity")
                .font(.caption)
                .foregroundColor(.secondary)
            Text(webSocketService.lastCommandLog)
                .font(.system(.caption, design: .monospaced))
                .padding(8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(.systemGray6))
                .cornerRadius(6)
        }
    }

    // MARK: - Supported Commands

    private var supportedCommandsSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Supported Commands", systemImage: "checkmark.circle.fill")
                .font(.headline)
                .foregroundColor(.green)

            ForEach(SupportedCommand.allCases, id: \.rawValue) { cmd in
                HStack(spacing: 8) {
                    Image(systemName: "checkmark")
                        .foregroundColor(.green)
                        .frame(width: 20)
                    VStack(alignment: .leading) {
                        Text(cmd.rawValue)
                            .font(.system(.body, design: .monospaced))
                        Text(cmd.description)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
            }
        }
        .padding()
        .background(Color.green.opacity(0.05))
        .cornerRadius(12)
    }

    // MARK: - Unsupported Commands

    private var unsupportedCommandsSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Unsupported on iOS", systemImage: "xmark.circle.fill")
                .font(.headline)
                .foregroundColor(.red)

            Text("These commands from the Android protocol cannot be implemented on stock iOS due to sandbox restrictions.")
                .font(.caption)
                .foregroundColor(.secondary)
                .padding(.bottom, 4)

            ForEach(UnsupportedCommand.allCases, id: \.rawValue) { cmd in
                HStack(spacing: 8) {
                    Image(systemName: "xmark")
                        .foregroundColor(.red)
                        .frame(width: 20)
                    VStack(alignment: .leading) {
                        Text(cmd.rawValue)
                            .font(.system(.body, design: .monospaced))
                        Text(cmd.reason)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
            }
        }
        .padding()
        .background(Color.red.opacity(0.05))
        .cornerRadius(12)
    }

    // MARK: - Limitations

    private var limitationsSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("iOS Limitations", systemImage: "info.circle")
                .font(.headline)
                .foregroundColor(.blue)

            Text("""
            iOS does not allow third-party apps to perform UI automation like Android's \
            AccessibilityService. This companion app can only execute commands within its \
            own sandbox.

            For full automation on iOS, consider:
            - WebDriverAgent (requires Mac + Xcode)
            - Jailbroken device with custom tweaks
            - iOS Shortcuts for limited automation workflows
            """)
            .font(.caption)
            .foregroundColor(.secondary)
        }
        .padding()
        .background(Color.blue.opacity(0.05))
        .cornerRadius(12)
    }
}

#Preview {
    ContentView()
        .environmentObject(WebSocketService())
        .environmentObject(SettingsManager())
}
