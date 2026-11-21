import SwiftUI

@main @MainActor struct HakilleApp: App {
    @StateObject private var model = MobileModel()
    @Environment(\.scenePhase) private var phase
    var body: some Scene {
        WindowGroup {
            ZStack {
                ContentView().environmentObject(model)
                // Rideau de confidentialité lorsque l'app passe en arrière-plan ou
                // dans le sélecteur d'applications (pas de fuite visuelle).
                if phase != .active {
                    Color.black.ignoresSafeArea()
                        .accessibilityHidden(true)
                }
            }
            .onAppear { model.foreground(phase == .active, background: phase == .background) }
            .onChange(of: phase) { model.foreground($0 == .active, background: $0 == .background) }
        }
    }
}
