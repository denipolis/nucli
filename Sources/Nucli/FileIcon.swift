import AppKit

enum FileIcon {
    private static let code = "chevron.left.forwardslash.chevron.right"

    private static let byExtension: [String: (String, NSColor)] = [
        "swift": ("swift", .systemOrange),
        "md": ("doc.richtext", .systemBlue), "markdown": ("doc.richtext", .systemBlue), "mdx": ("doc.richtext", .systemBlue),
        "sh": ("terminal", .systemGreen), "bash": ("terminal", .systemGreen), "zsh": ("terminal", .systemGreen), "fish": ("terminal", .systemGreen),
        "js": ("curlybraces", .systemYellow), "mjs": ("curlybraces", .systemYellow), "cjs": ("curlybraces", .systemYellow), "jsx": ("curlybraces", .systemCyan),
        "ts": ("curlybraces", .systemBlue), "mts": ("curlybraces", .systemBlue), "tsx": ("curlybraces", .systemCyan),
        "json": ("curlybraces.square", .systemYellow), "jsonc": ("curlybraces.square", .systemYellow),
        "py": (code, .systemBlue), "go": (code, .systemCyan), "rs": (code, .systemOrange), "rb": (code, .systemRed),
        "java": (code, .systemRed), "kt": (code, .systemPurple), "kts": (code, .systemPurple), "scala": (code, .systemRed),
        "c": (code, .systemBlue), "h": (code, .systemPurple), "m": (code, .systemBlue), "mm": (code, .systemBlue),
        "cpp": (code, .systemBlue), "cc": (code, .systemBlue), "hpp": (code, .systemPurple), "cs": (code, .systemPurple),
        "php": (code, .systemIndigo), "dart": (code, .systemCyan), "lua": (code, .systemBlue), "zig": (code, .systemOrange),
        "html": (code, .systemOrange), "htm": (code, .systemOrange), "xml": (code, .systemOrange), "vue": (code, .systemGreen), "svelte": (code, .systemOrange),
        "css": ("paintbrush", .systemBlue), "scss": ("paintbrush", .systemPink), "sass": ("paintbrush", .systemPink), "less": ("paintbrush", .systemBlue),
        "sql": ("cylinder", .systemYellow), "db": ("cylinder", .systemYellow), "sqlite": ("cylinder", .systemYellow),
        "png": ("photo", .systemPurple), "jpg": ("photo", .systemPurple), "jpeg": ("photo", .systemPurple), "gif": ("photo", .systemPurple),
        "webp": ("photo", .systemPurple), "heic": ("photo", .systemPurple), "ico": ("photo", .systemPurple), "icns": ("photo", .systemPurple),
        "tiff": ("photo", .systemPurple), "svg": ("photo", .systemOrange),
        "pdf": ("doc.fill", .systemRed),
        "yml": ("gearshape", .systemPink), "yaml": ("gearshape", .systemPink), "toml": ("gearshape", .systemGray), "ini": ("gearshape", .systemGray),
        "conf": ("gearshape", .systemGray), "cfg": ("gearshape", .systemGray), "plist": ("gearshape", .systemGray), "env": ("gearshape", .systemYellow),
        "lock": ("lock", .systemGray), "resolved": ("lock", .systemGray), "sum": ("lock", .systemGray),
        "zip": ("doc.zipper", .systemBrown), "gz": ("doc.zipper", .systemBrown), "tar": ("doc.zipper", .systemBrown), "tgz": ("doc.zipper", .systemBrown),
        "txt": ("doc.text", .secondaryLabelColor), "log": ("doc.text", .secondaryLabelColor),
        "ex": (code, .systemPurple), "exs": (code, .systemPurple), "erl": (code, .systemRed), "hs": (code, .systemPurple), "elm": (code, .systemCyan),
        "ml": (code, .systemOrange), "mli": (code, .systemOrange), "fs": (code, .systemBlue), "clj": (code, .systemGreen), "cljs": (code, .systemGreen),
        "jl": (code, .systemPurple), "nim": (code, .systemYellow), "cr": (code, .secondaryLabelColor), "gleam": (code, .systemPink),
        "sol": (code, .systemGray), "v": (code, .systemBlue), "odin": (code, .systemBlue), "astro": (code, .systemOrange),
        "graphql": ("point.3.connected.trianglepath.dotted", .systemPink), "gql": ("point.3.connected.trianglepath.dotted", .systemPink),
        "prisma": ("cylinder", .systemIndigo), "proto": (code, .systemGray), "thrift": (code, .systemGray),
        "tf": ("cloud", .systemPurple), "tfvars": ("cloud", .systemPurple), "hcl": ("cloud", .systemPurple), "nix": ("snowflake", .systemBlue),
        "cmake": ("hammer", .systemGreen), "properties": ("gearshape", .systemGray),
    ]

    private static let byName: [String: (String, NSColor)] = [
        ".gitignore": ("arrow.triangle.branch", .systemOrange), ".gitattributes": ("arrow.triangle.branch", .systemOrange),
        ".gitmodules": ("arrow.triangle.branch", .systemOrange), "Dockerfile": ("shippingbox", .systemBlue),
        "Makefile": ("hammer", .systemOrange), "CMakeLists.txt": ("hammer", .systemGreen), "Justfile": ("hammer", .systemOrange), "LICENSE": ("doc.text", .systemYellow), "AGENTS.md": ("sparkles", .systemPurple),
    ]

    static func icon(for name: String, isDirectory: Bool) -> (image: NSImage?, color: NSColor?) {
        if isDirectory { return (NSImage(systemSymbolName: "folder", accessibilityDescription: nil), .secondaryLabelColor) }
        if name == "CLAUDE.md" { return (AgentBadge.image(for: "claude"), nil) }
        let (symbol, color) = byName[name] ?? (name.hasPrefix(".env") ? byExtension["env"] : nil) ?? byExtension[(name as NSString).pathExtension.lowercased()] ?? ("doc", .secondaryLabelColor)
        return (NSImage(systemSymbolName: symbol, accessibilityDescription: nil), color)
    }
}
