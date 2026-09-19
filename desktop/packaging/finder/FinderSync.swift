import Cocoa
import FinderSync

@objc(FinderSync)
class FinderSync: FIFinderSync {
    override init() {
        super.init()
        var roots = Set<URL>([FileManager.default.homeDirectoryForCurrentUser])
        for url in FileManager.default.mountedVolumeURLs(includingResourceValuesForKeys: nil) ?? [] { roots.insert(url) }
        FIFinderSyncController.default().directoryURLs = roots
    }
    override func menu(for menuKind: FIMenuKind) -> NSMenu? {
        let menu = NSMenu(title: "Codewhale-MedSci")
        let item = NSMenuItem(title: "在 Codewhale-MedSci 中打开", action: #selector(openProject(_:)), keyEquivalent: "")
        item.target = self
        menu.addItem(item)
        return menu
    }
    @objc func openProject(_ sender: Any?) {
        let controller = FIFinderSyncController.default()
        let candidate = controller.selectedItemURLs()?.first ?? controller.targetedURL()
        guard let url = candidate, (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true else { return }
        let application = Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let config = NSWorkspace.OpenConfiguration()
        config.arguments = [url.path]
        config.createsNewApplicationInstance = true // Each invocation owns an independent workspace/session.
        NSWorkspace.shared.openApplication(at: application, configuration: config) { _, _ in }
    }
}
