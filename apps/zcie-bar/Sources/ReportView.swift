// The detailed report window of ZCIE Bar: an overview of all projects, and a page for each project
// with charts of the two groups, the actions of ZCIE and the tokens of each day.

import Charts
import SwiftUI

/// The sidebar items of the overview and the index.
private let overviewTag = "overview"
private let indexTag = "index"

struct ReportView: View {
    @ObservedObject var store: UsageStore
    @State private var selection: String? = overviewTag

    var body: some View {
        NavigationSplitView {
            sidebar.navigationSplitViewColumnWidth(min: 230, ideal: 260)
        } detail: {
            if let report = store.report {
                ScrollView {
                    if selection == indexTag, let index = report.index {
                        IndexPage(index: index, lastRefresh: store.lastRefresh).padding(20)
                    } else if let project = report.projects.first(where: { $0.root == selection }) {
                        ProjectPage(project: project, report: report).padding(20)
                    } else {
                        OverviewPage(report: report, lastRefresh: store.lastRefresh, selection: $selection).padding(20)
                    }
                }
            } else {
                Text(store.error ?? "Scanning the Claude Code sessions…").foregroundStyle(.secondary)
            }
        }
        // Each open starts at the overview; an open window refreshes every few seconds.
        .onAppear {
            selection = overviewTag
            store.appeared()
        }
        .onDisappear { store.disappeared() }
        .onReceive(NotificationCenter.default.publisher(for: .openReport)) { _ in selection = overviewTag }
        .toolbar {
            ToolbarItem {
                Button { store.refresh() } label: { Label("Refresh", systemImage: "arrow.clockwise") }.disabled(store.loading)
            }
        }
    }

    private var sidebar: some View {
        let projects = store.report?.projects ?? []
        return List(selection: $selection) {
            Label("Overview", systemImage: "square.grid.2x2").tag(overviewTag)
            Label("Index", systemImage: "externaldrive").tag(indexTag)
            SwiftUI.Section("Using ZCIE") {
                ForEach(projects.filter { $0.zcie.sessions > 0 }) { SidebarRow(project: $0).tag($0.root) }
            }
            SwiftUI.Section("Not using ZCIE yet") {
                ForEach(projects.filter { $0.zcie.sessions == 0 }) { SidebarRow(project: $0).tag($0.root) }
            }
        }
    }
}

struct SidebarRow: View {
    let project: ProjectReport

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 1) {
                Text(project.name).lineLimit(1)
                Text(count(project.zcie.sessions + project.plain.sessions, "session")).font(.caption2).foregroundStyle(.secondary)
            }
            Spacer()
            SavingBadge(saving: project.saving)
        }
    }
}

