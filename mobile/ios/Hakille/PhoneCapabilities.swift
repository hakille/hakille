import AVFoundation
import CoreLocation
import CryptoKit
import Darwin
import Foundation
import UIKit
import UniformTypeIdentifiers
import Combine

struct PhoneLocation: Equatable, Sendable {
    let latitude: Double
    let longitude: Double
    let accuracy: Double
    let timestamp: Date
    var approximate: Bool = false
    var mock: Bool = false
}

enum PhoneCapabilityPolicy {
    static let maximumDocumentBytes = 8 * 1024 * 1024
    static let maximumPhotoBytes = 8 * 1024 * 1024
    static let maximumPreviewChars = 4000
    static let maximumCameraLongEdgePx: CGFloat = 1536
    static let maximumCameraTotalPixels: Int = 4_000_000

    static func computeCameraScale(width: CGFloat, height: CGFloat) -> CGFloat {
        let longEdge = max(width, height)
        guard longEdge > 0 else { return 0 }
        return min(1, maximumCameraLongEdgePx / longEdge)
    }

    static func accepts(_ location: PhoneLocation, now: Date) -> Bool {
        let age = now.timeIntervalSince(location.timestamp)
        return !location.mock
            && location.latitude.isFinite && location.longitude.isFinite && location.accuracy.isFinite
            && (-90...90).contains(location.latitude) && (-180...180).contains(location.longitude)
            && (0...40_100_000).contains(location.accuracy) && (-2...15).contains(age)
    }
    static func displayName(_ name: String) -> String {
        String(name.unicodeScalars.filter { !CharacterSet.controlCharacters.contains($0) }.map(String.init).joined().prefix(128))
    }
}

struct LocalDocument: Sendable {
    let name: String
    let data: Data
    let sha256: String
    let text: String?

    init(name: String, data: Data, text: String?) {
        self.name = name
        self.data = data
        self.sha256 = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        self.text = text
    }
}

/// Le fichier choisi est lu sous scope, de façon bornée, puis le scope est
/// immédiatement révoqué. Aucun bookmark persistant, aucun transfert au hub.
actor PhoneDocumentReader {
    func read(_ url: URL) throws -> LocalDocument {
        try Task.checkCancellation()
        let deadline = ContinuousClock.now.advanced(by: .seconds(10))
        guard url.isFileURL else { throw MobileStateError.unavailable }
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let fd = Darwin.open(url.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK)
        guard fd >= 0 else { throw MobileStateError.unavailable }
        defer { Darwin.close(fd) }
        var info = stat()
        guard fstat(fd, &info) == 0, info.st_mode & S_IFMT == S_IFREG,
              info.st_size >= 0, info.st_size <= off_t(PhoneCapabilityPolicy.maximumDocumentBytes) else { throw MobileStateError.invalidCheckpoint }
        var data = Data(); var buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            try Task.checkCancellation()
            guard ContinuousClock.now < deadline else { throw MobileStateError.unavailable }
            let count = Darwin.read(fd, &buffer, 4096)
            if count < 0 { if errno == EINTR { continue }; throw MobileStateError.unavailable }
            if count == 0 { break }
            guard data.count + count <= PhoneCapabilityPolicy.maximumDocumentBytes else { throw MobileStateError.invalidCheckpoint }
            data.append(contentsOf: buffer.prefix(count))
        }
        let text = String(data: data, encoding: .utf8).map { String($0.prefix(PhoneCapabilityPolicy.maximumPreviewChars)) }
        return LocalDocument(name: PhoneCapabilityPolicy.displayName(url.lastPathComponent), data: data, text: text)
    }
}

private enum LocationEvent: Sendable { case authorization(Int32), sample(PhoneLocation), failed }
private final class LocationCallbacks: NSObject, CLLocationManagerDelegate {
    let receive: @MainActor @Sendable (LocationEvent) -> Void
    init(receive: @escaping @MainActor @Sendable (LocationEvent) -> Void) { self.receive = receive }
    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        let raw = manager.authorizationStatus.rawValue; let receive = receive
        Task { @MainActor in receive(.authorization(raw)) }
    }
    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard let last = locations.last else { return }
        let isMocked = last.sourceInformation?.isSimulatedBySoftware == true
        let sample = PhoneLocation(
            latitude: last.coordinate.latitude,
            longitude: last.coordinate.longitude,
            accuracy: last.horizontalAccuracy,
            timestamp: last.timestamp,
            approximate: manager.accuracyAuthorization == .reducedAccuracy,
            mock: isMocked
        )
        let receive = receive
        Task { @MainActor in receive(.sample(sample)) }
    }
    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        let receive = receive; Task { @MainActor in receive(.failed) }
    }
}

