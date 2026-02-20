import Foundation
import SwiftUI

/// Persists server URL and API key using UserDefaults.
/// Mirrors the Android app's API URL field and token management.
class SettingsManager: ObservableObject {
    private static let serverURLKey = "phonemcp_server_url"
    private static let apiKeyKey = "phonemcp_api_key"
    private static let deviceNameKey = "phonemcp_device_name"
    private static let defaultServerURL = "https://server10.doodkin.com"

    @Published var serverURL: String {
        didSet { UserDefaults.standard.set(serverURL, forKey: Self.serverURLKey) }
    }

    @Published var apiKey: String {
        didSet { UserDefaults.standard.set(apiKey, forKey: Self.apiKeyKey) }
    }

    @Published var deviceName: String {
        didSet { UserDefaults.standard.set(deviceName, forKey: Self.deviceNameKey) }
    }

    var isConfigured: Bool {
        !serverURL.isEmpty && !apiKey.isEmpty
    }

    init() {
        self.serverURL = UserDefaults.standard.string(forKey: Self.serverURLKey) ?? Self.defaultServerURL
        self.apiKey = UserDefaults.standard.string(forKey: Self.apiKeyKey) ?? ""
        self.deviceName = UserDefaults.standard.string(forKey: Self.deviceNameKey) ?? UIDevice.current.name
    }

    /// Build the WebSocket discovery URL.
    /// Calls POST /api/discover with the API key to get a worker URL.
    func discoverWorkerURL() async throws -> String {
        guard isConfigured else {
            throw SettingsError.notConfigured
        }

        let url = URL(string: "\(serverURL)/api/discover")!
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.httpBody = "{}".data(using: .utf8)

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse else {
            throw SettingsError.invalidResponse
        }

        guard httpResponse.statusCode == 200 else {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw SettingsError.discoveryFailed(statusCode: httpResponse.statusCode, body: body)
        }

        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let wsUrl = json["wsUrl"] as? String, !wsUrl.isEmpty else {
            throw SettingsError.noWorkerAvailable
        }

        return wsUrl
    }

    enum SettingsError: LocalizedError {
        case notConfigured
        case invalidResponse
        case discoveryFailed(statusCode: Int, body: String)
        case noWorkerAvailable

        var errorDescription: String? {
            switch self {
            case .notConfigured:
                return "Server URL and API key must be configured"
            case .invalidResponse:
                return "Invalid response from server"
            case .discoveryFailed(let code, let body):
                return "Discovery failed (HTTP \(code)): \(body)"
            case .noWorkerAvailable:
                return "No worker available"
            }
        }
    }
}
