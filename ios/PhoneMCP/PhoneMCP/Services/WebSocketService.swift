import Foundation
import Combine

/// WebSocket client that connects to the PhoneMCP worker.
/// Implements the same protocol as the Android WebSocketClient:
///   1. Connect to worker URL
///   2. Send auth message: {"type": "auth", "token": "<key>", "role": "phone", "last_ack": 0}
///   3. Receive commands: {"id": <num>, "cmd": "<cmd>", "params": {...}}
///   4. Send responses: {"id": <num>, "status": "ok"|"error", ...}
///   5. Handle ping/pong heartbeat
class WebSocketService: ObservableObject {
    @Published var state: ConnectionState = .disconnected
    @Published var lastCommandLog: String = ""

    private var webSocketTask: URLSessionWebSocketTask?
    private var session: URLSession?
    private var token: String?
    private var workerURL: String?
    private var commandHandler: CommandHandler?

    private var reconnectAttempt = 0
    private var shouldReconnect = false
    private let maxReconnectAttempts = 5
    private let maxReconnectDelay: TimeInterval = 30.0
    private var reconnectWorkItem: DispatchWorkItem?

    /// Generation counter to prevent stale callbacks, same pattern as Android.
    private var connectionGeneration: Int64 = 0

    init() {
        self.commandHandler = CommandHandler()
    }

    // MARK: - Public API

    /// Connect via discovery API, matching Android's connectViaApi().
    func connect(settings: SettingsManager) {
        disconnect()
        self.token = settings.apiKey
        shouldReconnect = true
        reconnectAttempt = 0
        connectionGeneration += 1

        Task { @MainActor in
            state = .discovering
        }

        Task {
            do {
                let wsUrl = try await settings.discoverWorkerURL()
                self.workerURL = wsUrl
                await MainActor.run {
                    state = .connecting(url: wsUrl)
                }
                doConnect(wsUrl: wsUrl)
            } catch {
                await MainActor.run {
                    state = .failed(reason: error.localizedDescription)
                }
                scheduleReconnect(settings: settings)
            }
        }
    }

    /// Connect directly to a worker URL, matching Android's connectDirect().
    func connectDirect(wsUrl: String, token: String) {
        disconnect()
        self.token = token
        self.workerURL = wsUrl
        shouldReconnect = true
        reconnectAttempt = 0
        connectionGeneration += 1

        Task { @MainActor in
            state = .connecting(url: wsUrl)
        }

        doConnect(wsUrl: wsUrl)
    }

    func disconnect() {
        shouldReconnect = false
        connectionGeneration += 1
        reconnectWorkItem?.cancel()
        reconnectWorkItem = nil
        webSocketTask?.cancel(with: .normalClosure, reason: nil)
        webSocketTask = nil
        session?.invalidateAndCancel()
        session = nil

        Task { @MainActor in
            state = .disconnected
        }
    }

    // MARK: - Private Connection Logic

    private func doConnect(wsUrl: String) {
        let myGeneration = connectionGeneration

        guard let url = URL(string: wsUrl) else {
            Task { @MainActor in
                state = .failed(reason: "Invalid worker URL: \(wsUrl)")
            }
            return
        }

        let config = URLSessionConfiguration.default
        config.waitsForConnectivity = true
        session = URLSession(configuration: config)

        let task = session!.webSocketTask(with: url)
        webSocketTask = task
        task.resume()

        // Send auth immediately after opening
        sendAuth(generation: myGeneration)

        // Start listening for messages
        listenForMessages(generation: myGeneration)
    }

    private func sendAuth(generation: Int64) {
        guard generation == connectionGeneration,
              let token = token else { return }

        Task { @MainActor in
            state = .authenticating
        }

        let authMessage: [String: Any] = [
            "type": "auth",
            "token": token,
            "role": "phone",
            "last_ack": 0
        ]

        guard let data = try? JSONSerialization.data(withJSONObject: authMessage),
              let text = String(data: data, encoding: .utf8) else { return }

        webSocketTask?.send(.string(text)) { [weak self] error in
            if let error = error {
                self?.log("Auth send error: \(error.localizedDescription)")
            }
        }
    }

