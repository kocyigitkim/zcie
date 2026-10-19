// Shared views of ZCIE Bar: colors, cards, KPI tiles, the saving gauge and badge.

import SwiftUI

enum Palette {
    static let zcie = Color.teal
    static let plain = Color.gray.opacity(0.55)
    static let saving = Color.green
    static let loss = Color.orange

    static func of(_ percent: Double) -> Color { percent >= 0 ? saving : loss }
}

/// A rounded card with the control background.
struct CardModifier: ViewModifier {
    var padding: CGFloat = 12

    func body(content: Content) -> some View {
        content
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color(nsColor: .controlBackgroundColor)))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color.primary.opacity(0.08)))
    }
}

extension View {
    func card(padding: CGFloat = 12) -> some View { modifier(CardModifier(padding: padding)) }
}

/// A titled section of the report.
struct ReportSection<Content: View>: View {
    let title: String
    let systemImage: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(title, systemImage: systemImage).font(.headline)
            content
        }
        .card(padding: 14)
    }
}

/// One key number: a title, the value and a caption.
struct KPITile: View {
    let title: String
    let value: String
    let systemImage: String
    var caption: String = ""
    var tint: Color = .accentColor

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Label(title, systemImage: systemImage).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            Text(value).font(.title2.weight(.semibold)).monospacedDigit().foregroundStyle(tint).lineLimit(1).minimumScaleFactor(0.6)
            if !caption.isEmpty {
                Text(caption).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .card()
    }
}

/// The saving % in a ring; with no value, the number of sessions each group still needs.
struct SavingGauge: View {
    let project: ProjectReport
    var size: CGFloat = 64

    var body: some View {
        if let percent = project.saving.percent {
            ZStack {
                Circle().stroke(Color.primary.opacity(0.08), lineWidth: size / 9)
                Circle()
                    .trim(from: 0, to: min(abs(percent), 100) / 100)
                    .stroke(Palette.of(percent), style: StrokeStyle(lineWidth: size / 9, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                Text(savingText(project.saving)).font(.system(size: size / 4.6, weight: .bold, design: .rounded)).monospacedDigit()
            }
            .frame(width: size, height: size)
        } else {
            BaselineProgress(project: project).frame(width: max(size * 2.4, 150))
        }
    }
}

/// No saving value yet: the reason.
struct BaselineProgress: View {
    let project: ProjectReport

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Label("No value yet", systemImage: "hourglass").font(.caption.weight(.semibold))
            Text(project.saving.note).font(.caption2).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// The saving %: green for fewer tokens, orange for more, grey with no value.
struct SavingBadge: View {
    let saving: Saving

    var body: some View {
        if let percent = saving.percent {
            Text(savingText(saving))
                .font(.system(.callout, design: .rounded).weight(.semibold)).monospacedDigit()
                .padding(.horizontal, 7).padding(.vertical, 2)
                .background(Capsule().fill(Palette.of(percent).opacity(saving.rough ? 0.08 : 0.16)))
                .foregroundStyle(Palette.of(percent).opacity(saving.rough ? 0.75 : 1))
                .help(saving.rough ? saving.note : "Against the sessions with no ZCIE of this project")
        } else {
            Text("—").foregroundStyle(.secondary)
        }
    }
}

/// Two thin bars: the tokens per turn with ZCIE and with no ZCIE, to the same scale.
struct CompareBars: View {
    let with: Double?
    let without: Double?

    var body: some View {
        let top = max(with ?? 0, without ?? 0, 1)
        VStack(alignment: .leading, spacing: 2) {
            bar(with, top, Palette.zcie)
            bar(without, top, Palette.plain)
        }
    }

    private func bar(_ value: Double?, _ top: Double, _ tint: Color) -> some View {
        GeometryReader { proxy in
            Capsule().fill(tint).frame(width: max(2, proxy.size.width * (value ?? 0) / top))
        }
        .frame(height: 4)
    }
}