/// All projects: key numbers, the tokens of each day, and the saving of each project.
struct OverviewPage: View {
    let report: UsageReport
    let lastRefresh: Date?
    @Binding var selection: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Overview").font(.largeTitle.weight(.semibold))
                HStack(spacing: 6) {
                    Circle().fill(Color.green).frame(width: 7, height: 7)
                    Text("Live, every \(Int(UsageStore.interval)) s · last \(report.windowDays) days, since \(report.since) · \(updatedText(lastRefresh))")
                }
                .foregroundStyle(.secondary)
            }
            kpis
            ReportSection(title: "Tokens of each day, all projects", systemImage: "chart.bar") {
                DailyTokensChart(days: allDays(report), height: 220)
            }
            savingsChart
            MethodNote(text: report.method)
        }
    }

    private var kpis: some View {
        let capsules = report.projects.reduce(Int64(0)) { $0 + $1.zcie.capsules }
        let blocks = report.projects.reduce(Int64(0)) { $0 + $1.gate.block }
        let used = report.projects.filter { $0.zcie.sessions > 0 }.count
        let rough = report.roughSavedTokens ?? 0
        return LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 4), spacing: 10) {
            KPITile(title: "Tokens saved", value: short(report.savedTokens + rough), systemImage: "leaf",
                    caption: rough != 0 ? "\(short(report.savedTokens)) observed + ≈ \(short(rough)) rough" : "observed",
                    tint: report.savedTokens + rough >= 0 ? Palette.saving : Palette.loss)
            KPITile(title: "Tokens today", value: short(todayTokens(report)), systemImage: "clock", caption: "all projects, live", tint: Palette.zcie)
            KPITile(title: "ZCIE sessions", value: "\(report.zcieSessions)", systemImage: "bolt", caption: "\(used) projects", tint: Palette.zcie)
            KPITile(title: "Plain sessions", value: "\(report.plainSessions)", systemImage: "circle.dashed", caption: "no ZCIE, the baseline", tint: .secondary)
            KPITile(title: "Capsules", value: short(capsules), systemImage: "shippingbox", caption: "given to Claude", tint: .primary)
            KPITile(title: "Errors caught", value: "\(blocks)", systemImage: "checkmark.shield", caption: "verification gate", tint: .primary)
            KPITile(title: "Automated sessions", value: short(report.automated?.cost ?? 0), systemImage: "gearshape.arrow.triangle.2.circlepath", caption: "\(report.automated?.sessions ?? 0) runs of programs (claude -p), not in the saving", tint: .secondary)
        }
    }

    private var savingsChart: some View {
        let rows = report.projects.filter { $0.saving.percent != nil }
        return ReportSection(title: "Saving of each project", systemImage: "chart.bar.doc.horizontal") {
            if rows.isEmpty {
                Text("No project has a saving value yet: a project needs one session with ZCIE, and one session with no ZCIE in it or in any project (rough ≈).")
                    .font(.callout).foregroundStyle(.secondary)
            } else {
                Chart(rows) { project in
                    BarMark(x: .value("Saving %", project.saving.percent ?? 0), y: .value("Project", project.name))
                        .foregroundStyle(Palette.of(project.saving.percent ?? 0).opacity(project.saving.rough ? 0.5 : 1))
                        .cornerRadius(4)
                        .annotation(position: .trailing) { Text(savingText(project.saving)).font(.caption).monospacedDigit() }
                }
                .frame(height: CGFloat(rows.count) * 30 + 30)
            }
        }
    }
}

/// One project: the saving, charts of the two groups, the actions of ZCIE and the numbers.
struct ProjectPage: View {
    let project: ProjectReport
    let report: UsageReport

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            header
            kpis
            HStack(alignment: .top, spacing: 16) {
                ReportSection(title: "Cost per turn (median, input-token units)", systemImage: "gauge.with.dots.needle.33percent") { PerTurnChart(project: project) }
                ReportSection(title: "Tool calls per turn", systemImage: "wrench.and.screwdriver") { ToolCallsChart(project: project) }
            }
            ReportSection(title: "What the tokens are", systemImage: "square.stack.3d.up") {
                TokenMixChart(project: project)
                Text("Cache reads cost about a tenth of new input tokens.").font(.caption).foregroundStyle(.secondary)
            }
            ReportSection(title: "Tokens of each day", systemImage: "calendar") { DailyTokensChart(days: project.days) }
            SessionsSection(project: project)
            ActionsSection(project: project)
            if let index = report.index?.projects.first(where: { $0.projectId == project.projectId }) {
                ProjectIndexSection(project: index)
            }
            ReportSection(title: "Numbers", systemImage: "tablecells") {
                DisclosureGroup("Show the table") { ComparisonTable(project: project) }
            }
            MethodNote(text: report.method)
        }
    }

    private var header: some View {
        HStack(alignment: .center, spacing: 18) {
            VStack(alignment: .leading, spacing: 3) {
                Text(project.name).font(.largeTitle.weight(.semibold))
                Text(project.root).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                Text(project.saving.note + ((project.saving.excluded ?? 0) > 0 ? "; \(project.saving.excluded ?? 0) outlier session(s) left out" : ""))
                    .font(.callout).foregroundStyle(.secondary)
                ConfidenceBadge(saving: project.saving)
            }
            Spacer()
            SavingGauge(project: project, size: 84)
        }
    }

    private var kpis: some View {
        HStack(spacing: 10) {
            KPITile(title: "Tokens saved", value: project.saving.tokens.map(short) ?? "—", systemImage: "leaf", tint: project.saving.percent.map(Palette.of) ?? .secondary)
            KPITile(title: "Cost/turn, ZCIE", value: project.zcie.weightedPerTurn.map(short) ?? "—", systemImage: "bolt", caption: count(project.zcie.sessions, "session"), tint: Palette.zcie)
            KPITile(title: "Cost/turn, no ZCIE", value: project.plain.weightedPerTurn.map(short) ?? "—", systemImage: "circle.dashed", caption: count(project.plain.sessions, "session"), tint: .secondary)
            KPITile(title: "Turns", value: short(project.zcie.turns + project.plain.turns), systemImage: "text.bubble", caption: "your messages", tint: .primary)
        }
    }
}

