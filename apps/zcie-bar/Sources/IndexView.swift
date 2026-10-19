// The index page of ZCIE Bar (ADR-023): the daemon, the index of each project, its reads and
// writes, and the last operations of the activity log, live.

import Charts
import SwiftUI

struct IndexPage: View {
    let index: IndexReport
    let lastRefresh: Date?
    @State private var filter = "all"

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Index").font(.largeTitle.weight(.semibold))
                HStack(spacing: 6) {
                    Circle().fill(index.daemon == nil ? Color.orange : Color.green).frame(width: 7, height: 7)
                    Text("Live · reads and writes of the ZCIE daemon · \(updatedText(lastRefresh))")
                }
                .foregroundStyle(.secondary)
            }
            daemon
            ReportSection(title: "Reads and writes of each project", systemImage: "arrow.left.arrow.right") { activityChart }
            ReportSection(title: "Projects in the index", systemImage: "externaldrive") { IndexTable(projects: index.projects) }
            ReportSection(title: "Last operations", systemImage: "list.bullet.rectangle") { recent }
            MethodNote(text: "Reads: query (a question of a hook, the MCP server or the CLI) and expand (source lines). Writes: capture (an event: a message, a tool call, a file change) and job (indexing a file, a commit, a memory extraction, a scan). Each operation is a line of \(index.log).")
        }
    }

    @ViewBuilder private var daemon: some View {
        if let d = index.daemon {
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 3), spacing: 10) {
                KPITile(title: "Daemon", value: "v\(d.version)", systemImage: "server.rack", caption: "running \(durationText(d.uptimeS))", tint: .green)
                KPITile(title: "Storage", value: bytesText(d.storageBytes), systemImage: "internaldrive", caption: "store + blobs", tint: .primary)
                KPITile(title: "Events", value: short(Double(d.canonicalSeq)), systemImage: "number", caption: lag(d), tint: d.canonicalSeq == d.searchSeq && d.canonicalSeq == d.graphSeq ? Palette.zcie : .orange)
                KPITile(title: "Queue", value: "\(d.queueInteractive + d.queueBackground)", systemImage: "tray.full", caption: "\(d.queueInteractive) interactive, \(d.queueBackground) background", tint: .primary)
                KPITile(title: "Failed jobs", value: "\(d.deadLetter)", systemImage: "exclamationmark.triangle", caption: "dead letter", tint: d.deadLetter > 0 ? .orange : .primary)
                KPITile(title: "Projects", value: "\(index.projects.count)", systemImage: "folder", caption: "\(index.projects.filter(\.indexed).count) with a code index", tint: .primary)
            }
        } else {
            Label("The daemon does not answer: \(index.daemonError ?? "no state yet"). The log below still shows the last operations.", systemImage: "exclamationmark.triangle")
                .foregroundStyle(.orange).card()
        }
    }

    private func lag(_ d: DaemonState) -> String {
        let graph = d.canonicalSeq - d.graphSeq
        let search = d.canonicalSeq - d.searchSeq
        return graph == 0 && search == 0 ? "graph and search are current" : "behind: graph \(graph), search \(search)"
    }

    private var activityChart: some View {
        let active = index.projects.filter { $0.activity.reads.count + $0.activity.writes.captures + $0.activity.writes.jobs > 0 }.prefix(12)
        return Group {
            if active.isEmpty {
                Text("No operation in the window yet.").foregroundStyle(.secondary)
            } else {
                Chart {
                    ForEach(Array(active)) { p in
                        BarMark(x: .value("Operations", Double(p.activity.reads.queries)), y: .value("Project", p.name)).foregroundStyle(by: .value("Kind", "Queries"))
                        BarMark(x: .value("Operations", Double(p.activity.reads.expands)), y: .value("Project", p.name)).foregroundStyle(by: .value("Kind", "Expands"))
                        BarMark(x: .value("Operations", Double(p.activity.writes.captures)), y: .value("Project", p.name)).foregroundStyle(by: .value("Kind", "Captures"))
                        BarMark(x: .value("Operations", Double(p.activity.writes.jobs)), y: .value("Project", p.name)).foregroundStyle(by: .value("Kind", "Index jobs"))
                    }
                }
                .chartForegroundStyleScale(["Queries": Color.teal, "Expands": Color.cyan, "Captures": Color.purple, "Index jobs": Color.indigo])
                .chartXScale(type: .symmetricLog)
                .chartLegend(position: .top, alignment: .leading)
                .frame(height: CGFloat(active.count) * 26 + 50)
                Text("Log scale: index jobs of a first scan are thousands; queries are a few for each prompt.").font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private var recent: some View {
        let names = Dictionary(index.projects.map { ($0.projectId, $0.name) }, uniquingKeysWith: { a, _ in a })
        let ops = index.recent.filter { filter == "all" || (filter == "reads" ? $0.isRead : !$0.isRead) }.prefix(80)
        return VStack(alignment: .leading, spacing: 8) {
            Picker("", selection: $filter) {
                Text("All").tag("all")
                Text("Reads").tag("reads")
                Text("Writes").tag("writes")
            }
            .pickerStyle(.segmented)
            .frame(width: 260)
            ForEach(Array(ops)) { op in OpRow(op: op, project: names[op.ws ?? ""] ?? op.ws ?? "") }
        }
    }
}

/// One operation of the log.
struct OpRow: View {
    let op: ActivityOp
    let project: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text(clockText(op.ts)).font(.caption.monospacedDigit()).foregroundStyle(.secondary).frame(width: 70, alignment: .leading)
            Label(op.op ?? "", systemImage: icon).font(.caption.weight(.semibold)).foregroundStyle(tint).frame(width: 82, alignment: .leading)
            Text(project).font(.caption).lineLimit(1).frame(width: 130, alignment: .leading)
            Text(op.detail).font(.caption).lineLimit(1).truncationMode(.middle).frame(maxWidth: .infinity, alignment: .leading).help(op.detail)
            Text(String(format: "%.1f ms", op.ms ?? 0)).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
        }
    }

    private var icon: String {
        switch op.op {
        case "query": return "magnifyingglass"
        case "expand": return "doc.text"
        case "capture": return "square.and.arrow.down"
        default: return op.ok == false ? "xmark.octagon" : "gearshape.2"
        }
    }

    private var tint: Color { op.ok == false ? .red : (op.isRead ? .teal : .indigo) }
}

