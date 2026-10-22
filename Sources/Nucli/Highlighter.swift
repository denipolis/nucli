import AppKit

struct Language {
    let lineComments: [String]
    let block: (open: String, close: String)?
    let quotes: Set<UInt16>
    let keywords: Set<String>
    let types: Bool
    var headings = false

    private static func words(_ text: String) -> Set<String> {
        Set(text.split(separator: " ").map(String.init))
    }

    private static let common = "true false null nil if else for while do return break continue switch case default try catch finally throw import"

    private static func cStyle(_ extra: String, block: (String, String)? = ("/*", "*/"), quotes: Set<UInt16> = [34, 39, 96]) -> Language {
        Language(lineComments: ["//"], block: block, quotes: quotes, keywords: words(common + " " + extra), types: true)
    }

    private static func hashStyle(_ extra: String, block: (String, String)? = nil) -> Language {
        Language(lineComments: ["#"], block: block, quotes: [34, 39, 96], keywords: words(common + " " + extra), types: true)
    }

    static let swift = cStyle("actor any as associatedtype async await class convenience defer deinit didSet dynamic enum extension fallthrough fileprivate final func get guard in indirect init inout internal is lazy let mutating nonisolated open operator override private protocol public repeat required rethrows self Self some static struct subscript super throws typealias unowned var weak where willSet")
    static let script = cStyle("abstract as async await class const constructor debugger declare delete enum export extends from function get implements in instanceof interface keyof let namespace new of private protected public readonly satisfies set static super this type typeof undefined var void yield")
    static let rust = cStyle("as async await const crate dyn enum extern fn impl in let loop match mod move mut pub ref self Self static struct super trait type unsafe use where", quotes: [34])
    static let go = cStyle("chan const defer fallthrough func go goto interface map package range select struct type var", quotes: [34, 39, 96])
    static let jvm = cStyle("abstract as class companion const data enum extends final fun implements in instanceof interface internal is lateinit new object open override package private protected public sealed static super suspend synchronized this throws val var void when")
    static let c = cStyle("auto char class const constexpr delete double enum explicit extern float friend goto inline int long mutable namespace new noexcept operator private protected public register short signed sizeof static struct template this typedef typename union unsigned using virtual void volatile nullptr self id instancetype")
    static let csharp = cStyle("abstract as async await base bool class const decimal delegate double enum event explicit extern float foreach get in int interface internal is lock long namespace new object operator out override params private protected public readonly record ref sealed set static string struct this typeof using var virtual void where yield")
    static let dart = cStyle("abstract as async await class const covariant dynamic enum extends extension factory final get implements in is late library mixin new on part required set static super sync this typedef var void with yield")
    static let zig = cStyle("const var fn pub struct enum union error defer errdefer comptime inline export extern test unreachable orelse and or usingnamespace", block: nil, quotes: [34, 39])
    static let other = cStyle("as async await class const def enum export extends fn fun func function impl import interface let module mut new pub self static struct this type use val var")
    static let python = hashStyle("and as assert async await class def del elif except from global in is lambda nonlocal not or pass raise self with yield None True False")
    static let ruby = hashStyle("alias and begin class def defined? do elsif end ensure in module next not or redo rescue retry self super then undef unless until when yield")
    static let shell = hashStyle("case do done elif esac export fi function in local readonly select then until declare source alias unset")
    static let elixir = hashStyle("after alias and case cond def defmacro defmodule defp defprotocol defimpl defstruct do end fn in not or quote receive require unless unquote use when with")
    static let config = Language(lineComments: ["#"], block: nil, quotes: [34, 39], keywords: words("true false yes no on off null"), types: false)
    static let hcl = Language(lineComments: ["#", "//"], block: ("/*", "*/"), quotes: [34], keywords: words("resource data variable output module provider locals terraform for in if true false null each count"), types: false)
    static let nix = Language(lineComments: ["#"], block: ("/*", "*/"), quotes: [34], keywords: words("let in with rec inherit import if then else assert true false null"), types: false)
    static let ini = Language(lineComments: [";", "#"], block: nil, quotes: [34, 39], keywords: words("true false yes no on off"), types: false)
    static let lisp = Language(lineComments: [";"], block: nil, quotes: [34], keywords: words("def defn defn- defmacro defprotocol defrecord fn let if when cond do loop recur ns require nil true false"), types: false)
    static let erlang = Language(lineComments: ["%"], block: nil, quotes: [34], keywords: words("after begin case catch end fun if of receive try when module export import"), types: true)
    static let haskell = Language(lineComments: ["--"], block: ("{-", "-}"), quotes: [34], keywords: words("case class data deriving do else if import in instance let module newtype of then type where"), types: true)
    static let ml = Language(lineComments: ["//"], block: ("(*", "*)"), quotes: [34], keywords: words("let rec in match with fun function type module open if then else begin end of val mutable and or not true false"), types: true)
    static let julia = hashStyle("function end module struct mutable abstract using export let local global quote macro begin elseif in where", block: ("#=", "=#"))
    static let sql = Language(lineComments: ["--"], block: ("/*", "*/"), quotes: [34, 39], keywords: words("select from where insert into values update set delete create table alter drop index join left right inner outer on and or not null as group by order limit having primary key references default function end if then else return for do while true false nil in local").union(words("select from where insert into values update set delete create table alter drop index join left right inner outer on and or not null as group by order limit having primary key references default".uppercased())), types: false)
    static let lua = Language(lineComments: ["--"], block: ("--[[", "]]"), quotes: [34, 39], keywords: words("and break do else elseif end false for function goto if in local nil not or repeat return then true until while"), types: false)
    static let json = Language(lineComments: ["//"], block: ("/*", "*/"), quotes: [34], keywords: ["true", "false", "null"], types: false)
    static let graphql = Language(lineComments: ["#"], block: nil, quotes: [34], keywords: words("query mutation subscription fragment on type input enum interface union scalar schema extend implements directive true false null"), types: true)
    static let markup = Language(lineComments: [], block: ("<!--", "-->"), quotes: [34, 39], keywords: [], types: false)
    static let markdown = Language(lineComments: [], block: ("<!--", "-->"), quotes: [96], keywords: [], types: false, headings: true)
    static let css = Language(lineComments: ["//"], block: ("/*", "*/"), quotes: [34, 39], keywords: ["important", "media", "import", "keyframes", "supports"], types: false)

