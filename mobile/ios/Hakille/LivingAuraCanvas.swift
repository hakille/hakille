import SwiftUI

public enum PresenceState: String, Sendable {
    case idle
    case listening
    case acting
    case speaking
    case decisionRequired
    case urgent

    public var localizedDescription: String {
        switch self {
        case .idle: return "au repos"
        case .listening: return "à l'écoute"
        case .acting: return "en action"
        case .speaking: return "parle"
        case .decisionRequired: return "décision requise"
        case .urgent: return "attention urgente"
        }
    }
}

public struct AuraRingUpdateDto: Sendable, Equatable {
    public let tau: Float
    public let stateName: String
    public let primaryHex: String
    public let glowHex: String
    public let pulsePeriodMs: UInt32
    public let scaleFactor: Float
    public let timestampMs: UInt64

    public init(
        tau: Float,
        stateName: String,
        primaryHex: String,
        glowHex: String,
        pulsePeriodMs: UInt32 = 3000,
        scaleFactor: Float = 1.0,
        timestampMs: UInt64 = 0
    ) {
        self.tau = tau
        self.stateName = stateName
        self.primaryHex = primaryHex
        self.glowHex = glowHex
        self.pulsePeriodMs = pulsePeriodMs
        self.scaleFactor = scaleFactor
        self.timestampMs = timestampMs
    }

    public var isHighTrust: Bool { tau >= 0.90 || stateName == "EmeraldHighTrust" }
    public var isAmbient: Bool { (tau >= 0.40 && tau < 0.90) || stateName == "AmberAmbient" }
    public var isStepUp: Bool { stateName == "VioletStepUp" }
    public var isLocked: Bool { stateName == "RedLocked" }
}

public func wireCountForSize(size: CGFloat) -> Int {
    size < 24 ? HakilleTokens.presenceWireCountCompact : HakilleTokens.presenceWireCountFull
}

public struct LivingPresenceAuraView: View {
    @Environment(\.colorScheme) private var scheme
    public let state: PresenceState
    public let auraUpdate: AuraRingUpdateDto?
    public let size: CGFloat

    @State private var pulsePhase: CGFloat = 0.0

    public init(
        state: PresenceState = .idle,
        auraUpdate: AuraRingUpdateDto? = nil,
        size: CGFloat = 176
    ) {
        self.state = state
        self.auraUpdate = auraUpdate
        self.size = size
    }

    private var wires: Int { wireCountForSize(size: size) }
    private var isCompact: Bool { size < 24 }

    private var ringColor: Color {
        if let update = auraUpdate {
            return Color(hex: update.primaryHex)
        }
        switch state {
        case .idle, .listening, .speaking:
            return Color(hex: scheme == .dark ? HakilleTokens.colorLueurDark : HakilleTokens.colorLueurLight)
        case .acting:
            return Color(hex: scheme == .dark ? HakilleTokens.colorSuccesDark : HakilleTokens.colorSuccesLight)
        case .decisionRequired:
            // Braise is strictly authorized for decision-ring attention indicator
            return Color(hex: scheme == .dark ? HakilleTokens.colorBraiseDark : HakilleTokens.colorBraiseLight)
        case .urgent:
            return Color(hex: scheme == .dark ? HakilleTokens.colorErreurDark : HakilleTokens.colorErreurLight)
        }
    }

    private var haloColor: Color {
        if let update = auraUpdate {
            return Color(hex: update.glowHex)
        }
        switch state {
        case .idle:
            return ringColor.opacity(0.08)
        case .listening:
            return ringColor.opacity(0.28)
        case .acting:
            return ringColor.opacity(0.25)
        case .speaking:
            return ringColor.opacity(0.35)
        case .decisionRequired:
            return Color(hex: scheme == .dark ? HakilleTokens.colorBraiseHaloDark : HakilleTokens.colorBraiseHaloLight)
        case .urgent:
            return ringColor.opacity(0.35)
        }
    }