/// What ZCIE did in the project: capsules, read guard, verification gate.
struct ActionsSection: View {
    let project: ProjectReport

    var body: some View {
        ReportSection(title: "Actions of ZCIE", systemImage: "bolt.shield") {
            HStack(alignment: .center, spacing: 12) {
                KPITile(title: "Context capsules", value: short(project.zcie.capsules), systemImage: "shippingbox", caption: "≈ \(short(project.zcie.capsuleChars / 4)) tokens", tint: Palette.zcie)
                KPITile(title: "Large reads limited", value: short(project.readGuard.events), systemImage: "doc.text.magnifyingglass", caption: "\(short(project.readGuard.fileLines - project.readGuard.linesRead)) lines not read", tint: .primary)
                GateDonut(gate: project.gate).card()
            }
            Text("The read guard and the gate are counted from 2026-10-19, the start of the hook statistics.").font(.caption).foregroundStyle(.secondary)
        }
    }
}

/// The two groups side by side, as numbers.
struct ComparisonTable: View {
    let project: ProjectReport

    var body: some View {
        Grid(alignment: .trailing, horizontalSpacing: 28, verticalSpacing: 6) {
            GridRow {
                Text("").gridColumnAlignment(.leading)
                Text("With ZCIE").bold()
                Text("No ZCIE").bold()
            }
            Divider()
            row("Sessions", Double(project.zcie.sessions), Double(project.plain.sessions))
            row("Turns", Double(project.zcie.turns), Double(project.plain.turns))
            row("Cost per turn (median, input-token units)", project.zcie.weightedPerTurn, project.plain.weightedPerTurn)
            row("Raw tokens per turn (median)", project.zcie.tokensPerTurn, project.plain.tokensPerTurn)
            row("Read calls per turn", project.zcie.readsPerTurn, project.plain.readsPerTurn, decimals: true)
            row("Search calls per turn", project.zcie.searchesPerTurn, project.plain.searchesPerTurn, decimals: true)
            row("Input tokens", Double(project.zcie.input), Double(project.plain.input))
            row("Cache write tokens", Double(project.zcie.cacheWrite), Double(project.plain.cacheWrite))
            row("Cache read tokens", Double(project.zcie.cacheRead), Double(project.plain.cacheRead))
            row("Output tokens", Double(project.zcie.output), Double(project.plain.output))
        }
        .padding(.top, 6)
    }

    private func row(_ title: String, _ with: Double?, _ without: Double?, decimals: Bool = false) -> some View {
        let text = { (value: Double?) -> String in
            guard let value else { return "—" }
            return decimals ? String(format: "%.2f", value) : short(value)
        }
        return GridRow {
            Text(title).gridColumnAlignment(.leading)
            Text(text(with)).monospacedDigit()
            Text(text(without)).monospacedDigit()
        }
    }
}

/// How the saving is measured.
struct MethodNote: View {
    let text: String

    var body: some View {
        Label { Text(text).font(.caption) } icon: { Image(systemName: "info.circle") }
            .foregroundStyle(.secondary)
            .card()
    }
}
