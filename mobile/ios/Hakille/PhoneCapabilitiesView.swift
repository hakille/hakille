import SwiftUI
import UIKit
import UniformTypeIdentifiers

@MainActor private struct CameraCapture: UIViewControllerRepresentable {
    let receive: (UIImage?) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(receive: receive) }
    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera; picker.mediaTypes = [UTType.image.identifier]
        picker.allowsEditing = false; picker.delegate = context.coordinator
        return picker
    }
    func updateUIViewController(_ uiViewController: UIImagePickerController, context: Context) { }
    @MainActor final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let receive: (UIImage?) -> Void
        init(receive: @escaping (UIImage?) -> Void) { self.receive = receive }
        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) { receive(nil) }
        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            receive(info[.originalImage] as? UIImage)
        }
    }
}

@MainActor private struct LocalDocumentPicker: UIViewControllerRepresentable {
    let receive: (URL?) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(receive: receive) }
    func makeUIViewController(context: Context) -> UIDocumentPickerViewController {
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.plainText, .pdf, .image], asCopy: false)
        picker.allowsMultipleSelection = false; picker.delegate = context.coordinator
        return picker
    }
    func updateUIViewController(_ uiViewController: UIDocumentPickerViewController, context: Context) { }
    @MainActor final class Coordinator: NSObject, UIDocumentPickerDelegate {
        let receive: (URL?) -> Void
        init(receive: @escaping (URL?) -> Void) { self.receive = receive }
        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) { receive(nil) }
        func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) { receive(urls.count == 1 ? urls.first : nil) }
    }
}

@MainActor struct PhoneCapabilitiesView: View {
    @ObservedObject var phone: PhoneCapabilities
    @Environment(\.colorScheme) private var scheme
    private var surface: Color { Color(hakille: scheme == .dark ? HakilleTokens.colorSurface1Dark : HakilleTokens.colorSurface1Light) }
    private var secondary: Color { Color(hakille: scheme == .dark ? HakilleTokens.colorTexte2Dark : HakilleTokens.colorTexte2Light) }
    @State private var external: ExternalDestination?
    var body: some View {
        Section("On this phone") {
            Text("Local previews. Nothing is sent to your hub.").font(HakilleTypography.caption).foregroundStyle(secondary)
            Button { phone.requestCamera() } label: { Label("Camera preview", systemImage: "camera").frame(minHeight: 44) }
            Button { phone.requestDocument() } label: { Label("Choose a document", systemImage: "doc").frame(minHeight: 44) }.disabled(phone.documentPending)
            Text("Text, PDF or image · up to 8 MiB").font(HakilleTypography.caption).foregroundStyle(secondary)
            if phone.documentPending { ProgressView("Reading the selected document…") }
            if phone.locationPending {
                ProgressView("Requesting one location fix…")
                Button("Cancel location request") { phone.cancelLocation() }
            } else {
                Button { phone.requestLocation() } label: { Label("Get my location once", systemImage: "location").frame(minHeight: 44) }
            }
            if !phone.message.isEmpty { Text(phone.message).textSelection(.enabled) }
            if let photo = phone.photo {
                Image(uiImage: photo).resizable().scaledToFit().frame(maxHeight: 280)
                    .clipShape(RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusCard)))
                    .accessibilityLabel("Photo captured on this device. Preview only.")
            }
            if let document = phone.document {
                Label(document.name, systemImage: "doc.text").font(HakilleTypography.heading)
                Text(ByteCountFormatter.string(fromByteCount: Int64(document.data.count), countStyle: .file))
                if let text = document.text { Text(text).lineLimit(20).textSelection(.enabled) }
                else { Text("This format has no text preview here. The file was read locally.").font(HakilleTypography.caption) }
            }
            if let location = phone.location {
                if location.approximate || location.accuracy > 1000 { Text("Approximate location").font(HakilleTypography.caption) }
                Text(String(format: "%.6f, %.6f", locale: Locale(identifier: "en_US_POSIX"), location.latitude, location.longitude)).textSelection(.enabled)
                Text("Measured accuracy: ±\(Int(location.accuracy.rounded())) m")
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    Text("Captured \(Int(max(0, context.date.timeIntervalSince(location.timestamp)))) seconds ago — not live").font(HakilleTypography.caption)
                }
            }
            DisclosureGroup("About these tools") {
                Text("Photo and document previews stay in memory and are cleared in the background. Your chosen Files provider may download a document. Location uses one foreground request, never background tracking. No model response can activate these tools.").font(HakilleTypography.caption)
            }
            if phone.photo != nil || phone.document != nil || phone.location != nil {
                Button("Clear local previews", role: .destructive) { phone.clearPreviews() }
            }
        }.listRowBackground(surface)
        Section("Open another app") {
            Text("Choose a destination, then confirm. Hakille opens it without controlling the other app.").font(HakilleTypography.caption).foregroundStyle(secondary)
            ForEach(ExternalDestination.allCases) { destination in
                Button(destination.label) { external = destination }.frame(minHeight: 44)
            }
        }
        .listRowBackground(surface)
        .confirmationDialog("Open the selected destination?", isPresented: Binding(get: { external != nil }, set: { if !$0 { external = nil } }), titleVisibility: .visible) {
            if let destination = external {
                Button(destination.label) { phone.openConfirmed(destination); external = nil }
            }
            Button("Cancel", role: .cancel) { external = nil }
        } message: {
            Text(external?.url.absoluteString ?? "")
        }
        .sheet(isPresented: $phone.cameraPresented) {
            if let token = phone.cameraToken { CameraCapture { phone.receivePhoto($0, token: token) }.ignoresSafeArea() }
        }
        .sheet(isPresented: $phone.documentPickerPresented) {
            if let token = phone.documentToken { LocalDocumentPicker { phone.receiveDocument($0, token: token) } }
        }
    }
}
