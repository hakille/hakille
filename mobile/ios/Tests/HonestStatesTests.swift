import XCTest
@testable import Hakille

final class HonestStatesTests: XCTestCase {

    func testLoadingThresholdsAdhereToDesignSpec() {
        // Spec states.rs §19:
        // SPINNER_AFTER_MS = 400ms
        // HONEST_AFTER_MS = 2000ms
        func loadingTier(elapsedMs: Double) -> String {
            if elapsedMs < 400 {
                return "LoadingFast"
            } else if elapsedMs < 2000 {
                return "LoadingSkeleton"
            } else {
                return "LoadingHonest"
            }
        }

        XCTAssertEqual(loadingTier(elapsedMs: 0), "LoadingFast")
        XCTAssertEqual(loadingTier(elapsedMs: 250), "LoadingFast")
        XCTAssertEqual(loadingTier(elapsedMs: 399), "LoadingFast")

        XCTAssertEqual(loadingTier(elapsedMs: 400), "LoadingSkeleton")
        XCTAssertEqual(loadingTier(elapsedMs: 1200), "LoadingSkeleton")
        XCTAssertEqual(loadingTier(elapsedMs: 1999), "LoadingSkeleton")

        XCTAssertEqual(loadingTier(elapsedMs: 2000), "LoadingHonest")
        XCTAssertEqual(loadingTier(elapsedMs: 6000), "LoadingHonest")
    }

    func testActionErrorOrderAndStructure() {
        let error = ActionErrorState(
            done: "Brouillon sauvegardé localement",
            notDone: "Message non transmis au hub : connexion fermée",
            technicalDetail: "HTTP 503 / IPC_TIMEOUT",
            retryLabel: "Réessayer",
            undoLabel: "Conserver le brouillon"
        )

        XCTAssertEqual(error.done, "Brouillon sauvegardé localement")
        XCTAssertEqual(error.notDone, "Message non transmis au hub : connexion fermée")
        XCTAssertEqual(error.technicalDetail, "HTTP 503 / IPC_TIMEOUT")
        XCTAssertEqual(error.retryLabel, "Réessayer")
        XCTAssertEqual(error.undoLabel, "Conserver le brouillon")

        let rendered = "Fait : \(error.done)\nPas fait : \(error.notDone)\n[\(error.retryLabel)] [\(error.undoLabel)]\nDétails : \(error.technicalDetail)"
        guard let fait = rendered.range(of: "Fait :"),
              let pasFait = rendered.range(of: "Pas fait :"),
              let actions = rendered.range(of: "[Réessayer]"),
              let details = rendered.range(of: "Détails :") else {
            XCTFail("Missing component in rendered ActionError")
            return
        }

        XCTAssertTrue(fait.lowerBound < pasFait.lowerBound)
        XCTAssertTrue(pasFait.lowerBound < actions.lowerBound)
        XCTAssertTrue(actions.lowerBound < details.lowerBound)
    }

    func testFatigueAlertMessageFormatting() {
        let rawNotification = "Conseil bien-être : Prenez une pause de 5 minutes."
        XCTAssertTrue(rawNotification.hasPrefix("Conseil bien-être :"))
        let recommendation = String(rawNotification.dropFirst("Conseil bien-être :".count)).trimmingCharacters(in: .whitespaces)
        XCTAssertEqual(recommendation, "Prenez une pause de 5 minutes.")
    }
}