    private func listenForMessages(generation: Int64) {
        guard generation == connectionGeneration else { return }

        webSocketTask?.receive { [weak self] result in
            guard let self = self, generation == self.connectionGeneration else { return }

            switch result {
            case .success(let message):
                switch message {
                case .string(let text):
                    self.handleMessage(text, generation: generation)
                case .data(let data):
                    if let text = String(data: data, encoding: .utf8) {
                        self.handleMessage(text, generation: generation)
                    }
                @unknown default:
                    break
                }
                // Continue listening
                self.listenForMessages(generation: generation)

            case .failure(let error):
                self.log("WebSocket error: \(error.localizedDescription)")
                Task { @MainActor in
                    self.state = .failed(reason: error.localizedDescription)
                }
                self.scheduleReconnect(settings: nil)
            }
        }
    }

    private func handleMessage(_ text: String, generation: Int64) {
        guard generation == connectionGeneration else { return }

        guard let data = text.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            log("Failed to parse message")
            return
        }

        let type = json["type"] as? String ?? ""

        switch type {
        case "auth_ok":
            log("Authenticated successfully")
            reconnectAttempt = 0
            Task { @MainActor in
                state = .connected
            }

        case "auth_fail":
            let error = json["error"] as? String ?? "unknown"
            log("Auth failed: \(error)")
            shouldReconnect = false
            Task { @MainActor in
                state = .authFailed(reason: error)
            }

        case "ping":
            sendPong()

        case "error":
            let error = json["error"] as? String ?? "unknown"
            log("Server error: \(error)")

        default:
            // Check if it's a command
            if let command = Command(json: json) {
                executeCommand(command, generation: generation)
            }
        }
    }

    private func sendPong() {
        let pong: [String: Any] = ["type": "pong"]
        guard let data = try? JSONSerialization.data(withJSONObject: pong),
              let text = String(data: data, encoding: .utf8) else { return }
        webSocketTask?.send(.string(text)) { _ in }
    }

    // MARK: - Command Execution

    private func executeCommand(_ command: Command, generation: Int64) {
        log("Received command \(command.id): \(command.cmd)")

        guard let handler = commandHandler else {
            sendResponse(.error(command.id, message: "command handler not initialized"))
            return
        }

        handler.execute(command) { [weak self] response in
            guard let self = self, generation == self.connectionGeneration else { return }
            self.sendResponse(response)
            self.log("Responded to \(command.cmd) [\(command.id)]: \(response.status)")
        }
    }

    private func sendResponse(_ response: CommandResponse) {
        let json = response.toJSON()
        guard let data = try? JSONSerialization.data(withJSONObject: json),
              let text = String(data: data, encoding: .utf8) else { return }
        webSocketTask?.send(.string(text)) { [weak self] error in
            if let error = error {
                self?.log("Send error: \(error.localizedDescription)")
            }
        }
    }

    // MARK: - Reconnection (matches Android exponential backoff pattern)

    private func scheduleReconnect(settings: SettingsManager?) {
        guard shouldReconnect else { return }
        guard reconnectAttempt < maxReconnectAttempts else {
            shouldReconnect = false
            Task { @MainActor in
                state = .failed(reason: "Max reconnect attempts reached")
            }
            return
        }

        let myGeneration = connectionGeneration
        let delay = min(pow(2.0, Double(reconnectAttempt)), maxReconnectDelay)
        reconnectAttempt += 1

        Task { @MainActor in
            state = .reconnecting(attempt: reconnectAttempt, maxAttempts: maxReconnectAttempts)
        }

        let workItem = DispatchWorkItem { [weak self] in
            guard let self = self,
                  self.shouldReconnect,
                  myGeneration == self.connectionGeneration else { return }

            if let settings = settings {
                self.connect(settings: settings)
            } else if let wsUrl = self.workerURL {
                self.doConnect(wsUrl: wsUrl)
            }
        }
        reconnectWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: workItem)
    }

    // MARK: - Logging

    private func log(_ message: String) {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        let timestamp = formatter.string(from: Date())
        let entry = "[\(timestamp)] \(message)"
        print("PhoneMCP: \(entry)")
        Task { @MainActor in
            lastCommandLog = entry
        }
    }
}