    public var body: some View {
        ZStack {
            Canvas { context, canvasSize in
                let center = CGPoint(x: canvasSize.width / 2, y: canvasSize.height / 2)
                let baseRadius = (min(canvasSize.width, canvasSize.height) / 2) - 4
                let breath = sin(pulsePhase * 2 * .pi) * 0.03
                let activeRadius = baseRadius * (CGFloat(auraUpdate?.scaleFactor ?? 1.0) + breath)

                // Halo bloom
                let haloRect = CGRect(x: center.x - activeRadius - 2, y: center.y - activeRadius - 2, width: (activeRadius + 2) * 2, height: (activeRadius + 2) * 2)
                context.stroke(Path(ellipseIn: haloRect), with: .color(haloColor), lineWidth: 4)

                // Main presence ring
                let ringStrokeWidth: CGFloat = (state == .listening || state == .speaking || state == .decisionRequired) ? 2 : 1
                let ringRect = CGRect(x: center.x - activeRadius, y: center.y - activeRadius, width: activeRadius * 2, height: activeRadius * 2)
                context.stroke(Path(ellipseIn: ringRect), with: .color(ringColor), lineWidth: ringStrokeWidth)

                // Inner hairline ring
                if !isCompact {
                    let innerRadius = activeRadius - 10
                    let innerRect = CGRect(x: center.x - innerRadius, y: center.y - innerRadius, width: innerRadius * 2, height: innerRadius * 2)
                    let lineColor = Color(hex: scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight).opacity(0.6)
                    context.stroke(Path(ellipseIn: innerRect), with: .color(lineColor), lineWidth: 1)
                }

                // 21 Kora Wires (7 if compact)
                if !isCompact && size >= 48 {
                    let wireRadius = activeRadius - 16
                    let maxHeight = CGFloat(HakilleTokens.presenceWireMaxHeightPx)
                    let wireWidth = CGFloat(HakilleTokens.presenceWireWidthPx)
                    let angleStep = (2 * .pi) / CGFloat(wires)

                    for i in 0..<wires {
                        let angle = (CGFloat(i) * angleStep) - (.pi / 2)
                        let wirePhase = (pulsePhase + (CGFloat(i) / CGFloat(wires))).truncatingRemainder(dividingBy: 1.0)
                        let wireMod: CGFloat = {
                            switch state {
                            case .speaking:
                                return max(0, sin(wirePhase * 2 * .pi)) * 0.6 + 0.4
                            case .listening:
                                return 0.4 + (0.2 * sin(wirePhase * 2 * .pi))
                            case .acting:
                                return 0.3 + (0.3 * cos(wirePhase * 2 * .pi))
                            default:
                                return 0.25
                            }
                        }()
                        let length = min(max(4, maxHeight * wireMod), maxHeight)
                        let startX = center.x + cos(angle) * (wireRadius - length)
                        let startY = center.y + sin(angle) * (wireRadius - length)
                        let endX = center.x + cos(angle) * wireRadius
                        let endY = center.y + sin(angle) * wireRadius

                        var wirePath = Path()
                        wirePath.move(to: CGPoint(x: startX, y: startY))
                        wirePath.addLine(to: CGPoint(x: endX, y: endY))
                        context.stroke(
                            wirePath,
                            with: .color(ringColor.opacity(state == .speaking ? 0.85 : 0.4)),
                            style: StrokeStyle(lineWidth: wireWidth, lineCap: .round)
                        )
                    }
                }
            }
            .frame(width: size, height: size)

            if size >= 64 {
                Image("HakilleMark")
                    .renderingMode(.original)
                    .resizable()
                    .scaledToFit()
                    .frame(width: size * 0.77, height: size * 0.77)
            }
        }
        .frame(width: size, height: size)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Présence Hakille : \(state.localizedDescription)")
        .onAppear {
            let duration = Double(auraUpdate?.pulsePeriodMs ?? 3000) / 1000.0
            withAnimation(.linear(duration: max(duration, 0.6)).repeatForever(autoreverses: false)) {
                pulsePhase = 1.0
            }
        }
    }
}
