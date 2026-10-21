import SwiftUI

/// A parsed piece of Markdown.
indirect enum MarkdownBlock: Hashable, Sendable {
    case paragraph(String)
    case heading(level: Int, text: String)
    case code(String)
    case list(ordered: Bool, start: Int, items: [[MarkdownBlock]])
    case quote([MarkdownBlock])
    case table([[String]])
    case rule
}

/// A small block parser: enough Markdown for agent replies, with inline
/// syntax left to `AttributedString`.
enum MarkdownParser {
    static func parse(_ text: String) -> [MarkdownBlock] {
        parse(lines: text.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n"))
    }

    /// Closes a code fence still open mid-stream, so the half-written block
    /// draws as code rather than as prose.
    static func closingOpenFence(_ text: String) -> String {
        let fences = text.components(separatedBy: "\n").filter { $0.trimmingCharacters(in: .whitespaces).hasPrefix("```") }.count
        return fences % 2 == 1 ? text + "\n```" : text
    }

    private static func parse(lines: [String]) -> [MarkdownBlock] {
        var blocks: [MarkdownBlock] = []
        var index = 0
        var paragraph: [String] = []

        func flushParagraph() {
            let joined = paragraph.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            if !joined.isEmpty { blocks.append(.paragraph(joined)) }
            paragraph = []
        }

        while index < lines.count {
            let line = lines[index]
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                flushParagraph()
                let fence = String(trimmed.prefix(3))
                var code: [String] = []
                index += 1
                while index < lines.count, !lines[index].trimmingCharacters(in: .whitespaces).hasPrefix(fence) {
                    code.append(lines[index])
                    index += 1
                }
                blocks.append(.code(code.joined(separator: "\n")))
                index += 1
                continue
            }
            if trimmed.isEmpty {
                flushParagraph()
                index += 1
                continue
            }
            if let heading = heading(trimmed) {
                flushParagraph()
                blocks.append(heading)
                index += 1
                continue
            }
            if isRule(trimmed) {
                flushParagraph()
                blocks.append(.rule)
                index += 1
                continue
            }
            if trimmed.hasPrefix(">") {
                flushParagraph()
                var quoted: [String] = []
                while index < lines.count {
                    let current = lines[index].trimmingCharacters(in: .whitespaces)
                    guard current.hasPrefix(">") else { break }
                    quoted.append(String(current.dropFirst()).trimmingPrefix(" ").description)
                    index += 1
                }
                blocks.append(.quote(parse(lines: quoted)))
                continue
            }
            if trimmed.hasPrefix("|"), index + 1 < lines.count, isTableDivider(lines[index + 1]) {
                flushParagraph()
                var rows = [cells(trimmed)]
                index += 2
                while index < lines.count, lines[index].trimmingCharacters(in: .whitespaces).hasPrefix("|") {
                    rows.append(cells(lines[index].trimmingCharacters(in: .whitespaces)))
                    index += 1
                }
                blocks.append(.table(rows))
                continue
            }
            if let marker = listMarker(line) {
                flushParagraph()
                var items: [[String]] = []
                let ordered = marker.ordered
                let start = marker.number
                let indent = marker.indent
                while index < lines.count {
                    let current = lines[index]
                    if let next = listMarker(current), next.indent == indent, next.ordered == ordered {
                        items.append([String(current.dropFirst(next.contentStart))])
                        index += 1
                    } else if current.trimmingCharacters(in: .whitespaces).isEmpty {
                        // A blank line ends the list unless the next one carries on.
                        if index + 1 < lines.count, let next = listMarker(lines[index + 1]), next.indent >= indent {
                            index += 1
                        } else {
                            break
                        }
                    } else if leadingSpaces(current) > indent, !items.isEmpty {
                        items[items.count - 1].append(String(current.dropFirst(min(leadingSpaces(current), indent + 2))))
                        index += 1
                    } else {
                        break
                    }
                }
                blocks.append(.list(ordered: ordered, start: start, items: items.map { parse(lines: $0) }))
                continue
            }
            paragraph.append(line)
            index += 1
        }
        flushParagraph()
        return blocks
    }

    private static func heading(_ line: String) -> MarkdownBlock? {
        let hashes = line.prefix { $0 == "#" }.count
        guard (1...6).contains(hashes), line.dropFirst(hashes).first == " " else { return nil }
        return .heading(level: hashes, text: String(line.dropFirst(hashes + 1)).trimmingCharacters(in: CharacterSet(charactersIn: "# ")))
    }

    private static func isRule(_ line: String) -> Bool {
        let compact = line.replacingOccurrences(of: " ", with: "")
        guard compact.count >= 3, let first = compact.first, "-*_".contains(first) else { return false }
        return compact.allSatisfy { $0 == first }
    }

    private static func isTableDivider(_ line: String) -> Bool {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        return trimmed.hasPrefix("|") && trimmed.contains("-") && trimmed.allSatisfy { "|-: ".contains($0) }
    }

    private static func cells(_ line: String) -> [String] {
        var parts = line.split(separator: "|", omittingEmptySubsequences: false).map { $0.trimmingCharacters(in: .whitespaces) }
        if parts.first == "" { parts.removeFirst() }
        if parts.last == "" { parts.removeLast() }
        return parts
    }

