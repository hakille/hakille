import SwiftUI
import Combine

/// 3-Tier Honest Wait Counter (states.rs §19):
/// - t < 400ms: LoadingFast (invisible transition, no indicator)
/// - 400ms <= t < 2000ms: LoadingSkeleton (calm static skeleton, no fake radar)
/// - t >= 2000ms: LoadingHonest (exact elapsed seconds, local Qwen CPU explanation, active Annuler button)
struct HonestWaitView: View {
    let startTime: Date?
    let onCancel: () -> Void

    @State private var elapsedMs: Double = 0
    @State private var elapsedSeconds: Int = 0
    private let timer = Timer.publish(every: 0.1, on: .main, in: .common).autoconnect()

    var body: some View {
        Group {
            if elapsedMs < 400 {
                // LoadingFast: invisible transition, zero visual jitter
                EmptyView()
            } else if elapsedMs < 2000 {
                // LoadingSkeleton: calm static skeleton, no fake radar
                VStack(alignment: .leading, spacing: 8) {
                    RoundedRectangle(cornerRadius: 4)
                        .fill(HakilleTokens.colorSurface2Dark)
                        .frame(width: 180, height: 14)
                    RoundedRectangle(cornerRadius: 4)
                        .fill(HakilleTokens.colorSurface2Dark)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .frame(height: 12)
                }
                .padding(16)
                .background(HakilleTokens.colorSurface1Dark)
                .clipShape(RoundedRectangle(cornerRadius: 12))
                .accessibilityLabel("Chargement en cours")
            } else {
                // LoadingHonest: exact elapsed seconds + local Qwen CPU explanation + active cancel button
                VStack(alignment: .leading, spacing: 12) {
                    HStack(spacing: 12) {
                        ProgressView()
                            .tint(HakilleTokens.colorBraiseDark)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Génération locale Qwen CPU en cours… (\(elapsedSeconds)s)")
                                .font(HakilleTypography.body)
                                .foregroundStyle(HakilleTokens.colorTexte1Dark)
                            Text("Inférence sur le processeur du hub. Aucun traitement cloud.")
                                .font(HakilleTypography.caption)
                                .foregroundStyle(HakilleTokens.colorTexte2Dark)
                        }
                    }
                    HStack {
                        Spacer()
                        Button("Annuler", role: .cancel, action: onCancel)
                            .buttonStyle(.bordered)
                            .tint(HakilleTokens.colorBraiseDark)
                    }
                }
                .padding(16)
                .background(HakilleTokens.colorSurface1Dark)
                .clipShape(RoundedRectangle(cornerRadius: 12))
            }
        }
        .onReceive(timer) { now in
            guard let start = startTime else {
                elapsedMs = 0
                elapsedSeconds = 0
                return
            }
            let diff = max(0, now.timeIntervalSince(start))
            elapsedMs = diff * 1000.0
            elapsedSeconds = Int(diff)
        }
    }
}
