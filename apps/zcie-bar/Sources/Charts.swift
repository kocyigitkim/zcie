// The charts of ZCIE Bar (Swift Charts).

import Charts
import SwiftUI

private let dayFormat: DateFormatter = {
    let format = DateFormatter()
    format.dateFormat = "yyyy-MM-dd"
    format.timeZone = TimeZone(identifier: "UTC")
    format.locale = Locale(identifier: "en_US_POSIX")
    return format
}()

func parseDay(_ text: String) -> Date { dayFormat.date(from: text) ?? .distantPast }

/// The days of all projects added together, the last `limit` days.
func allDays(_ report: UsageReport, limit: Int = 0) -> [DayRow] {
    var rows: [String: DayRow] = [:]
    for day in report.projects.flatMap(\.days) {
        var row = rows[day.date] ?? DayRow(date: day.date, zcieTokens: 0, plainTokens: 0, zcieTurns: 0, plainTurns: 0)
        row.zcieTokens += day.zcieTokens
        row.plainTokens += day.plainTokens
        row.zcieTurns += day.zcieTurns
        row.plainTurns += day.plainTurns
        rows[day.date] = row
    }
    let sorted = rows.values.sorted { $0.date < $1.date }
    return limit > 0 ? Array(sorted.suffix(limit)) : sorted
}

private let groupScale: KeyValuePairs<String, Color> = ["With ZCIE": Palette.zcie, "No ZCIE": Palette.plain]

/// Axis labels in "1.2M" form.
private func shortAxis() -> some AxisContent {
    AxisMarks(position: .leading) { value in
        AxisGridLine()
        AxisValueLabel { if let number = value.as(Double.self) { Text(short(number)) } }
    }
}

/// Tokens of each day, stacked by group.
struct DailyTokensChart: View {
    let days: [DayRow]
    var height: CGFloat = 200

    var body: some View {
        if days.isEmpty {
            Text("No activity in the window.").foregroundStyle(.secondary).frame(height: 60)
        } else {
            Chart(days) { day in
                BarMark(x: .value("Day", parseDay(day.date), unit: .day), y: .value("Tokens", Double(day.zcieTokens)))
                    .foregroundStyle(by: .value("Group", "With ZCIE"))
                BarMark(x: .value("Day", parseDay(day.date), unit: .day), y: .value("Tokens", Double(day.plainTokens)))
                    .foregroundStyle(by: .value("Group", "No ZCIE"))
            }
            .chartForegroundStyleScale(groupScale)
            .chartYAxis { shortAxis() }
            .chartLegend(position: .top, alignment: .trailing)
            .frame(height: height)
        }
    }
}

/// A small chart of the last days, with no axes.
struct Sparkline: View {
    let days: [DayRow]

    var body: some View {
        Chart(days) { day in
            BarMark(x: .value("Day", parseDay(day.date), unit: .day), y: .value("Tokens", Double(day.zcieTokens)))
                .foregroundStyle(by: .value("Group", "With ZCIE"))
            BarMark(x: .value("Day", parseDay(day.date), unit: .day), y: .value("Tokens", Double(day.plainTokens)))
                .foregroundStyle(by: .value("Group", "No ZCIE"))
        }
        .chartForegroundStyleScale(groupScale)
        .chartXAxis(.hidden)
        .chartYAxis(.hidden)
        .chartLegend(.hidden)
        .frame(height: 44)
    }
}

/// The median tokens per turn of the two groups.
struct PerTurnChart: View {
    let project: ProjectReport

    var body: some View {
        let rows = [("With ZCIE", project.zcie.weightedPerTurn), ("No ZCIE", project.plain.weightedPerTurn)]
        Chart(rows, id: \.0) { row in
            BarMark(x: .value("Tokens per turn", row.1 ?? 0), y: .value("Group", row.0))
                .foregroundStyle(by: .value("Group", row.0))
                .cornerRadius(4)
                .annotation(position: .trailing) { Text(row.1.map(short) ?? "—").font(.caption).monospacedDigit() }
        }
        .chartForegroundStyleScale(groupScale)
        .chartLegend(.hidden)
        .chartXAxis(.hidden)
        .frame(height: 90)
    }
}

