import SwiftUI

public enum GuestPassHelper {
    public static let guestRingPx: Int = 1
    public static let defaultDurationSecs: UInt32 = 3600

    public static let forbiddenGuestActions: Set<String> = [
        "memory.forget",
        "wallet.spend",
        "system.reboot",
        "autonomy.promote",
        "audit.clear"
    ]

    public static func computeRingFraction(elapsedSecs: UInt32, durationSecs: UInt32) -> Float {
        guard durationSecs > 0 else { return 0.0 }
        let ratio = 1.0 - Float(elapsedSecs) / Float(durationSecs)
        return min(1.0, max(0.0, ratio))
    }

    public static func isExpired(elapsedSecs: UInt32, durationSecs: UInt32) -> Bool {
        durationSecs == 0 || elapsedSecs >= durationSecs
    }

    public static func isAuthorizedForGuest(power: String, expired: Bool, requestedAction: String) -> Bool {
        guard !expired else { return false }
        guard !forbiddenGuestActions.contains(requestedAction) else { return false }
        return power == requestedAction
    }
}

public struct GuestPassView: View {
    @Environment(\.colorScheme) private var scheme
    public let guestPass: JSONValue
    public let busy: Bool
    public let onStartGuestPass: (String, String, UInt32) -> Void
    public let onRevokeGuestPass: () -> Void

    @State private var guestName = "Invité"
    @State private var guestPower = "wifi.connect"
    @State private var durationSecs: UInt32 = GuestPassHelper.defaultDurationSecs

    public init(
        guestPass: JSONValue,
        busy: Bool,
        onStartGuestPass: @escaping (String, String, UInt32) -> Void,
        onRevokeGuestPass: @escaping () -> Void
    ) {
        self.guestPass = guestPass
        self.busy = busy
        self.onStartGuestPass = onStartGuestPass
        self.onRevokeGuestPass = onRevokeGuestPass
    }

    private var textColor: Color {
        scheme == .dark ? HakilleTokens.colorTexte1Dark : HakilleTokens.colorTexte1Light
    }

    private var secondaryText: Color {
        scheme == .dark ? HakilleTokens.colorTexte2Dark : HakilleTokens.colorTexte2Light
    }

    private var outlineColor: Color {
        scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight
    }

    private var lueurColor: Color {
        scheme == .dark ? HakilleTokens.colorLueurDark : HakilleTokens.colorLueurLight
    }

    private var isActive: Bool {
        !guestPass.isNull && guestPass["expired"] != .bool(true)
    }

    public var body: some View {
        Section("Pass Invité éphémère") {
            Text("Délégation temporaire bornée (anneau 1 px). Toutes les échelles d'autonomie sont abaissées au cran 0 (Observer).")
                .font(HakilleTypography.caption)
                .foregroundStyle(secondaryText)

            if isActive {
                let dur = UInt32(guestPass["duration_secs"].id ?? UInt64(GuestPassHelper.defaultDurationSecs))
                let elapsed = UInt32(guestPass["elapsed_secs"].id ?? 0)
                let fraction = GuestPassHelper.computeRingFraction(elapsedSecs: elapsed, durationSecs: dur)
                let remainingMin = max(0, Int(dur) - Int(elapsed)) / 60

                HStack(spacing: 16) {
                    ZStack {
                        // Strict 1pt hairline countdown ring (GUEST_RING_PX = 1)
                        Circle()
                            .stroke(outlineColor, lineWidth: CGFloat(GuestPassHelper.guestRingPx))
                        Circle()
                            .trim(from: 0, to: CGFloat(fraction))
                            .stroke(
                                lueurColor,
                                style: StrokeStyle(
                                    lineWidth: CGFloat(GuestPassHelper.guestRingPx),
                                    lineCap: .round
                                )
                            )
                            .rotationEffect(.degrees(-90))
                        Text("\(remainingMin)m")
                            .font(HakilleTypography.caption)
                            .foregroundStyle(textColor)
                    }
                    .frame(width: 56, height: 56)

                    VStack(alignment: .leading, spacing: 4) {
                        Text(guestPass["name"].text)
                            .font(HakilleTypography.heading)
                            .foregroundStyle(textColor)
                        Text("Pouvoir : \(guestPass["power"].text)")
                            .font(HakilleTypography.caption)
                            .foregroundStyle(secondaryText)
                        Text("Autonomie : Cran 0 (Observer)")
                            .font(HakilleTypography.caption)
                            .foregroundStyle(secondaryText)
                    }
                    Spacer()
                    Button("Révoquer", role: .destructive) {
                        onRevokeGuestPass()
                    }
                    .disabled(busy)
                    .frame(minHeight: 44)
                }
            } else {
                TextField("Nom de l'invité", text: $guestName)
                    .disabled(busy)
                Picker("Pouvoir délégué", selection: $guestPower) {
                    Text("wifi.connect").tag("wifi.connect")
                    Text("salon.lumieres").tag("salon.lumieres")
                    Text("musique").tag("musique")
                    Text("tv.control").tag("tv.control")
                }
                .disabled(busy)
                Picker("Durée", selection: $durationSecs) {
                    Text("30 min").tag(UInt32(1800))
                    Text("1 h").tag(UInt32(3600))
                    Text("2 h").tag(UInt32(7200))
                }
                .pickerStyle(.segmented)
                .disabled(busy)
                Button("Activer le pass invité") {
                    onStartGuestPass(guestName, guestPower, durationSecs)
                }
                .disabled(busy || guestName.trimmingCharacters(in: .whitespaces).isEmpty)
                .frame(minHeight: 44)
            }
        }
    }
}
