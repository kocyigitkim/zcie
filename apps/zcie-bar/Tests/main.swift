// TS-22-06 (ADR-022): the app decodes the report of `zcie usage --json` from stdin.

import Foundation

let data = FileHandle.standardInput.readDataToEndOfFile()
let decoder = JSONDecoder()
decoder.keyDecodingStrategy = .convertFromSnakeCase
do {
    let report = try decoder.decode(UsageReport.self, from: data)
    let first = report.projects.first
    print("projects \(report.projects.count) first \(first?.name ?? "-") saving \(first?.saving.percent.map(percentText) ?? first?.saving.note ?? "-") guard \(first?.readGuard.events ?? 0) index \(report.index?.projects.count ?? -1) recent \(report.index?.recent.count ?? -1)")
} catch {
    print("decode failed: \(error)")
    exit(1)
}