/// One bar of a grouped chart.
struct GroupedValue: Identifiable {
    let category: String
    let group: String
    let value: Double
    var id: String { category + "|" + group }
}

/// Read and search calls per turn of the two groups.
struct ToolCallsChart: View {
    let project: ProjectReport

    var body: some View {
        // A group with no session has no bar.
        let rows = [
            (project.zcie, "With ZCIE"), (project.plain, "No ZCIE"),
        ].filter { $0.0.sessions > 0 }.flatMap { group, name in
            [GroupedValue(category: "Read", group: name, value: group.readsPerTurn ?? 0),
             GroupedValue(category: "Search", group: name, value: group.searchesPerTurn ?? 0)]
        }
        Chart(rows) { row in
            BarMark(x: .value("Tool", row.category), y: .value("Calls per turn", row.value))
                .foregroundStyle(by: .value("Group", row.group))
                .position(by: .value("Group", row.group))
                .cornerRadius(3)
                .annotation(position: .top) { Text(String(format: "%.2f", row.value)).font(.caption2).monospacedDigit() }
        }
        .chartForegroundStyleScale(groupScale)
        .chartLegend(.hidden)
        .frame(height: 150)
    }
}

/// The parts of the tokens of each group, in percent of the group.
struct TokenMixChart: View {
    let project: ProjectReport

    private static let parts: KeyValuePairs<String, Color> = ["Input": .blue, "Cache write": .purple, "Cache read": .teal.opacity(0.55), "Output": .pink]

    var body: some View {
        Chart(rows) { row in
            BarMark(x: .value("Share", row.value), y: .value("Group", row.category))
                .foregroundStyle(by: .value("Part", row.group))
        }
        .chartForegroundStyleScale(Self.parts)
        .chartXAxis { AxisMarks(values: [0, 25, 50, 75, 100]) { value in
            AxisGridLine()
            AxisValueLabel { if let number = value.as(Double.self) { Text("\(Int(number))%") } }
        } }
        .chartLegend(position: .bottom)
        .frame(height: 120)
    }

    private var rows: [GroupedValue] {
        [("With ZCIE", project.zcie), ("No ZCIE", project.plain)].filter { $0.1.sessions > 0 }.flatMap { name, group -> [GroupedValue] in
            let total = Double(max(group.tokens, 1))
            let values: [(String, Int64)] = [("Input", group.input), ("Cache write", group.cacheWrite), ("Cache read", group.cacheRead), ("Output", group.output)]
            return values.map { GroupedValue(category: name, group: $0.0, value: Double($0.1) / total * 100) }
        }
    }
}

/// The results of the verification gate as a ring.
struct GateDonut: View {
    let gate: Gate

    private static let colors: KeyValuePairs<String, Color> = ["Passed": .green, "Blocked": .orange, "Failed": .red, "Skipped": .gray]

    var body: some View {
        let rows = [("Passed", gate.pass), ("Blocked", gate.block), ("Failed", gate.fail), ("Skipped", gate.skipped)]
        let total = rows.reduce(0) { $0 + $1.1 }
        if total == 0 {
            Text("No checks yet").font(.caption).foregroundStyle(.secondary).frame(width: 150, height: 120)
        } else {
            Chart(rows.filter { $0.1 > 0 }, id: \.0) { row in
                SectorMark(angle: .value("Checks", Double(row.1)), innerRadius: .ratio(0.62), angularInset: 1.5)
                    .foregroundStyle(by: .value("Result", row.0))
                    .cornerRadius(3)
            }
            .chartForegroundStyleScale(Self.colors)
            .chartLegend(position: .trailing)
            .chartBackground { _ in Text("\(total)").font(.title3.weight(.semibold)).monospacedDigit() }
            .frame(width: 220, height: 120)
        }
    }
}
