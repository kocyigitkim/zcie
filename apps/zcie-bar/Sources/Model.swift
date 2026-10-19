// The report of `zcie usage --json` (ADR-022) and the store that loads it.

import Foundation

struct UsageReport: Codable, Equatable {
    var generatedAt: String
    var windowDays: Int
    var since: String
    var method: String
    var savedTokens: Int64
    var roughSavedTokens: Int64?
    var zcieSessions: Int
    var plainSessions: Int
    var projects: [ProjectReport]
    var automated: Automated?
    var index: IndexReport?
}

struct TokenGroup: Codable, Equatable {
    var sessions: Int
    var turns: Int64
    var tokens: Int64
    var input: Int64
    var cacheWrite: Int64
    var cacheRead: Int64
    var output: Int64
    var tokensPerTurn: Double?
    /// Cost in input-token units (input ×1, cache write ×1.25/×2, cache read ×0.1, output ×5).
    var weighted: Double?
    var weightedPerTurn: Double?
    var readsPerTurn: Double?
    var searchesPerTurn: Double?
    var capsules: Int64
    var capsuleChars: Int64
}

struct Saving: Codable, Equatable {
    var percent: Double?
    var tokens: Int64?
    var note: String
    var confidence: String?
    var excluded: Int?
    /// `project`, `all projects` (rough) or empty.
    var basis: String?

    var rough: Bool { basis == "all projects" }
}

struct ReadGuard: Codable, Equatable {
    var events: Int64
    var fileLines: Int64
    var linesRead: Int64
}

struct Gate: Codable, Equatable {
    var pass: Int64
    var block: Int64
    var skipped: Int64
    var fail: Int64
}

struct DayRow: Codable, Equatable, Identifiable {
    var date: String
    var zcieTokens: Int64
    var plainTokens: Int64
    var zcieTurns: Int64
    var plainTurns: Int64
    var id: String { date }
}

struct ProjectReport: Codable, Equatable, Identifiable {
    var name: String
    var root: String
    var projectId: String?
    var lastActive: String
    var zcie: TokenGroup
    var plain: TokenGroup
    var saving: Saving
    var readGuard: ReadGuard
    var gate: Gate
    var days: [DayRow]
    var sessions: [SessionRow]?
    var id: String { root }

    // `guard` is a Swift keyword; the other keys come from the snake case strategy.
    enum CodingKeys: String, CodingKey {
        case name, root, projectId, lastActive, zcie, plain, saving, gate, days, sessions
        case readGuard = "guard"
    }
}

/// Runs `zcie usage --json` and keeps the last report.
@MainActor
final class UsageStore: ObservableObject {
    @Published var report: UsageReport?
    @Published var error: String?
    @Published var loading = false
    /// Time of the last successful refresh.
    @Published var lastRefresh: Date?
    private var timer: Timer?

    /// Open menus and report windows. With none open, the refresh runs every `idleInterval`.
    private var viewers = 0

