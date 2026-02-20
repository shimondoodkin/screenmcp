import Foundation
import UIKit
import AVFoundation

/// Handles incoming commands from the PhoneMCP worker.
/// Only commands that iOS allows within the app sandbox are implemented.
/// All Android-only commands return an "unsupported" response with a reason.
class CommandHandler {

    private var captureSession: AVCaptureSession?
    private var photoOutput: AVCapturePhotoOutput?

    // MARK: - Main Dispatch

    func execute(_ command: Command, completion: @escaping (CommandResponse) -> Void) {
        // Check if this is a known unsupported command from the Android protocol
        if let unsupported = UnsupportedCommand(rawValue: command.cmd) {
            completion(.unsupported(command.id, reason: unsupported.reason))
            return
        }

        switch command.cmd {
        case "clipboard_get":
            handleClipboardGet(command, completion: completion)

        case "clipboard_set":
            handleClipboardSet(command, completion: completion)

        case "open_url":
            handleOpenURL(command, completion: completion)

        case "camera":
            handleCamera(command, completion: completion)

        case "device_info":
            handleDeviceInfo(command, completion: completion)

        case "run_shortcut":
            handleRunShortcut(command, completion: completion)

        default:
            completion(.error(command.id, message: "unknown command: \(command.cmd)"))
        }
    }

    // MARK: - Clipboard

    private func handleClipboardGet(_ command: Command, completion: @escaping (CommandResponse) -> Void) {
        DispatchQueue.main.async {
            let text = UIPasteboard.general.string ?? ""
            completion(.ok(command.id, result: ["text": text]))
        }
    }

    private func handleClipboardSet(_ command: Command, completion: @escaping (CommandResponse) -> Void) {
        guard let text = command.params["text"] as? String else {
            completion(.error(command.id, message: "missing 'text' parameter"))
            return
        }
        DispatchQueue.main.async {
            UIPasteboard.general.string = text
            completion(.ok(command.id))
        }
    }

    // MARK: - Open URL

    private func handleOpenURL(_ command: Command, completion: @escaping (CommandResponse) -> Void) {
        guard let urlString = command.params["url"] as? String,
              let url = URL(string: urlString) else {
            completion(.error(command.id, message: "missing or invalid 'url' parameter"))
            return
        }

        DispatchQueue.main.async {
            if UIApplication.shared.canOpenURL(url) {
                UIApplication.shared.open(url) { success in
                    if success {
                        completion(.ok(command.id))
                    } else {
                        completion(.error(command.id, message: "failed to open URL"))
                    }
                }
            } else {
                completion(.error(command.id, message: "cannot open URL: \(urlString)"))
            }
        }
    }

    // MARK: - Camera

