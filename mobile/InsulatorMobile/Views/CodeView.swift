import SwiftUI

/// One row of code: a line of a file, or of a diff.
struct CodeRow: Hashable {
    enum Tone: Hashable { case add, delete }

    let number: Int?
    let text: String
    var sign: String?
    var tone: Tone?
    /// A diff's unchanged line, drawn softer than its changes.
    var dim = false
    /// A diff's hunk header, across the whole row.
    var hunk = false

    init(number: Int?, text: String) {
        self.number = number
        self.text = text
    }

    init(diff line: DiffLine) {
        switch line.kind {
        case .hunk:
            number = nil
            text = line.text
            hunk = true
        case .add:
            number = line.newNumber
            text = line.text
            sign = "+"
            tone = .add
        case .delete:
            number = line.oldNumber
            text = line.text
            sign = "−"
            tone = .delete
        case .context:
            number = line.newNumber
            text = line.text
            sign = " "
            dim = true
        }
    }
}

/// Lines of code with their numbers (and a diff's signs) in a gutter beside
/// them. Unwrapped, the code pans sideways as one layer while the gutter
/// stays put; wrapped, long lines wrap under their number.
struct CodeView: View {
    let rows: [CodeRow]
    let wrap: Bool

    private static let rowHeight: CGFloat = 20
    private static let hunkHeight: CGFloat = 26
    private static let numberWidth: CGFloat = 44
    private static let signWidth: CGFloat = 20

    var body: some View {
        let signs = rows.contains { $0.sign != nil }
        let gutter = Self.numberWidth + (signs ? Self.signWidth : 0)
        ScrollView(.vertical) {
            if wrap {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                        if row.hunk {
                            hunk(row)
                        } else {
                            HStack(alignment: .top, spacing: 0) {
                                gutterCell(row, signs: signs)
                                    .frame(maxHeight: .infinity, alignment: .top)
                                    .background(gutterTint(row))
                                codeText(row)
                                    .fixedSize(horizontal: false, vertical: true)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .background(rowTint(row))
                        }
                    }
                }
                .padding(.bottom, 40)
            } else {
                HStack(alignment: .top, spacing: 0) {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                            if row.hunk {
                                hunkBand().frame(width: gutter)
                            } else {
                                gutterCell(row, signs: signs)
                                    .frame(height: Self.rowHeight)
                                    .background(gutterTint(row))
                            }
                        }
                    }
                    .frame(width: gutter)
                    ScrollView(.horizontal) {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                                if row.hunk {
                                    hunk(row)
                                } else {
                                    codeText(row)
                                        .lineLimit(1)
                                        .fixedSize()
                                        .frame(height: Self.rowHeight)
                                        .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
                                        .background(rowTint(row))
                                }
                            }
                        }
                        .frame(minWidth: UIScreen.main.bounds.width - gutter, alignment: .leading)
                    }
                    .scrollIndicators(.hidden)
                    .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
                }
                .padding(.bottom, 40)
            }
        }
        .scrollBounceBehavior(.basedOnSize)
    }

    private func gutterCell(_ row: CodeRow, signs: Bool) -> some View {
        HStack(spacing: 0) {
            Text(row.number.map(String.init) ?? "")
                .font(AppTheme.code(size: 11))
                .foregroundStyle(toneText(row) ?? AppTheme.content.opacity(0.35))
                .frame(width: Self.numberWidth - 8, alignment: .trailing)
                .padding(.trailing, 8)
            if signs {
                Text(row.sign ?? "")
                    .font(AppTheme.code(size: 12))
                    .foregroundStyle(toneText(row) ?? .clear)
                    .frame(width: Self.signWidth)
            }
        }
        .frame(minHeight: Self.rowHeight)
    }

    private func codeText(_ row: CodeRow) -> some View {
        Text(row.text.isEmpty ? " " : String(row.text.prefix(1_000)))
            .font(AppTheme.code(size: 12))
            .foregroundStyle(AppTheme.content.opacity(0.8))
            .opacity(row.dim ? 0.7 : 1)
            .padding(.leading, 4)
            .padding(.trailing, 20)
            .frame(minHeight: Self.rowHeight)
    }

    private func hunk(_ row: CodeRow) -> some View {
        Text(row.text)
            .font(AppTheme.code(size: 11))
            .foregroundStyle(AppTheme.content.opacity(0.4))
            .lineLimit(1)
            .padding(.horizontal, 12)
            .frame(maxWidth: .infinity, minHeight: Self.hunkHeight, alignment: .leading)
            .background(AppTheme.content.opacity(0.05))
    }

    private func hunkBand() -> some View {
        AppTheme.content.opacity(0.05).frame(height: Self.hunkHeight)
    }

    private func rowTint(_ row: CodeRow) -> Color {
        switch row.tone {
        case .add: DiffTheme.addRow
        case .delete: DiffTheme.deleteRow
        case nil: .clear
        }
    }

    private func gutterTint(_ row: CodeRow) -> Color {
        switch row.tone {
        case .add: DiffTheme.addGutter
        case .delete: DiffTheme.deleteGutter
        case nil: .clear
        }
    }

    private func toneText(_ row: CodeRow) -> Color? {
        switch row.tone {
        case .add: DiffTheme.addText
        case .delete: DiffTheme.deleteText
        case nil: nil
        }
    }
}

/// A file the agent named, read from the session's working copy on the Mac.
struct FileScreen: View {
    @Bindable var app: AppModel
    let root: String
    let path: String

    @Environment(\.dismiss) private var dismiss
    @AppStorage("code.wrap") private var wrap = false
    @State private var content: String?
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Group {
                if let content {
                    let lines = content.split(separator: "\n", omittingEmptySubsequences: false)
                    CodeView(rows: lines.enumerated().map { CodeRow(number: $0.offset + 1, text: String($0.element)) }, wrap: wrap)
                } else if let error {
                    VStack(spacing: 10) {
                        IconView(.fileText, size: 28).foregroundStyle(AppTheme.muted)
                        Text("Couldn't open \((path as NSString).lastPathComponent)")
                            .font(AppTheme.font(size: 17, weight: .semibold))
                        Text(error)
                            .font(AppTheme.font(size: 14))
                            .foregroundStyle(AppTheme.muted)
                            .multilineTextAlignment(.center)
                    }
                    .padding(28)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .background(AppTheme.sheet.ignoresSafeArea())
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button { dismiss() } label: { IconView(.close, size: 18) }
                        .accessibilityLabel("Close")
                }
                ToolbarItem(placement: .principal) {
                    VStack(spacing: 1) {
                        Text((path as NSString).lastPathComponent)
                            .font(AppTheme.font(size: 16, weight: .semibold))
                        let directory = (path as NSString).deletingLastPathComponent
                        Text(directory.isEmpty ? "/" : directory)
                            .font(AppTheme.font(size: 12))
                            .foregroundStyle(AppTheme.muted)
                    }
                    .lineLimit(1)
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button { wrap.toggle() } label: {
                        Image(systemName: "text.word.spacing")
                            .foregroundStyle(wrap ? AppTheme.accent : AppTheme.content)
                    }
                    .accessibilityLabel(wrap ? "Don't wrap lines" : "Wrap lines")
                }
            }
        }
        .task {
            do {
                content = try await app.readFile(path, root: root)
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}
