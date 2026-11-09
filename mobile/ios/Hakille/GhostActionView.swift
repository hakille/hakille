import SwiftUI

/// Ghost Action Projection Overlay (states & protocol):
/// Translucent anticipatory action proposal at focal point,
/// dismissible on drag/look-away, actionable on tap.
struct GhostActionView: View {
    let ghost: JSONValue
    let onAccept: (String) -> Void
    let onDismiss: (String) -> Void

    @State private var offset: CGFloat = 0

    var body: some View {
        let id = ghost["id"].text
        guard !id.isEmpty else { return AnyView(EmptyView()) }
        let label = ghost["label"].text
        let confidence = Int(((ghost["confidence"].doubleValue ?? 0.9) * 100).clamped(to: 0...100))

        return AnyView(
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("Suggestion anticipée (\(confidence)%)")
                        .font(HakilleTypography.caption)
                        .foregroundStyle(HakilleTokens.colorBraiseDark)
                        .bold()
                    Spacer()
                    Button("Masquer") { onDismiss(id) }
                        .font(HakilleTypography.caption)
                        .foregroundStyle(HakilleTokens.colorTexte2Dark)
                }
                Text(label)
                    .font(HakilleTypography.title)
                    .foregroundStyle(HakilleTokens.colorTexte1Dark)
                HStack {
                    Text("Glisser pour masquer")
                        .font(HakilleTypography.caption)
                        .foregroundStyle(HakilleTokens.colorTexte2Dark)
                    Spacer()
                    Button("Valider") { onAccept(id) }
                        .buttonStyle(.borderedProminent)
                        .tint(HakilleTokens.colorBraiseDark)
                }
            }
            .padding(16)
            .background(HakilleTokens.colorSurface1Dark.opacity(0.92))
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(
                RoundedRectangle(cornerRadius: 12)
                    .stroke(HakilleTokens.colorBraiseDark.opacity(0.6), lineWidth: 1)
            )
            .offset(x: offset)
            .gesture(
                DragGesture()
                    .onChanged { gesture in offset = gesture.translation.width }
                    .onEnded { gesture in
                        if abs(gesture.translation.width) > 150 {
                            onDismiss(id)
                        } else {
                            withAnimation { offset = 0 }
                        }
                    }
            )
        )
    }
}

/// Ergonomic Fatigue Alert View:
/// Soothing ergonomic well-being banner for breaks and posture.
struct FatigueAlertView: View {
    let recommendation: String
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Bien-être & Ergonomie")
                    .font(HakilleTypography.caption)
                    .foregroundStyle(HakilleTokens.colorLueurDark)
                    .bold()
                Text(recommendation)
                    .font(HakilleTypography.body)
                    .foregroundStyle(HakilleTokens.colorTexte1Dark)
            }
            Spacer()
            Button("Compris", action: onDismiss)
                .font(HakilleTypography.caption)
                .tint(HakilleTokens.colorTexte1Dark)
        }
        .padding(14)
        .background(HakilleTokens.colorSurface1Dark)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(HakilleTokens.colorLueurDark.opacity(0.6), lineWidth: 1)
        )
    }
}

private extension Comparable {
    func clamped(to limits: ClosedRange<Self>) -> Self {
        min(max(self, limits.lowerBound), limits.upperBound)
    }
}