enum ExternalDestination: String, CaseIterable, Identifiable {
    case maps, calendar, settings, documentation, clock
    var id: String { rawValue }
    var label: String {
        switch self {
        case .maps: return String(localized: "Open Maps")
        case .calendar: return String(localized: "Open Calendar")
        case .settings: return String(localized: "Open Hakille settings")
        case .documentation: return String(localized: "Open project documentation")
        case .clock: return String(localized: "Open Clock")
        }
    }
    var url: URL {
        switch self {
        case .maps: return URL(string: "maps://")!
        case .calendar: return URL(string: "calshow:")!
        case .settings: return URL(string: UIApplication.openSettingsURLString)!
        case .documentation: return URL(string: "https://github.com/hakille/hakille#readme")!
        case .clock: return URL(string: "clock://")!
        }
    }
}

enum ExternalLaunchPolicy {
    static func allowed(_ url: URL) -> Bool {
        ExternalDestination.allCases.contains { $0.url.absoluteString == url.absoluteString }
    }
}

@MainActor final class PhoneCapabilities: ObservableObject {
    @Published private(set) var message = ""
    @Published private(set) var photo: UIImage?
    @Published private(set) var document: LocalDocument?
    @Published private(set) var location: PhoneLocation?
    @Published var cameraPresented = false
    @Published var documentPickerPresented = false
    private(set) var cameraToken: UUID?
    private(set) var documentToken: UUID?
    @Published private(set) var locationPending = false
    @Published private(set) var documentPending = false
    private var generation = UUID()
    private var locationToken: UUID?
    private var locationManager: CLLocationManager?
    private var locationCallbacks: LocationCallbacks?
    private var locationTimeout: Task<Void, Never>?
    private var locationStarted = false
    private let reader = PhoneDocumentReader()
    private var documentTask: Task<Void, Never>?
    private var active = false