    /// Captures a photo using AVCaptureSession.
    /// Returns base64-encoded JPEG, similar to the Android camera command.
    private func handleCamera(_ command: Command, completion: @escaping (CommandResponse) -> Void) {
        let cameraPosition: AVCaptureDevice.Position =
            (command.params["camera"] as? String) == "1" ? .front : .back
        let quality = command.params["quality"] as? Int ?? 80

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else {
                completion(.error(command.id, message: "handler deallocated"))
                return
            }

            guard let device = AVCaptureDevice.default(
                .builtInWideAngleCamera,
                for: .video,
                position: cameraPosition
            ) else {
                completion(.error(command.id, message: "camera not available"))
                return
            }

            do {
                let session = AVCaptureSession()
                session.sessionPreset = .photo

                let input = try AVCaptureDeviceInput(device: device)
                guard session.canAddInput(input) else {
                    completion(.error(command.id, message: "cannot add camera input"))
                    return
                }
                session.addInput(input)

                let output = AVCapturePhotoOutput()
                guard session.canAddOutput(output) else {
                    completion(.error(command.id, message: "cannot add photo output"))
                    return
                }
                session.addOutput(output)

                self.captureSession = session
                self.photoOutput = output

                session.startRunning()

                // Brief delay to let camera warm up
                Thread.sleep(forTimeInterval: 0.5)

                let settings = AVCapturePhotoSettings()
                let delegate = PhotoCaptureDelegate(quality: quality) { result in
                    session.stopRunning()
                    self.captureSession = nil
                    self.photoOutput = nil

                    switch result {
                    case .success(let base64):
                        completion(.ok(command.id, result: ["image": base64]))
                    case .failure(let error):
                        completion(.error(command.id, message: error.localizedDescription))
                    }
                }
                output.capturePhoto(with: settings, delegate: delegate)
                // Keep delegate alive until callback
                objc_setAssociatedObject(output, "delegate", delegate, .OBJC_ASSOCIATION_RETAIN)

            } catch {
                completion(.error(command.id, message: "camera error: \(error.localizedDescription)"))
            }
        }
    }

    // MARK: - Device Info

    private func handleDeviceInfo(_ command: Command, completion: @escaping (CommandResponse) -> Void) {
        DispatchQueue.main.async {
            let device = UIDevice.current
            device.isBatteryMonitoringEnabled = true

            let info: [String: Any] = [
                "name": device.name,
                "model": device.model,
                "systemName": device.systemName,
                "systemVersion": device.systemVersion,
                "batteryLevel": device.batteryLevel,
                "batteryState": Self.batteryStateString(device.batteryState),
                "identifierForVendor": device.identifierForVendor?.uuidString ?? "",
                "platform": "ios",
                "supportedCommands": SupportedCommand.allCases.map { $0.rawValue },
                "unsupportedCommands": UnsupportedCommand.allCases.map { $0.rawValue }
            ]
            completion(.ok(command.id, result: info))
        }
    }

    private static func batteryStateString(_ state: UIDevice.BatteryState) -> String {
        switch state {
        case .unknown: return "unknown"
        case .unplugged: return "unplugged"
        case .charging: return "charging"
        case .full: return "full"
        @unknown default: return "unknown"
        }
    }

    // MARK: - Run Shortcut

    /// Opens an iOS Shortcut by name using the shortcuts:// URL scheme.
    private func handleRunShortcut(_ command: Command, completion: @escaping (CommandResponse) -> Void) {
        guard let name = command.params["name"] as? String else {
            completion(.error(command.id, message: "missing 'name' parameter"))
            return
        }

        let encoded = name.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? name
        let urlString = "shortcuts://run-shortcut?name=\(encoded)"

        guard let url = URL(string: urlString) else {
            completion(.error(command.id, message: "invalid shortcut name"))
            return
        }

        DispatchQueue.main.async {
            UIApplication.shared.open(url) { success in
                if success {
                    completion(.ok(command.id, result: ["shortcut": name]))
                } else {
                    completion(.error(command.id, message: "failed to open shortcut: \(name)"))
                }
            }
        }
    }
}

// MARK: - Photo Capture Delegate

/// AVCapturePhotoCaptureDelegate that captures a single photo and returns base64 JPEG.
private class PhotoCaptureDelegate: NSObject, AVCapturePhotoCaptureDelegate {
    private let quality: Int
    private let completion: (Result<String, Error>) -> Void
    private var didComplete = false

    init(quality: Int, completion: @escaping (Result<String, Error>) -> Void) {
        self.quality = quality
        self.completion = completion
    }

    func photoOutput(_ output: AVCapturePhotoOutput,
                     didFinishProcessingPhoto photo: AVCapturePhoto,
                     error: Error?) {
        guard !didComplete else { return }
        didComplete = true

        if let error = error {
            completion(.failure(error))
            return
        }

        guard let imageData = photo.fileDataRepresentation() else {
            completion(.failure(NSError(domain: "PhoneMCP", code: -1,
                                       userInfo: [NSLocalizedDescriptionKey: "no image data"])))
            return
        }

        // Re-compress to JPEG at requested quality
        guard let image = UIImage(data: imageData),
              let jpegData = image.jpegData(compressionQuality: CGFloat(quality) / 100.0) else {
            completion(.failure(NSError(domain: "PhoneMCP", code: -1,
                                       userInfo: [NSLocalizedDescriptionKey: "JPEG compression failed"])))
            return
        }

        let base64 = jpegData.base64EncodedString()
        completion(.success(base64))
    }
}
