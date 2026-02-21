import Foundation

/// Connection states matching the Android app's status flow.
enum ConnectionState: Equatable {
    case disconnected
    case discovering
    case connecting(url: String)
    case authenticating
    case connected
    case authFailed(reason: String)
    case reconnecting(attempt: Int, maxAttempts: Int)
    case failed(reason: String)

    var displayText: String {
        switch self {
        case .disconnected:
            return "Disconnected"
        case .discovering:
            return "Discovering worker..."
        case .connecting(let url):
            return "Connecting to \(url)..."
        case .authenticating:
            return "Authenticating..."
        case .connected:
            return "Connected"
        case .authFailed(let reason):
            return "Auth failed: \(reason)"
        case .reconnecting(let attempt, let maxAttempts):
            return "Reconnecting (\(attempt)/\(maxAttempts))..."
        case .failed(let reason):
            return "Failed: \(reason)"
        }
    }

    var isConnected: Bool {
        if case .connected = self { return true }
        return false
    }

    var color: String {
        switch self {
        case .connected: return "green"
        case .disconnected, .authFailed, .failed: return "red"
        default: return "yellow"
        }
    }
}