/// The index of each project as a table.
struct IndexTable: View {
    let projects: [IndexProject]

    var body: some View {
        Table(projects) {
            TableColumn("Project") { p in
                HStack(spacing: 4) {
                    Image(systemName: p.indexed ? "checkmark.circle.fill" : "brain").foregroundStyle(p.indexed ? .green : .secondary)
                        .help(p.indexed ? "Code index: files are indexed and watched" : "Memory only: add the folder with `zcie project add` for a code index")
                    Text(p.name).lineLimit(1).help(p.roots.joined(separator: "\n"))
                }
            }
            .width(min: 140, ideal: 190)
            TableColumn("Files") { p in Text(short(Double(p.files))).monospacedDigit() }.width(56)
            TableColumn("Symbols") { p in Text(short(Double(p.symbols))).monospacedDigit() }.width(64)
            TableColumn("Memories") { p in Text(short(Double(p.memories))).monospacedDigit() }.width(70)
            TableColumn("Waiting") { p in Text("\(p.pendingJobs)").monospacedDigit().foregroundStyle(p.pendingJobs > 0 ? .orange : .primary) }.width(56)
            TableColumn("Reads") { p in Text("\(p.activity.reads.queries) q / \(p.activity.reads.expands) e").monospacedDigit() }.width(86)
            TableColumn("Read ms p50/p95") { p in Text(msText(p.readMsP50) + " / " + msText(p.readMsP95)).monospacedDigit() }.width(108)
            TableColumn("Writes") { p in Text("\(p.activity.writes.captures) ev / \(short(Double(p.activity.writes.jobs))) jobs").monospacedDigit() }.width(118)
            TableColumn("Failed") { p in Text("\(p.activity.writes.jobsFailed)").monospacedDigit().foregroundStyle(p.activity.writes.jobsFailed > 0 ? .red : .primary) }.width(48)
            TableColumn("Last") { p in Text(clockText(p.activity.last.isEmpty ? nil : p.activity.last)).monospacedDigit().foregroundStyle(.secondary) }.width(70)
        }
        .frame(height: min(CGFloat(projects.count) * 24 + 32, 420))
    }

    private func msText(_ value: Double?) -> String {
        guard let value else { return "—" }
        return value.isInfinite ? ">5s" : "≤\(Int(value))"
    }
}

/// The index of one project, on its page.
struct ProjectIndexSection: View {
    let project: IndexProject

    var body: some View {
        let reads = project.activity.reads
        let writes = project.activity.writes
        ReportSection(title: "Index", systemImage: "externaldrive") {
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 4), spacing: 10) {
                KPITile(title: "Files", value: short(Double(project.files)), systemImage: "doc", caption: project.indexed ? "indexed and watched" : "memory only", tint: project.indexed ? .green : .secondary)
                KPITile(title: "Symbols", value: short(Double(project.symbols)), systemImage: "curlybraces", caption: "\(project.commits) commits", tint: .primary)
                KPITile(title: "Memories", value: short(Double(project.memories)), systemImage: "brain", caption: "\(project.pendingJobs) jobs waiting", tint: .primary)
                KPITile(title: "Reads", value: "\(reads.count)", systemImage: "magnifyingglass", caption: "\(reads.queries) queries, \(reads.empty) empty", tint: Palette.zcie)
                KPITile(title: "Read time", value: project.readMsP95.map { $0.isInfinite ? ">5 s" : "≤\(Int($0)) ms" } ?? "—", systemImage: "timer", caption: "p95; p50 " + (project.readMsP50.map { "≤\(Int($0)) ms" } ?? "—"), tint: .primary)
                KPITile(title: "Tokens read", value: short(Double(reads.tokens)), systemImage: "text.alignleft", caption: "\(reads.cached) from the cache", tint: .primary)
                KPITile(title: "Events written", value: "\(writes.captures)", systemImage: "square.and.arrow.down", caption: bytesText(writes.captureBytes) + ", \(writes.duplicates) duplicates", tint: .purple)
                KPITile(title: "Index jobs", value: short(Double(writes.jobs)), systemImage: "gearshape.2", caption: "\(short(Double(writes.keys))) keys, \(writes.jobsFailed) failed", tint: writes.jobsFailed > 0 ? .red : .indigo)
            }
            if !writes.jobKinds.isEmpty || !writes.events.isEmpty {
                Text((writes.jobKinds.sorted { $0.value > $1.value }.map { "\($0.key) \($0.value)" } + writes.events.sorted { $0.value > $1.value }.map { "\($0.key) \($0.value)" }).joined(separator: " · "))
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}
