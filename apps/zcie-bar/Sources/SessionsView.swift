// The sessions of a project (ADR-024): each with its cost, its subagent share and the outlier
// mark, so that one large task does not hide behind a median.

import Charts
import SwiftUI

struct SessionsSection: View {
    let project: ProjectReport

    var body: some View {
        let rows = project.sessions ?? []
        ReportSection(title: "Sessions (cost per turn, input-token units)", systemImage: "list.number") {
            if rows.isEmpty {
                Text("No session in the window.").foregroundStyle(.secondary)
            } else {
                Chart(rows) { row in
                    BarMark(x: .value("Cost per turn", row.costPerTurn), y: .value("Session", label(row)))
                        .foregroundStyle(row.zcie ? Palette.zcie : Palette.plain)
                        .opacity(row.outlier ? 0.35 : 1)
                        .annotation(position: .trailing) {
                            Text(short(row.costPerTurn) + (row.outlier ? "  outlier, left out" : "")).font(.caption2).foregroundStyle(.secondary)
                        }
                }
                .chartLegend(.hidden)
                .frame(height: CGFloat(rows.count) * 26 + 24)
                Table(rows) {
                    TableColumn("Session") { r in Text(r.id).monospaced() }.width(90)
                    TableColumn("Start") { r in Text(r.start.prefix(16).replacingOccurrences(of: "T", with: " ")).monospacedDigit() }.width(120)
                    TableColumn("ZCIE") { r in Image(systemName: r.zcie ? "bolt.fill" : "circle.dashed").foregroundStyle(r.zcie ? Palette.zcie : .secondary) }.width(40)
                    TableColumn("Turns") { r in Text("\(r.turns)").monospacedDigit() }.width(50)
                    TableColumn("Cost") { r in Text(short(r.cost)).monospacedDigit() }.width(70)
                    TableColumn("Per turn") { r in Text(short(r.costPerTurn)).monospacedDigit() }.width(70)
                    TableColumn("Subagents") { r in Text(r.subagents == 0 ? "—" : "\(r.subagents) · \(Int(r.subagentShare * 100))%").monospacedDigit() }.width(90)
                    TableColumn("In the saving") { r in Text(r.outlier ? "no: outlier" : "yes").foregroundStyle(r.outlier ? .orange : .secondary) }.width(100)
                }
                .frame(height: min(CGFloat(rows.count) * 24 + 32, 320))
                Text("An outlier costs 3 times the median per turn of the project or more (for example many subagents on one large task). It is listed, but the saving leaves it out.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private func label(_ row: SessionRow) -> String { "\(row.id) · \(row.turns) turns" }
}

/// The confidence of a saving: how many sessions it uses.
struct ConfidenceBadge: View {
    let saving: Saving

    var body: some View {
        if let level = saving.confidence, saving.percent != nil {
            Label("confidence \(level)", systemImage: level == "high" ? "checkmark.seal" : (level == "medium" ? "seal" : "exclamationmark.triangle"))
                .font(.caption.weight(.semibold))
                .foregroundStyle(level == "high" ? .green : (level == "medium" ? .yellow : .orange))
                .help(saving.note + ((saving.excluded ?? 0) > 0 ? " · \(saving.excluded ?? 0) outlier session(s) left out" : ""))
        }
    }
}
