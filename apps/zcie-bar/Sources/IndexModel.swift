// The index section of `zcie usage --json` (ADR-023): the daemon, the index of each project, and
// the reads and writes of the activity log.

import Foundation

struct IndexReport: Codable, Equatable {
    var daemon: DaemonState?
    var daemonError: String?
    var projects: [IndexProject]
    var recent: [ActivityOp]
    var log: String
}

struct DaemonState: Codable, Equatable {
    var version: String
    var uptimeS: Int64
    var storageBytes: Int64
    var canonicalSeq: Int64
    var graphSeq: Int64
    var searchSeq: Int64
    var queueInteractive: Int64
    var queueBackground: Int64
    var deadLetter: Int64
}

struct Reads: Codable, Equatable {
    var queries: Int64
    var expands: Int64
    var empty: Int64
    var cached: Int64
    var tokens: Int64
    var results: Int64
    var msTotal: Double
    var count: Int64 { queries + expands }
}

struct Writes: Codable, Equatable {
    var captures: Int64
    var captureBytes: Int64
    var duplicates: Int64
    var events: [String: Int64]
    var jobs: Int64
    var jobsFailed: Int64
    var jobKinds: [String: Int64]
    var keys: Int64
    var searchOps: Int64
    var jobMs: Double
}

struct Activity: Codable, Equatable {
    var reads: Reads
    var writes: Writes
    var last: String
}

struct IndexProject: Codable, Equatable, Identifiable {
    var projectId: String
    var name: String
    var roots: [String]
    var files: Int64
    var symbols: Int64
    var commits: Int64
    var memories: Int64
    var pendingJobs: Int64
    var indexed: Bool
    var activity: Activity
    var readMsP50: Double?
    var readMsP95: Double?
    var id: String { projectId }
}

/// One line of the activity log. The fields depend on `op`.
struct ActivityOp: Codable, Equatable, Identifiable {
    var ts: String?
    var op: String?
    var ws: String?
    var ms: Double?
    var client: String?
    var question: String?
    var `class`: String?
    var results: Int?
    var tokens: Int?
    var refs: Int?
    var items: Int?
    var firstRef: String?
    var eventType: String?
    var bytes: Int?
    var duplicate: Bool?
    var kind: String?
    var target: String?
    var ok: Bool?
    var error: String?
    var keys: Int?
    var searchOps: Int?
    var id: String { (ts ?? "") + (op ?? "") + (ws ?? "") + String(ms ?? 0) + (target ?? question ?? firstRef ?? "") }

    /// A short description of the operation.
    var detail: String {
        switch op {
        case "query": return "\"\(question ?? "")\" → \(results ?? 0) results, \(tokens ?? 0) tokens" + (self.class.map { " · \($0)" } ?? "")
        case "expand": return "\(firstRef ?? "")" + ((refs ?? 0) > 1 ? " +\((refs ?? 1) - 1)" : "") + " → \(items ?? 0) items, \(tokens ?? 0) tokens"
        case "capture": return "\(eventType ?? "") · \(bytes ?? 0) B" + (duplicate == true ? " · duplicate" : "")
        case "job": return "\(kind ?? "") \(target ?? "")" + (ok == false ? " · FAILED: \(error ?? "")" : " · \(keys ?? 0) keys, \(searchOps ?? 0) search ops")
        default: return op ?? ""
        }
    }

    var isRead: Bool { op == "query" || op == "expand" }
}

/// "1.2 GB", "34 MB".
func bytesText(_ bytes: Int64) -> String {
    ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
}

/// "3 h 12 min".
func durationText(_ seconds: Int64) -> String {
    let formatter = DateComponentsFormatter()
    formatter.allowedUnits = seconds >= 3600 ? [.day, .hour, .minute] : [.minute, .second]
    formatter.unitsStyle = .abbreviated
    return formatter.string(from: TimeInterval(seconds)) ?? "\(seconds) s"
}

/// The local clock time of an RFC 3339 time stamp ("01:46:12").
func clockText(_ ts: String?) -> String {
    guard let ts else { return "" }
    let format = ISO8601DateFormatter()
    format.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let date = format.date(from: ts) ?? ISO8601DateFormatter().date(from: ts)
    return date?.formatted(date: .omitted, time: .standard) ?? String(ts.prefix(19))
}
