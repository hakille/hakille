import SwiftUI

@main @MainActor struct HakilleApp: App {
    @StateObject private var model = MobileModel()
    @Environment(\.scenePhase) private var phase
    var body: some Scene {
        WindowGroup {
            ContentView().environmentObject(model)
                .onAppear { model.foreground(phase == .active, background: phase == .background) }
                .onChange(of: phase) { model.foreground($0 == .active, background: $0 == .background) }
        }
    }
}
