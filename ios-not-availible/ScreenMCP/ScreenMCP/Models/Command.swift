import Foundation

/// Represents an incoming command from the ScreenMCP worker.
/// Matches the Android protocol: {"id": <num>, "cmd": "<command>", "params": {...}}
struct Command {
    let id: Int64
    let cmd: String
    let params: [String: Any]

    init?(json: [String: Any]) {
        guard let id = json["id"] as? Int64 ?? (json["id"] as? Int).map({ Int64($0) }),
              let cmd = json["cmd"] as? String else {
            return nil
        }
        self.id = id
        self.cmd = cmd
        self.params = json["params"] as? [String: Any] ?? [:]
    }
}

/// Represents a response to send back to the worker.
/// Matches: {"id": <num>, "status": "ok"|"error", "result": {...}, "error": "..."}
struct CommandResponse {
    let id: Int64
    let status: String
    let result: [String: Any]?
    let error: String?

    static func ok(_ id: Int64, result: [String: Any]? = nil) -> CommandResponse {
        CommandResponse(id: id, status: "ok", result: result, error: nil)
    }

    static func error(_ id: Int64, message: String) -> CommandResponse {
        CommandResponse(id: id, status: "error", result: nil, error: message)
    }

    static func unsupported(_ id: Int64, reason: String) -> CommandResponse {
        CommandResponse(id: id, status: "ok", result: ["unsupported": true, "reason": reason], error: nil)
    }

    func toJSON() -> [String: Any] {
        var json: [String: Any] = [
            "id": id,
            "status": status
        ]
        if let result = result {
            json["result"] = result
        }
        if let error = error {
            json["error"] = error
        }
        return json
    }
}

/// Commands that iOS can actually execute within the app sandbox.
enum SupportedCommand: String, CaseIterable {
    case clipboardGet = "clipboard_get"
    case clipboardSet = "clipboard_set"
    case openURL = "open_url"
    case camera = "camera"
    case deviceInfo = "device_info"
    case runShortcut = "run_shortcut"

    var description: String {
        switch self {
        case .clipboardGet: return "Read system clipboard"
        case .clipboardSet: return "Write to system clipboard"
        case .openURL: return "Open a URL or deep link"
        case .camera: return "Capture a photo"
        case .deviceInfo: return "Get device information"
        case .runShortcut: return "Run an iOS Shortcut by name"
        }
    }
}

/// Commands from the Android protocol that iOS cannot support.
enum UnsupportedCommand: String, CaseIterable {
    case screenshot = "screenshot"
    case click = "click"
    case longClick = "long_click"
    case drag = "drag"
    case scroll = "scroll"
    case type = "type"
    case getText = "get_text"
    case selectAll = "select_all"
    case copy = "copy"
    case paste = "paste"
    case back = "back"
    case home = "home"
    case recents = "recents"
    case uiTree = "ui_tree"

    var reason: String {
        switch self {
        case .screenshot:
            return "iOS does not allow third-party apps to capture the screen outside their own sandbox"
        case .click, .longClick, .drag, .scroll:
            return "iOS does not allow third-party apps to inject touch events"
        case .type, .getText, .selectAll, .copy, .paste:
            return "iOS does not allow third-party apps to interact with other apps' text fields"
        case .back, .home, .recents:
            return "iOS does not expose system navigation actions to third-party apps"
        case .uiTree:
            return "iOS does not allow third-party apps to read the UI hierarchy of other apps"
        }
    }
}