    private static func leadingSpaces(_ line: String) -> Int {
        line.prefix { $0 == " " }.count
    }

    private struct Marker {
        let ordered: Bool
        let number: Int
        let indent: Int
        let contentStart: Int
    }

    private static func listMarker(_ line: String) -> Marker? {
        let indent = leadingSpaces(line)
        let rest = line.dropFirst(indent)
        if let first = rest.first, "-*+".contains(first), rest.dropFirst().first == " " {
            return Marker(ordered: false, number: 1, indent: indent, contentStart: indent + 2)
        }
        let digits = rest.prefix { $0.isNumber }
        guard !digits.isEmpty, digits.count < 4 else { return nil }
        let after = rest.dropFirst(digits.count)
        guard let dot = after.first, dot == "." || dot == ")", after.dropFirst().first == " " else { return nil }
        return Marker(ordered: true, number: Int(digits) ?? 1, indent: indent, contentStart: indent + digits.count + 2)
    }
}

/// Opens a file a transcript names, when there is a working copy to read it from.
struct FileOpener: Sendable {
    let open: @MainActor @Sendable (String) -> Void
}

extension EnvironmentValues {
    @Entry var fileOpener: FileOpener?
}

/// An agent reply read as Markdown. While it streams, a word still being
/// written waits until it is whole, and new words fade in over a time that
/// follows the stream's pace.
struct MarkdownView: View {
    let text: String
    var muted = false
    var streaming = false

    @State private var reveal = StreamReveal()
    @Environment(\.fileOpener) private var fileOpener

    var body: some View {
        if streaming {
            TimelineView(.animation(minimumInterval: 1.0 / 30)) { context in
                content(now: context.date)
            }
        } else {
            content(now: nil)
        }
    }

    private func content(now: Date?) -> some View {
        let shown = now.map { reveal.visible(text, at: $0) } ?? text
        let blocks = MarkdownParser.parse(streaming ? MarkdownParser.closingOpenFence(shown) : shown)
        let fade = now.map { reveal.fade(sourceLength: shown.count, at: $0) }
        return VStack(alignment: .leading, spacing: 12) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { index, block in
                BlockView(block: block, muted: muted, fade: index == blocks.count - 1 ? fade : nil)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .environment(\.openURL, OpenURLAction { url in
            if url.scheme == "insulator-file", let opener = fileOpener {
                opener.open(url.path(percentEncoded: false).trimmingPrefix("/").description)
                return .handled
            }
            return .systemAction
        })
    }
}

/// When each stretch of a streamed reply arrived, to fade its words in.
@MainActor
final class StreamReveal {
    private var arrivals: [(length: Int, at: Date)] = []
    private var lastLength = 0
    private var lastChange = Date.distantPast
    private var cadence: TimeInterval = 0.16

    /// The text to show: whole words only, unless the stream pauses mid-word.
    func visible(_ text: String, at now: Date) -> String {
        if text.count != lastLength {
            if text.count > lastLength {
                let gap = now.timeIntervalSince(lastChange)
                if lastChange != .distantPast { cadence = cadence * 0.7 + min(gap, 1) * 0.3 }
                arrivals.append((text.count, now))
            } else {
                arrivals = [(text.count, now)]
            }
            lastLength = text.count
            lastChange = now
        }
        guard now.timeIntervalSince(lastChange) < 0.15, let last = text.last, !last.isWhitespace else { return text }
        guard let cut = text.lastIndex(where: \.isWhitespace) else { return "" }
        return String(text[..<cut])
    }

    /// Each still-fading character's opacity, the last character first.
    func fade(sourceLength: Int, at now: Date) -> [Double] {
        let duration = min(0.4, max(0.12, cadence * 3))
        var alphas: [Double] = []
        // Walk back over the most recent arrivals only; older text is whole.
        var end = sourceLength
        for arrival in arrivals.reversed() {
            let alpha = min(1, now.timeIntervalSince(arrival.at) / duration)
            if alpha >= 1 { break }
            let start = arrivals.last(where: { $0.length < arrival.length })?.length ?? 0
            let count = max(0, min(end, arrival.length) - start)
            alphas.append(contentsOf: Array(repeating: alpha, count: count))
            end = start
        }
        return alphas
    }
}

private struct BlockView: View {
    let block: MarkdownBlock
    let muted: Bool
    /// Opacities for the characters at the end of this block, the last character first.
    var fade: [Double]?