    init() {
        refresh()
        // A refresh reads only the new transcript lines (about 30 ms), so the report is live.
        // The common run loop mode keeps the timer running while the menu is open.
        let timer = Timer(timeInterval: UsageStore.interval, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    /// Seconds between two refreshes with no menu and no report open.
    static let idleInterval: TimeInterval = 30

    private func tick() {
        let age = lastRefresh.map { Date().timeIntervalSince($0) } ?? .infinity
        if viewers > 0 || age >= UsageStore.idleInterval { refresh() }
    }

    /// A menu or a report window opened: refresh now, then every `interval`.
    func appeared() {
        viewers += 1
        refresh()
    }

    func disappeared() { viewers = max(0, viewers - 1) }

    /// Seconds between two refreshes.
    static let interval: TimeInterval = 3

    /// The project with the last activity.
    var current: ProjectReport? { report?.projects.first }

    func refresh() {
        guard !loading else { return }
        loading = true
        Task.detached {
            let result = UsageStore.load()
            await MainActor.run {
                switch result {
                case .success(let report):
                    // Redraw only when a value changed (the time stamp changes each run).
                    var same = report
                    same.generatedAt = self.report?.generatedAt ?? ""
                    if same != self.report { self.report = report }
                    self.lastRefresh = Date()
                    self.error = nil
                case .failure(let error): self.error = error.localizedDescription
                }
                self.loading = false
            }
        }
    }

    /// The `zcie` program: the path that `build.sh` found, else the ZCIE home, Homebrew, /usr/local or Cargo.
    nonisolated static func binary() -> String? {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let built = Bundle.main.object(forInfoDictionaryKey: "ZCIEPath") as? String ?? ""
        let paths = [built, "\(home)/.zcie/bin/zcie", "/opt/homebrew/bin/zcie", "/usr/local/bin/zcie", "\(home)/.cargo/bin/zcie"]
        return paths.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    nonisolated static func load() -> Result<UsageReport, Error> {
        guard let path = binary() else { return .failure(LoadError.message("zcie was not found. Run: zcie setup")) }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        // --no-disk: the project roots come from the records, so macOS asks for no protected folder.
        process.arguments = ["usage", "--json", "--no-disk"]
        let output = Pipe()
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            // Read before the wait: a full pipe would stop the program.
            let data = output.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            guard process.terminationStatus == 0 else { return .failure(LoadError.message("zcie usage failed (exit \(process.terminationStatus))")) }
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            return .success(try decoder.decode(UsageReport.self, from: data))
        } catch {
            return .failure(error)
        }
    }
}

enum LoadError: LocalizedError {
    case message(String)
    var errorDescription: String? {
        switch self { case .message(let text): return text }
    }
}

/// "1.3B", "1.2M", "34.5k" or "812".
func short(_ value: Double) -> String {
    let size = abs(value)
    if size >= 1e9 { return String(format: "%.1fB", value / 1e9) }
    if size >= 1e6 { return String(format: "%.1fM", value / 1e6) }
    if size >= 1e4 { return String(format: "%.1fk", value / 1e3) }
    return String(format: "%.0f", value)
}

func short(_ value: Int64) -> String { short(Double(value)) }

/// "−42%" for a saving, "+7%" for more tokens with ZCIE.
func percentText(_ percent: Double) -> String {
    String(format: "%@%.0f%%", percent >= 0 ? "−" : "+", abs(percent))
}

/// "1 session", "3 sessions".
func count(_ value: Int, _ noun: String) -> String { "\(value) \(noun)\(value == 1 ? "" : "s")" }

/// "updated 00:41:12".
func updatedText(_ date: Date?) -> String {
    guard let date else { return "updating…" }
    return "updated " + date.formatted(date: .omitted, time: .standard)
}

/// Tokens of today (UTC, the day of the transcripts) in all projects.
func todayTokens(_ report: UsageReport) -> Int64 {
    let format = DateFormatter()
    format.dateFormat = "yyyy-MM-dd"
    format.timeZone = TimeZone(identifier: "UTC")
    let today = format.string(from: Date())
    return report.projects.flatMap(\.days).filter { $0.date == today }.reduce(0) { $0 + $1.zcieTokens + $1.plainTokens }
}

/// The saving %, with "≈" for a rough value.
func savingText(_ saving: Saving) -> String {
    guard let percent = saving.percent else { return "—" }
    return (saving.rough ? "≈" : "") + percentText(percent)
}

struct Automated: Codable, Equatable {
    var sessions: Int
    var cost: Int64
}

/// One session of a project (ADR-024).
struct SessionRow: Codable, Equatable, Identifiable {
    var id: String
    var start: String
    var zcie: Bool
    var turns: Int64
    var cost: Int64
    var costPerTurn: Double
    var subagents: Int64
    var subagentShare: Double
    var outlier: Bool
}
