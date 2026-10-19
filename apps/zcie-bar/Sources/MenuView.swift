// The menu of ZCIE Bar: a summary, the last active project, the tokens of the last days, and the
// projects that used ZCIE.

import SwiftUI

struct MenuView: View {
    @ObservedObject var store: UsageStore
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            if let report = store.report {
                summary(report)
                if let project = store.current {
                    CurrentProjectCard(project: project)
                }
                recentDays(report)
                projects(report)
            } else if let error = store.error {
                Label(error, systemImage: "exclamationmark.triangle").foregroundStyle(.orange).card()
            } else {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Scanning the Claude Code sessions…").foregroundStyle(.secondary)
                }
                .card()
            }
            footer
        }
        .padding(14)
        .frame(width: 380)
        .onAppear { store.appeared() }
        .onDisappear { store.disappeared() }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "bolt.horizontal.circle.fill").font(.title2).foregroundStyle(Palette.zcie)
            VStack(alignment: .leading, spacing: 0) {
                Text("ZCIE").font(.headline)
                HStack(spacing: 4) {
                    Circle().fill(store.error == nil ? Color.green : Color.orange).frame(width: 6, height: 6)
                    Text("Live · last \(store.report?.windowDays ?? 90) days · \(updatedText(store.lastRefresh))")
                }
                .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Button { store.refresh() } label: {
                if store.loading { ProgressView().controlSize(.small) } else { Image(systemName: "arrow.clockwise") }
            }
            .buttonStyle(.borderless)
            .help("Refresh")
            .disabled(store.loading)
        }
    }

    private func summary(_ report: UsageReport) -> some View {
        let used = report.projects.filter { $0.zcie.sessions > 0 }.count
        let rough = report.roughSavedTokens ?? 0
        return HStack(spacing: 8) {
            KPITile(title: "Saved", value: short(report.savedTokens + rough), systemImage: "leaf",
                    caption: rough != 0 ? "≈ \(short(rough)) rough" : "observed",
                    tint: report.savedTokens + rough >= 0 ? Palette.saving : Palette.loss)
            KPITile(title: "Today", value: short(todayTokens(report)), systemImage: "clock", caption: "tokens, live", tint: Palette.zcie)
            KPITile(title: "Projects", value: "\(used) / \(report.projects.count)", systemImage: "folder", caption: "with ZCIE", tint: .primary)
        }
    }

    private func recentDays(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Tokens per day, last 14 days").font(.caption.weight(.semibold))
                Spacer()
                Legend()
            }
            Sparkline(days: allDays(report, limit: 14))
        }
        .card()
    }

    /// The projects that used ZCIE in the window; the others are in the report only.
    private func projects(_ report: UsageReport) -> some View {
        let used = report.projects.filter { $0.zcie.sessions > 0 }
        return VStack(alignment: .leading, spacing: 8) {
            Text("Projects with ZCIE").font(.caption.weight(.semibold))
            if used.isEmpty {
                Text("No project used ZCIE in the last \(report.windowDays) days.").font(.caption).foregroundStyle(.secondary)
            }
            ForEach(used.prefix(5)) { project in
                ProjectRow(project: project)
            }
            let others = report.projects.count - min(used.count, 5)
            if others > 0 {
                Text("\(others) more projects in the detailed report").font(.caption2).foregroundStyle(.secondary)
            }
        }
        .card()
    }

    private var footer: some View {
        HStack {
            Button {
                openWindow(id: "report")
                NSApp.activate(ignoringOtherApps: true)
            } label: {
                Label("Detailed report", systemImage: "chart.bar.xaxis")
            }
            .buttonStyle(.borderedProminent)
            .keyboardShortcut("r")
            Spacer()
            Button("Quit") { NSApp.terminate(nil) }.buttonStyle(.bordered)
        }
    }
}

/// The colors of the two groups.
struct Legend: View {
    var body: some View {
        HStack(spacing: 10) {
            item("ZCIE", Palette.zcie)
            item("No ZCIE", Palette.plain)
        }
        .font(.caption2).foregroundStyle(.secondary)
    }

    private func item(_ title: String, _ color: Color) -> some View {
        HStack(spacing: 4) {
            Circle().fill(color).frame(width: 7, height: 7)
            Text(title)
        }
    }
}

/// The project with the last activity: the saving gauge, or the progress to a value.
struct CurrentProjectCard: View {
    let project: ProjectReport

    var body: some View {
        HStack(alignment: .center, spacing: 14) {
            SavingGauge(project: project, size: 62)
            VStack(alignment: .leading, spacing: 4) {
                Text("Last active").font(.caption2).foregroundStyle(.secondary)
                Text(project.name).font(.headline).lineLimit(1)
                if let tokens = project.saving.tokens {
                    Text("\(short(tokens)) tokens saved").font(.caption).foregroundStyle(.secondary)
                    CompareBars(with: project.zcie.weightedPerTurn, without: project.plain.weightedPerTurn).frame(width: 150)
                } else {
                    Text(project.saving.note).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                }
            }
            Spacer(minLength: 0)
        }
        .card()
    }
}

/// One project of the menu: the comparison bars and the saving badge.
struct ProjectRow: View {
    let project: ProjectReport

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                Text(project.name).font(.callout).lineLimit(1)
                CompareBars(with: project.zcie.weightedPerTurn, without: project.plain.weightedPerTurn)
                Text(detail).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 8)
            SavingBadge(saving: project.saving)
        }
    }

    private var detail: String {
        if let tokens = project.saving.tokens {
            return "\(short(tokens)) saved · \(project.zcie.sessions) ZCIE / \(project.plain.sessions) plain sessions"
        }
        return project.saving.note
    }
}