    var body: some View {
        switch block {
        case .paragraph(let text):
            Text(Inline.render(text, muted: muted, fade: fade))
                .font(AppTheme.font(size: 16))
                .lineSpacing(6)
                .textSelection(.enabled)
        case .heading(let level, let text):
            Text(Inline.render(text, muted: muted, fade: fade))
                .font(AppTheme.font(size: muted ? 14 : (level <= 2 ? 18 : 16), weight: .semibold))
                .foregroundStyle(muted ? AppTheme.content.opacity(0.8) : AppTheme.content)
                .padding(.top, 4)
        case .code(let code):
            ScrollView(.horizontal) {
                Text(code)
                    .font(AppTheme.code(size: 13))
                    .lineSpacing(4)
                    .foregroundStyle(AppTheme.content.opacity(0.85))
                    .textSelection(.enabled)
                    .padding(12)
            }
            .scrollIndicators(.hidden)
            .background(AppTheme.content.opacity(0.05), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        case .list(let ordered, let start, let items):
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text(ordered ? "\(start + index)." : "•")
                            .font(AppTheme.font(size: 16))
                            .foregroundStyle(AppTheme.muted)
                            .frame(minWidth: 14, alignment: .leading)
                        VStack(alignment: .leading, spacing: 12) {
                            ForEach(Array(item.enumerated()), id: \.offset) { blockIndex, child in
                                BlockView(
                                    block: child,
                                    muted: muted,
                                    fade: index == items.count - 1 && blockIndex == item.count - 1 ? fade : nil
                                )
                            }
                        }
                    }
                }
            }
        case .quote(let children):
            VStack(alignment: .leading, spacing: 12) {
                ForEach(Array(children.enumerated()), id: \.offset) { _, child in
                    BlockView(block: child, muted: muted)
                }
            }
            .padding(.leading, 12)
            .overlay(alignment: .leading) {
                Rectangle().fill(AppTheme.content.opacity(0.2)).frame(width: 2)
            }
        case .table(let rows):
            TableView(rows: rows, muted: muted)
        case .rule:
            Rectangle().fill(AppTheme.stroke).frame(height: 0.5)
        }
    }
}

private struct TableView: View {
    let rows: [[String]]
    let muted: Bool

    var body: some View {
        let columns = rows.map(\.count).max() ?? 1
        ScrollView(.horizontal) {
            Grid(alignment: .leading, horizontalSpacing: 0, verticalSpacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.offset) { rowIndex, row in
                    if rowIndex > 0 { Divider().overlay(AppTheme.stroke) }
                    GridRow {
                        ForEach(0..<columns, id: \.self) { column in
                            Text(Inline.render(column < row.count ? row[column] : "", muted: muted, fade: nil))
                                .font(AppTheme.font(size: 14, weight: rowIndex == 0 ? .semibold : .regular))
                                .lineSpacing(3)
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                                .frame(minWidth: 140, alignment: .leading)
                        }
                    }
                }
            }
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(AppTheme.stroke, lineWidth: 0.5))
        }
        .scrollIndicators(.hidden)
    }
}

/// Inline Markdown as styled text: code spans in Menlo, file names among
/// them tinted and linked to open in the file viewer.
@MainActor
enum Inline {
    static func render(_ markdown: String, muted: Bool, fade: [Double]?) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        var text = (try? AttributedString(markdown: markdown, options: options)) ?? AttributedString(markdown)
        let ink = muted ? AppTheme.content.opacity(0.65) : AppTheme.content
        text.foregroundColor = ink
        for run in text.runs {
            let range = run.range
            if let intent = run.inlinePresentationIntent, intent.contains(.code) {
                let code = String(text[range].characters)
                text[range].font = AppTheme.code(size: 15)
                if let name = fileName(in: code) {
                    text[range].foregroundColor = AppTheme.fileRef
                    var url = URLComponents()
                    url.scheme = "insulator-file"
                    url.path = "/" + code.trimmingCharacters(in: .whitespaces)
                    text[range].link = url.url
                    _ = name
                } else {
                    text[range].backgroundColor = AppTheme.content.opacity(0.07)
                }
            }
            if run.link != nil, run.inlinePresentationIntent?.contains(.code) != true {
                text[range].foregroundColor = AppTheme.accent
            }
            if let intent = run.inlinePresentationIntent, intent.contains(.strikethrough) {
                text[range].foregroundColor = AppTheme.muted
            }
        }
        if let fade, !fade.isEmpty {
            let characters = text.characters
            var index = characters.endIndex
            for alpha in fade {
                guard index > characters.startIndex else { break }
                let previous = characters.index(before: index)
                text[previous..<index].foregroundColor = ink.opacity(alpha)
                index = previous
            }
        }
        return text
    }

    /// The file a code span names: one token with a plausible extension, a
    /// `:line` or `#L` suffix dropped, as the desktop's file links.
    static func fileName(in value: String) -> String? {
        let text = value.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty, text.count <= 240, !text.contains(where: \.isWhitespace) else { return nil }
        let stripped = text.replacingOccurrences(of: #"(:\d+(:\d+)?|#L\d+(-L\d+)?)$"#, with: "", options: .regularExpression)
        guard let name = stripped.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init),
              name.range(of: #"^[\w%@+().-]+$"#, options: .regularExpression) != nil else { return nil }
        if ["dockerfile", "makefile", "gemfile", "license"].contains(name.lowercased()) { return name }
        guard name.contains("."), let ext = name.split(separator: ".").last,
              ext.range(of: #"^[a-zA-Z][a-zA-Z0-9+-]{0,11}$"#, options: .regularExpression) != nil else { return nil }
        return name
    }
}
