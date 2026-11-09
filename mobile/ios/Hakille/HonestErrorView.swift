import SwiftUI

/// Gabarit d'erreur d'action (states.rs §19):
/// Strictement ordonné : Fait -> Pas fait -> Actions [Réessayer] [Conserver le brouillon] -> Détails
struct ActionErrorState: Equatable {
    let done: String
    let notDone: String
    let technicalDetail: String
    var retryLabel: String = "Réessayer"
    var undoLabel: String = "Conserver le brouillon"
}

struct HonestErrorView: View {
    let state: ActionErrorState
    let onRetry: () -> Void
    let onKeepDraft: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            // 1. Fait
            HStack(alignment: .top, spacing: 6) {
                Text("Fait :")
                    .font(HakilleTypography.caption)
                    .foregroundStyle(HakilleTokens.colorSuccesDark)
                    .bold()
                Text(state.done)
                    .font(HakilleTypography.body)
                    .foregroundStyle(HakilleTokens.colorTexte1Dark)
            }
            // 2. Pas fait
            HStack(alignment: .top, spacing: 6) {
                Text("Pas fait :")
                    .font(HakilleTypography.caption)
                    .foregroundStyle(HakilleTokens.colorErreurDark)
                    .bold()
                Text(state.notDone)
                    .font(HakilleTypography.body)
                    .foregroundStyle(HakilleTokens.colorTexte1Dark)
            }
            // 3. Actions
            HStack(spacing: 12) {
                Button(state.retryLabel, action: onRetry)
                    .buttonStyle(.borderedProminent)
                    .tint(HakilleTokens.colorBraiseDark)
                Button(state.undoLabel, action: onKeepDraft)
                    .buttonStyle(.bordered)
                    .tint(HakilleTokens.colorTexte1Dark)
            }
            .padding(.top, 4)
            // 4. Détails
            Text("Détails : \(state.technicalDetail)")
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(HakilleTokens.colorTexte2Dark)
        }
        .padding(16)
        .background(HakilleTokens.colorSurface1Dark)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(HakilleTokens.colorErreurDark, lineWidth: 1)
        )
    }
}
