// ZCIE Bar (ADR-022): the token savings of ZCIE in the menu bar, and a detailed report window.
// Opening the app (Finder, Spotlight, `open`) shows the report; the login item starts it with
// `--background` and shows only the menu bar item.

import SwiftUI

extension Notification.Name {
    static let openReport = Notification.Name("ZCIEBarOpenReport")
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    /// No window state from the last run: each open starts at the overview.
    func applicationWillFinishLaunching(_ notification: Notification) {
        UserDefaults.standard.set(false, forKey: "NSQuitAlwaysKeepsWindows")
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard !CommandLine.arguments.contains("--background") else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
            NotificationCenter.default.post(name: .openReport, object: nil)
        }
    }

    /// A second open of the running app shows the report.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
        NotificationCenter.default.post(name: .openReport, object: nil)
        return true
    }
}

@main
struct ZCIEBarApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var store = UsageStore()

    var body: some Scene {
        MenuBarExtra {
            MenuView(store: store)
        } label: {
            BarLabel(store: store)
        }
        .menuBarExtraStyle(.window)

        Window("ZCIE Report", id: "report") {
            ReportView(store: store)
                .frame(minWidth: 980, minHeight: 700)
        }
    }
}

/// The menu bar item. It is always alive, so it opens the report on `openReport`.
struct BarLabel: View {
    @ObservedObject var store: UsageStore
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Label(text, systemImage: "bolt.horizontal.circle")
            .labelStyle(.titleAndIcon)
            .onReceive(NotificationCenter.default.publisher(for: .openReport)) { _ in
                openWindow(id: "report")
                NSApp.activate(ignoringOtherApps: true)
            }
    }

    /// The saving of the project with the last activity, else "ZCIE".
    private var text: String {
        guard let saving = store.current?.saving, saving.percent != nil else { return "ZCIE" }
        return "ZCIE \(savingText(saving))"
    }
}