    private static let byExtension: [String: Language] = {
        var map: [String: Language] = [:]
        let groups: [(Language, String)] = [
            (swift, "swift"), (script, "js mjs cjs jsx ts mts cts tsx"), (rust, "rs"), (go, "go"), (jvm, "java kt kts scala sc groovy gradle"),
            (c, "c h m mm cc cpp cxx hpp hh ino metal cu"), (csharp, "cs"), (dart, "dart"), (zig, "zig"),
            (other, "php sol proto thrift v odin gleam prisma nim cr hx fs fsx fsi vala d"),
            (python, "py pyi pyw"), (ruby, "rb rake gemspec"), (shell, "sh bash zsh fish ksh"), (elixir, "ex exs"),
            (config, "yml yaml toml conf cfg env properties dockerignore gitignore"), (hcl, "tf tfvars hcl"), (nix, "nix"), (ini, "ini editorconfig"),
            (lisp, "clj cljs cljc edn el lisp scm rkt"), (erlang, "erl hrl"), (haskell, "hs lhs elm purs"), (ml, "ml mli"), (julia, "jl"),
            (sql, "sql"), (lua, "lua"), (json, "json jsonc json5"), (graphql, "graphql gql"),
            (markup, "html htm xml plist svg vue svelte astro xib storyboard"), (markdown, "md markdown mdx"), (css, "css scss sass less"),
            (Language(lineComments: ["#"], block: nil, quotes: [34, 39], keywords: words("if else endif foreach endforeach function endfunction macro endmacro set project add_executable add_library target_link_libraries include find_package"), types: false), "cmake"),
            (Language(lineComments: ["#"], block: nil, quotes: [34, 39], keywords: words("ifeq ifneq ifdef ifndef else endif include define endef export"), types: false), "mk mak"),
        ]
        for (language, extensions) in groups {
            for ext in extensions.split(separator: " ") { map[String(ext)] = language }
        }
        return map
    }()

