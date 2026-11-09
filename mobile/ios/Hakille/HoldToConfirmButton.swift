import SwiftUI
import UIKit

public let HOLD_TO_CONFIRM_MS: UInt64 = 600

public func computeHoldProgress(elapsedMs: UInt64, totalMs: UInt64 = HOLD_TO_CONFIRM_MS) -> Float {
    if elapsedMs == 0 { return 0.0 }
    if elapsedMs >= totalMs { return 1.0 }
    return min(1.0, max(0.0, Float(elapsedMs) / Float(totalMs)))
}

public struct HoldToConfirmButton: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.scenePhase) private var scenePhase

    public let onConfirmed: () -> Void
    public let onCancelled: () -> Void
    public let label: String
    public let confirmedLabel: String
    public let isEnabled: Bool

    @State private var isHolding: Bool = false
    @State private var isConfirmed: Bool = false
    @State private var accessibleArmed: Bool = false
    @State private var isGestureAborted: Bool = false
    @State private var progress: Float = 0.0
    @State private var pressStartTime: TimeInterval? = nil
    @State private var timerTask: Task<Void, Never>? = nil

    private let mediumHaptic = UIImpactFeedbackGenerator(style: .medium)
    private let heavyHaptic = UIImpactFeedbackGenerator(style: .heavy)

    public init(
        label: String = "Maintenir 600 ms pour valider",
        confirmedLabel: String = "Action validée",
        isEnabled: Bool = true,
        onConfirmed: @escaping () -> Void,
        onCancelled: @escaping () -> Void = {}
    ) {
        self.label = label
        self.confirmedLabel = confirmedLabel
        self.isEnabled = isEnabled
        self.onConfirmed = onConfirmed
        self.onCancelled = onCancelled
    }

    private var braiseColor: Color {
        scheme == .dark ? HakilleTokens.colorBraiseDark : HakilleTokens.colorBraiseLight
    }

    private var braiseHalo: Color {
        scheme == .dark ? HakilleTokens.colorBraiseHaloDark : HakilleTokens.colorBraiseHaloLight
    }

    private var lineColor: Color {
        scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight
    }

    private func cancelHold() {
        if isHolding {
            isHolding = false
            progress = 0.0
            pressStartTime = nil
            timerTask?.cancel()
            timerTask = nil
            onCancelled()
        }
    }

    public var body: some View {
        ZStack {
            // Background fill with halo during hold
            RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusControl))
                .fill(isHolding ? braiseHalo : Color.clear)

            // Outline and progress ring
            RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusControl))
                .stroke(isHolding ? braiseColor.opacity(0.4) : lineColor, lineWidth: 2)

            // Progressive Braise border
            GeometryReader { geo in
                if progress > 0 {
                    Path { path in
                        path.move(to: CGPoint(x: 0, y: geo.size.height - 2))
                        path.addLine(to: CGPoint(x: geo.size.width * CGFloat(progress), y: geo.size.height - 2))
                    }
                    .stroke(braiseColor, style: StrokeStyle(lineWidth: 4, lineCap: .round))
                }
            }

            HStack(spacing: 12) {
                if isHolding || progress > 0 {
                    ZStack {
                        Circle()
                            .stroke(braiseColor.opacity(0.3), lineWidth: 2)
                        Circle()
                            .trim(from: 0, to: CGFloat(progress))
                            .stroke(braiseColor, style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
                            .rotationEffect(.degrees(-90))
                    }
                    .frame(width: 20, height: 20)
                }

                Text(isConfirmed ? confirmedLabel : (accessibleArmed ? "Toucher pour valider (2/2)" : (isHolding ? "Maintien : \(Int(progress * 100))%" : label)))
                    .font(HakilleTypography.heading)
                    .foregroundStyle(isHolding ? braiseColor : (scheme == .dark ? Color.white : Color.black))
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 16)
        }
        .frame(maxWidth: .infinity, minHeight: 56) // Compliant with minimum 44pt touch target
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { value in
                    guard isEnabled && !isConfirmed && !isGestureAborted else { return }
                    if !isHolding {
                        isHolding = true
                        progress = 0.0
                        // Monotonic system uptime avoids backward jump crashes
                        pressStartTime = ProcessInfo.processInfo.systemUptime
                        mediumHaptic.prepare()
                        heavyHaptic.prepare()
                        mediumHaptic.impactOccurred() // 10ms haptic tick at 0ms

                        timerTask = Task { @MainActor in
                            while !Task.isCancelled {
                                try? await Task.sleep(nanoseconds: 16_000_000) // ~60fps
                                guard let start = pressStartTime else { break }
                                let now = ProcessInfo.processInfo.systemUptime
                                let elapsed = UInt64(max(0.0, (now - start) * 1000.0))
                                progress = computeHoldProgress(elapsedMs: elapsed)
                                if elapsed >= HOLD_TO_CONFIRM_MS {
                                    isConfirmed = true
                                    isHolding = false
                                    heavyHaptic.impactOccurred() // 10ms haptic tick at 600ms
                                    onConfirmed()
                                    break
                                }
                            }
                        }
                    } else {
                        // Drag off boundary check (> 20pt)
                        if abs(value.translation.width) > 20 || abs(value.translation.height) > 20 {
                            isGestureAborted = true
                            cancelHold()
                        }
                    }
                }
                .onEnded { _ in
                    isGestureAborted = false
                    if isHolding && !isConfirmed {
                        cancelHold()
                    }
                }
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(isConfirmed ? confirmedLabel : (accessibleArmed ? "Confirmation armée. Toucher pour valider définitivement." : "Validation sécurisée en deux étapes ou maintien de 600 millisecondes."))
        .accessibilityAddTraits(.isButton)
        .accessibilityAction {
            guard isEnabled && !isConfirmed else { return }
            if !accessibleArmed {
                accessibleArmed = true
                mediumHaptic.impactOccurred()
            } else {
                accessibleArmed = false
                isConfirmed = true
                progress = 1.0
                heavyHaptic.impactOccurred()
                onConfirmed()
            }
        }
        .onChange(of: scenePhase) { newPhase in
            if newPhase == .background || newPhase == .inactive {
                cancelHold()
            }
        }
        .onDisappear {
            cancelHold()
        }
    }
}