    func foreground(_ active: Bool, background: Bool) {
        self.active = active
        if background { cancel(); photo = nil; document = nil; location = nil }
        else if active, locationPending, let token = locationToken { startLocationIfAuthorized(token: token) }
    }
    func cancel() {
        generation = UUID(); cameraPresented = false; documentPickerPresented = false; cameraToken = nil; documentToken = nil
        documentTask?.cancel(); documentTask = nil; documentPending = false; stopLocation()
    }
    func clearPreviews() { cancel(); photo = nil; document = nil; location = nil; message = "" }
    func requestCamera() {
        guard active, UIApplication.shared.applicationState == .active else { return }
        guard UIImagePickerController.isSourceTypeAvailable(.camera) else { message = String(localized: "No camera is available on this device."); return }
        let token = generation
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized: cameraToken = UUID(); cameraPresented = true; message = ""
        case .notDetermined: Self.cameraPermission { [weak self] granted in
            guard let self, self.generation == token else { return }
            if granted && self.active && UIApplication.shared.applicationState == .active { self.cameraToken = UUID(); self.cameraPresented = true; self.message = "" }
            else if granted { self.message = String(localized: "Camera permission granted. Tap Camera again when the app is active.") }
            else { self.message = String(localized: "Camera permission denied. You can change it in iOS Settings.") }
        }
        default: message = String(localized: "Camera permission denied. You can change it in iOS Settings.")
        }
    }
    func receivePhoto(_ image: UIImage?, token: UUID) {
        guard cameraToken == token, cameraPresented, active else { return }
        cameraToken = nil
        cameraPresented = false
        guard let image else { message = String(localized: "Photo capture cancelled."); return }
        let width = image.size.width, height = image.size.height
        guard width.isFinite, height.isFinite, width > 0, height > 0,
              Int(width) * Int(height) <= PhoneCapabilityPolicy.maximumCameraTotalPixels else { message = String(localized: "The captured photo could not be previewed."); return }
        let scale = PhoneCapabilityPolicy.computeCameraScale(width: width, height: height)
        let format = UIGraphicsImageRendererFormat(); format.scale = 1; format.opaque = true
        let preview = UIGraphicsImageRenderer(size: CGSize(width: width * scale, height: height * scale), format: format).image { _ in
            image.draw(in: CGRect(x: 0, y: 0, width: width * scale, height: height * scale))
        }
        guard let data = preview.jpegData(compressionQuality: 0.8), data.count <= PhoneCapabilityPolicy.maximumPhotoBytes,
              let decoded = UIImage(data: data) else { message = String(localized: "The captured photo could not be previewed."); return }
        photo = decoded
        message = String(localized: "Photo preview only, kept in memory. Nothing was uploaded or saved to Photos.")
    }
    func requestDocument() {
        guard active, !documentPending else { return }
        documentToken = UUID(); documentPickerPresented = true; message = ""
    }
    func receiveDocument(_ url: URL?, token: UUID) {
        guard documentToken == token, documentPickerPresented, active else { return }
        documentToken = nil
        documentPickerPresented = false
        guard let url else { message = String(localized: "Document selection cancelled."); return }
        documentPending = true; let epoch = generation
        documentTask = Task {
            defer { if generation == epoch { documentTask = nil } }
            do {
                let result = try await reader.read(url)
                guard generation == epoch else { return }
                document = result; documentPending = false
                message = String(localized: "Document read locally. Nothing was uploaded. Clear previews to release it.")
            } catch {
                guard generation == epoch else { return }
                documentPending = false; message = String(localized: "Document unavailable or larger than 8 MiB. Its contents were not imported.")
            }
        }
    }
    func requestLocation() {
        guard active, !locationPending else { return }
        stopLocation(); location = nil; message = ""
        guard CLLocationManager.locationServicesEnabled() else { message = String(localized: "Location services are disabled on this device."); return }
        let token = UUID(); locationToken = token; locationPending = true
        let manager = CLLocationManager(); manager.desiredAccuracy = kCLLocationAccuracyHundredMeters
        let callbacks = LocationCallbacks { [weak self] event in self?.receiveLocation(event, token: token) }
        locationManager = manager; locationCallbacks = callbacks; manager.delegate = callbacks
        startLocationIfAuthorized(token: token)
    }
    private func startLocationIfAuthorized(token: UUID) {
        guard locationToken == token, active, !locationStarted, let manager = locationManager else { return }
        switch manager.authorizationStatus {
        case .notDetermined: manager.requestWhenInUseAuthorization()
        case .authorizedAlways, .authorizedWhenInUse:
            guard UIApplication.shared.applicationState == .active else { return }
            locationStarted = true; manager.requestLocation()
            locationTimeout = Task { [weak self] in
                do { try await Task.sleep(nanoseconds: 20_000_000_000) } catch { return }
                guard let self, self.locationToken == token else { return }
                self.stopLocation(); self.message = String(localized: "Location timed out. No position was assumed.")
            }
        default: stopLocation(); message = String(localized: "Location permission denied. You can change it in iOS Settings.")
        }
    }
    private func receiveLocation(_ event: LocationEvent, token: UUID) {
        guard locationToken == token else { return }
        switch event {
        case .authorization: startLocationIfAuthorized(token: token)
        case .sample(let sample):
            guard active, PhoneCapabilityPolicy.accepts(sample, now: Date()) else {
                stopLocation(); message = String(localized: "The location is stale or invalid. No position was assumed."); return
            }
            location = sample; stopLocation(); message = String(localized: "One location fix received. Location tracking is stopped; nothing was uploaded.")
        case .failed: stopLocation(); message = String(localized: "Location could not be determined. No position was assumed.")
        }
    }
    func cancelLocation() { stopLocation(); message = String(localized: "Location request cancelled.") }
    private func stopLocation() {
        locationToken = nil; locationTimeout?.cancel(); locationTimeout = nil
        locationManager?.stopUpdatingLocation(); locationManager?.delegate = nil
        locationManager = nil; locationCallbacks = nil; locationStarted = false; locationPending = false
    }
    func openConfirmed(_ destination: ExternalDestination) {
        guard active, ExternalLaunchPolicy.allowed(destination.url) else { return }
        let token = generation
        UIApplication.shared.open(destination.url, options: [:]) { [weak self] opened in
            Task { @MainActor in
                guard let self, self.generation == token else { return }
                self.message = opened ? String(localized: "The selected app was opened. Hakille does not control it.") : String(localized: "The selected app could not be opened.")
            }
        }
    }
    nonisolated private static func cameraPermission(_ receive: @escaping @MainActor @Sendable (Bool) -> Void) {
        AVCaptureDevice.requestAccess(for: .video) { granted in Task { @MainActor in receive(granted) } }
    }
}