    private static let byName: [String: String] = [
        "Makefile": "mk", "GNUmakefile": "mk", "Dockerfile": "sh", "Containerfile": "sh", "Gemfile": "rb", "Rakefile": "rb", "Podfile": "rb",
        "Brewfile": "rb", "Vagrantfile": "rb", "CMakeLists.txt": "cmake", "Justfile": "mk", "justfile": "mk", ".zshrc": "sh", ".bashrc": "sh",
        ".zprofile": "sh", ".profile": "sh", ".gitignore": "gitignore", ".dockerignore": "gitignore", ".editorconfig": "editorconfig",
    ]

    static func forPath(_ path: String) -> Language? {
        let name = (path as NSString).lastPathComponent
        if let ext = byName[name] ?? (name.hasPrefix(".env") || name.hasPrefix("Dockerfile.") ? (name.hasPrefix(".env") ? "env" : "sh") : nil) { return byExtension[ext] }
        return byExtension[(name as NSString).pathExtension.lowercased()]
    }
}

struct Highlighter {
    let language: Language
    let palette: SyntaxPalette
    private var inBlock = false

    init(language: Language, palette: SyntaxPalette) {
        self.language = language
        self.palette = palette
    }

    mutating func spans(in line: Substring) -> [(range: NSRange, color: NSColor)] {
        let u = Array(line.utf16)
        var spans: [(range: NSRange, color: NSColor)] = []
        func add(_ from: Int, _ to: Int, _ color: NSColor) {
            if to > from { spans.append((NSRange(location: from, length: to - from), color)) }
        }
        func matches(_ token: String, at index: Int) -> Bool {
            let t = Array(token.utf16)
            return index + t.count <= u.count && u[index..<index + t.count].elementsEqual(t)
        }
        func find(_ token: String, from index: Int) -> Int? {
            var i = index
            while i < u.count {
                if matches(token, at: i) { return i + token.utf16.count }
                i += 1
            }
            return nil
        }
        if language.headings, !inBlock, u.first == 35 {
            add(0, u.count, palette.keyword)
            return spans
        }
        if language.headings, u.count >= 3, u.prefix(3).allSatisfy({ $0 == 96 }) {
            add(0, u.count, palette.string)
            return spans
        }
        var i = 0
        if inBlock, let block = language.block {
            guard let end = find(block.close, from: 0) else {
                add(0, u.count, palette.comment)
                return spans
            }
            add(0, end, palette.comment)
            inBlock = false
            i = end
        }
        while i < u.count {
            let c = u[i]
            if let block = language.block, matches(block.open, at: i) {
                let from = i
                if let end = find(block.close, from: i + block.open.utf16.count) {
                    i = end
                } else {
                    i = u.count
                    inBlock = true
                }
                add(from, i, palette.comment)
            } else if language.lineComments.contains(where: { matches($0, at: i) }) {
                add(i, u.count, palette.comment)
                break
            } else if language.quotes.contains(c) {
                let from = i
                i += 1
                while i < u.count, u[i] != c { i += u[i] == 92 ? 2 : 1 }
                i = min(i + 1, u.count)
                add(from, i, palette.string)
            } else if Self.isDigit(c), i == 0 || !Self.isWord(u[i - 1]) {
                let from = i
                while i < u.count, Self.isWord(u[i]) || u[i] == 46 { i += 1 }
                add(from, i, palette.number)
            } else if Self.isWordStart(c) || (c == 64 && i + 1 < u.count && Self.isWordStart(u[i + 1])) {
                let from = i
                i += 1
                while i < u.count, Self.isWord(u[i]) { i += 1 }
                if c == 64 {
                    add(from, i, palette.attribute)
                } else if language.keywords.contains(String(decoding: u[from..<i], as: UTF16.self)) {
                    add(from, i, palette.keyword)
                } else if language.types, c >= 65, c <= 90 {
                    add(from, i, palette.type)
                }
            } else {
                i += 1
            }
        }
        return spans
    }

    private static func isDigit(_ c: UInt16) -> Bool { c >= 48 && c <= 57 }
    private static func isWordStart(_ c: UInt16) -> Bool { (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c == 95 || c > 127 }
    private static func isWord(_ c: UInt16) -> Bool { isWordStart(c) || isDigit(c) }
}
