import AppKit
import SwiftTerm

struct SyntaxPalette: @unchecked Sendable {
    let keyword: NSColor
    let string: NSColor
    let number: NSColor
    let comment: NSColor
    let type: NSColor
    let attribute: NSColor

    static let system = SyntaxPalette(keyword: .systemPink, string: .systemRed, number: .systemPurple, comment: .secondaryLabelColor, type: .systemTeal, attribute: .systemOrange)

    init(keyword: NSColor, string: NSColor, number: NSColor, comment: NSColor, type: NSColor, attribute: NSColor) {
        self.keyword = keyword
        self.string = string
        self.number = number
        self.comment = comment
        self.type = type
        self.attribute = attribute
    }

    init(_ hex: [UInt32]) {
        let colors = hex.map(TerminalTheme.color)
        self.init(keyword: colors[0], string: colors[1], number: colors[2], comment: colors[3], type: colors[4], attribute: colors[5])
    }
}

struct TerminalTheme {
    let id: String
    let name: String
    let isDark: Bool
    let background: UInt32
    let foreground: UInt32
    let ansi: [UInt32]
    var selection: UInt32?
    var syntax: SyntaxPalette = .system

    static let dark = TerminalTheme(
        id: "nucli-dark", name: "Nucli Dark", isDark: true,
        background: 0x0B0B0D,
        foreground: 0xE5E5EA,
        ansi: [
            0x2C2C2E, 0xFF453A, 0x32D74B, 0xFFD60A, 0x0A84FF, 0xBF5AF2, 0x64D2FF, 0xD1D1D6,
            0x636366, 0xFF6961, 0x5DE27A, 0xFFE45C, 0x409CFF, 0xDA8FFF, 0x8FE3FF, 0xFFFFFF,
        ]
    )

    static let light = TerminalTheme(
        id: "nucli-light", name: "Nucli Light", isDark: false,
        background: 0xFFFFFF,
        foreground: 0x1D1D1F,
        ansi: [
            0x1D1D1F, 0xD70015, 0x248A3D, 0xA05A00, 0x0040DD, 0x8944AB, 0x0071A4, 0x8E8E93,
            0x636366, 0xFF3B30, 0x34C759, 0xC69300, 0x007AFF, 0xAF52DE, 0x32ADE6, 0xC7C7CC,
        ]
    )

    private static let vitesseDarkAnsi: [UInt32] = [
        0x393A34, 0xCB7676, 0x4D9375, 0xE6CC77, 0x6394BF, 0xD9739F, 0x5EAAB5, 0xDBD7CA,
        0x777777, 0xCB7676, 0x4D9375, 0xE6CC77, 0x6394BF, 0xD9739F, 0x5EAAB5, 0xFFFFFF,
    ]
    private static let vitesseLightAnsi: [UInt32] = [
        0x121212, 0xAB5959, 0x1E754F, 0xBDA437, 0x296AA3, 0xA13865, 0x2993A3, 0xDBD7CA,
        0xAAAAAA, 0xAB5959, 0x1E754F, 0xBDA437, 0x296AA3, 0xA13865, 0x2993A3, 0xDDDDDD,
    ]
    private static let vitesseDarkSyntax = SyntaxPalette([0x4D9375, 0xC98A7D, 0x4C9A91, 0x758575, 0x5DA994, 0xBD976A])
    private static let vitesseLightSyntax = SyntaxPalette([0x1E754F, 0xB56959, 0x2F798A, 0xA0ADA0, 0x2E8F82, 0xB07D48])

    static let vitesseDark = TerminalTheme(id: "vitesse-dark", name: "Vitesse Dark", isDark: true, background: 0x121212, foreground: 0xDBD7CA, ansi: vitesseDarkAnsi, selection: 0x3A3A3A, syntax: vitesseDarkSyntax)
    static let vitesseDarkSoft = TerminalTheme(id: "vitesse-dark-soft", name: "Vitesse Dark Soft", isDark: true, background: 0x222222, foreground: 0xDBD7CA, ansi: vitesseDarkAnsi, selection: 0x444444, syntax: vitesseDarkSyntax)
    static let vitesseBlack = TerminalTheme(id: "vitesse-black", name: "Vitesse Black", isDark: true, background: 0x000000, foreground: 0xDBD7CA, ansi: vitesseDarkAnsi, selection: 0x2A2A2A, syntax: vitesseDarkSyntax)
    static let vitesseLight = TerminalTheme(id: "vitesse-light", name: "Vitesse Light", isDark: false, background: 0xFFFFFF, foreground: 0x393A34, ansi: vitesseLightAnsi, selection: 0xE4E4E4, syntax: vitesseLightSyntax)
    static let vitesseLightSoft = TerminalTheme(id: "vitesse-light-soft", name: "Vitesse Light Soft", isDark: false, background: 0xF1F0E9, foreground: 0x393A34, ansi: vitesseLightAnsi, selection: 0xDDDCD3, syntax: vitesseLightSyntax)

    static let builtIn = [dark, light, vitesseDark, vitesseDarkSoft, vitesseBlack, vitesseLight, vitesseLightSoft]

    static let automatic: [(id: String, name: String, dark: TerminalTheme, light: TerminalTheme)] = [
        ("nucli", "Nucli (Automatic)", dark, light),
        ("vitesse", "Vitesse (Automatic)", vitesseDark, vitesseLight),
    ]

    @MainActor
    static var isSystemDark: Bool {
        NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    }

    @MainActor
    static func resolve(_ id: String) -> TerminalTheme {
        if let pair = automatic.first(where: { $0.id == id }) { return isSystemDark ? pair.dark : pair.light }
        return builtIn.first { $0.id == id } ?? (isSystemDark ? dark : light)
    }

    @MainActor
    static func isAutomatic(_ id: String) -> Bool {
        automatic.contains { $0.id == id } || !builtIn.contains { $0.id == id }
    }

    @MainActor
    static var current: TerminalTheme { resolve(AppState.shared.theme) }

    @MainActor
    static func forProfile(_ profile: AgentProfile?) -> TerminalTheme {
        guard let id = profile?.theme, !id.isEmpty else { return current }
        return resolve(id)
    }

    var backgroundColor: NSColor { Self.color(background) }
    var foregroundColor: NSColor { Self.color(foreground) }
    var ansiColors: [NSColor] { ansi.map(Self.color) }

    func apply(to terminal: TerminalView) {
        terminal.installColors(ansi.map { Color(red8: UInt16($0 >> 16 & 0xFF), green8: UInt16($0 >> 8 & 0xFF), blue8: UInt16($0 & 0xFF)) })
        terminal.nativeBackgroundColor = Self.color(background)
        terminal.nativeForegroundColor = Self.color(foreground)
        terminal.caretColor = .controlAccentColor
        terminal.selectedTextBackgroundColor = selection.map(Self.color) ?? NSColor.controlAccentColor.withAlphaComponent(0.35)
    }

    static func color(_ hex: UInt32) -> NSColor {
        NSColor(srgbRed: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
    }

    init(id: String, name: String, isDark: Bool, background: UInt32, foreground: UInt32, ansi: [UInt32], selection: UInt32? = nil, syntax: SyntaxPalette = .system) {
        self.id = id
        self.name = name
        self.isDark = isDark
        self.background = background
        self.foreground = foreground
        self.ansi = ansi
        self.selection = selection
        self.syntax = syntax
    }
}
